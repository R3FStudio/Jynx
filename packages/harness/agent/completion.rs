use anyhow::{Result, anyhow};
use jynx_model::ModelReply;
use serde_json::{Value, json};

pub(super) const FINISH_TOOL: &str = "finish_turn";
pub(super) const PROTOCOL_GUIDANCE: &str = "[Jynx turn protocol] Continue with the next tool call. When the request is complete or blocked, call finish_turn alone with the final answer. Ordinary progress text is not a completion signal.";

pub(super) enum TurnDecision {
    Tools,
    Continue,
    Finish(String),
}

pub(super) fn schema() -> Value {
    json!({"type":"function","function":{
        "name":FINISH_TOOL,
        "description":"End the current user turn only when the request is complete or a concrete blocker prevents further work. Call alone with the final user-facing answer. Use ask_user for questions. Progress messages and successful tools do not end the turn.",
        "parameters":{"type":"object","properties":{
            "answer":{"type":"string","minLength":1},
            "status":{"type":"string","enum":["completed","blocked"]}
        },"required":["answer","status"],"additionalProperties":false}
    }})
}

pub(super) fn decide(reply: &ModelReply, requires_completion: bool) -> Result<TurnDecision> {
    if let Some(call) = reply
        .tool_calls
        .iter()
        .find(|call| call.name == FINISH_TOOL)
    {
        if !requires_completion || reply.tool_calls.len() != 1 {
            return Err(anyhow!(
                "finish_turn must be the only call in its response and tools must be enabled; no calls in this response were executed"
            ));
        }
        let status = call.arguments["status"].as_str();
        let answer = call.arguments["answer"]
            .as_str()
            .filter(|answer| !answer.trim().is_empty());
        if !matches!(status, Some("completed" | "blocked")) || answer.is_none() {
            return Err(anyhow!(
                "finish_turn requires a nonempty answer and status completed or blocked"
            ));
        }
        return Ok(TurnDecision::Finish(answer.unwrap().to_owned()));
    }
    if !reply.tool_calls.is_empty() {
        return Ok(TurnDecision::Tools);
    }
    match reply.phase.as_deref() {
        Some("final_answer") if !reply.text.trim().is_empty() => {
            Ok(TurnDecision::Finish(reply.text.clone()))
        }
        None if !requires_completion && !reply.text.trim().is_empty() => {
            Ok(TurnDecision::Finish(reply.text.clone()))
        }
        _ => Ok(TurnDecision::Continue),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jynx_model::ModelToolCall;
    fn reply(text: &str, phase: Option<&str>) -> ModelReply {
        ModelReply {
            input_tokens: None, output_tokens: None,
            text: text.into(),
            phase: phase.map(str::to_owned),
            tool_calls: vec![],
            finish_reason: Some("stop".into()),
        }
    }
    #[test]
    fn prose_never_decides_whether_an_agent_turn_is_complete() {
        for text in [
            "Next, I will examine the remaining modules.",
            "The work is done.",
            "arbitrary fixture words",
            "I",
        ] {
            assert!(matches!(
                decide(&reply(text, None), true).unwrap(),
                TurnDecision::Continue
            ));
            assert!(matches!(
                decide(&reply(text, Some("commentary")), true).unwrap(),
                TurnDecision::Continue
            ));
            assert!(matches!(
                decide(&reply(text, Some("final_answer")), true).unwrap(),
                TurnDecision::Finish(_)
            ));
        }
        assert!(matches!(
            decide(&reply("A normal chat answer.", None), false).unwrap(),
            TurnDecision::Finish(_)
        ));
    }
    #[test]
    fn explicit_completion_is_validated_and_cannot_skip_pending_tools() {
        let mut value = reply("", None);
        value.tool_calls.push(ModelToolCall {
            id: "finish".into(),
            name: FINISH_TOOL.into(),
            arguments: json!({"status":"completed","answer":"Verified the synthetic change."}),
        });
        assert!(matches!(
            decide(&value, true).unwrap(),
            TurnDecision::Finish(_)
        ));
        value.tool_calls.push(ModelToolCall {
            id: "read".into(),
            name: "read_file".into(),
            arguments: json!({"path":"fixture.txt"}),
        });
        assert!(decide(&value, true).is_err());
        value.tool_calls.remove(0);
        value.phase = Some("final_answer".into());
        assert!(matches!(decide(&value, true).unwrap(), TurnDecision::Tools));
        value.tool_calls[0] = ModelToolCall {
            id: "invalid".into(),
            name: FINISH_TOOL.into(),
            arguments: json!({"answer":""}),
        };
        assert!(decide(&value, true).is_err());
    }
}

#[cfg(test)]
mod turn_tests {
    use super::*;
    use crate::{Agent, ApprovalGate};
    use jynx_mcp::Catalog as McpCatalog;
    use jynx_protocol::{AgentSettings, ChatMessage};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};

    async fn read_request(socket: &mut TcpStream) -> Value {
        let mut bytes = Vec::new();
        let (offset, length) = loop {
            let mut buffer = [0u8; 4096];
            let count = socket.read(&mut buffer).await.unwrap();
            assert!(count > 0);
            bytes.extend_from_slice(&buffer[..count]);
            if let Some(end) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&bytes[..end]).to_ascii_lowercase();
                let length = headers
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length: "))
                    .unwrap()
                    .parse::<usize>()
                    .unwrap();
                break (end + 4, length);
            }
        };
        while bytes.len() < offset + length {
            let mut buffer = [0u8; 4096];
            let count = socket.read(&mut buffer).await.unwrap();
            assert!(count > 0);
            bytes.extend_from_slice(&buffer[..count]);
        }
        serde_json::from_slice(&bytes[offset..offset + length]).unwrap()
    }
    async fn respond(socket: &mut TcpStream, message: Value) {
        let body = json!({"choices":[{"message":message,"finish_reason":"stop"}]}).to_string();
        socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
    }
    fn input() -> Vec<ChatMessage> {
        vec![ChatMessage {
            phase: None,
            role: "user".into(),
            content: "Read the synthetic fixture and verify its contents.".into(),
            images: vec![],
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }]
    }
    #[tokio::test]
    async fn native_commentary_and_unphased_text_both_continue_until_explicit_completion() {
        for native in [false, true] {
            let root =
                std::env::temp_dir().join(format!("jynx-turn-protocol-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir(&root).unwrap();
            std::fs::write(root.join("fixture.txt"), "SYNTHETIC_VERIFIED_CONTENT").unwrap();
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let server = tokio::spawn(async move {
                for step in 0..3 {
                    let (mut socket, _) = listener.accept().await.unwrap();
                    let request = read_request(&mut socket).await;
                    assert!(
                        request["tools"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|tool| tool["function"]["name"] == FINISH_TOOL)
                    );
                    if step == 1 {
                        let messages = request["messages"].as_array().unwrap();
                        if native {
                            assert!(
                                messages
                                    .iter()
                                    .any(|message| message["phase"] == "commentary")
                            );
                        } else {
                            assert!(
                                messages
                                    .iter()
                                    .any(|message| message["content"] == PROTOCOL_GUIDANCE)
                            );
                        }
                    }
                    if step == 2 {
                        assert!(
                            request["messages"].as_array().unwrap().iter().any(
                                |message| message["role"] == "tool"
                                    && message["content"]
                                        .as_str()
                                        .unwrap_or("")
                                        .contains("SYNTHETIC_VERIFIED_CONTENT")
                            )
                        );
                    }
                    let message = match step {
                        0 => {
                            let mut value = json!({"content":"All done. Arbitrary prose is not a lifecycle event."});
                            if native {
                                value["phase"] = json!("commentary");
                            }
                            value
                        }
                        1 => {
                            json!({"content":"Reading the synthetic fixture.","tool_calls":[{"id":"read","type":"function","function":{"name":"read_file","arguments":"{\"path\":\"fixture.txt\"}"}}]})
                        }
                        _ => {
                            json!({"content":"","tool_calls":[{"id":"finish","type":"function","function":{"name":FINISH_TOOL,"arguments":json!({"status":"completed","answer":"Verified the synthetic fixture."}).to_string()}}]})
                        }
                    };
                    respond(&mut socket, message).await;
                }
            });
            let agent = Agent::new(
                format!("http://{address}/v1"),
                "fixture".into(),
                None,
                root.to_string_lossy().into(),
                32768,
                AgentSettings {
                    final_answer_presentation: true,
                    permission_mode: ":danger-full-access".into(),
                    ..Default::default()
                },
                false,
                ApprovalGate::default(),
            )
            .unwrap();
            let (events, mut observed) = tokio::sync::mpsc::unbounded_channel();
            let response = tokio::time::timeout(
                std::time::Duration::from_secs(10),
                agent.run_with_catalog(input(), events, Some(McpCatalog::empty())),
            )
            .await
            .unwrap()
            .unwrap();
            assert_eq!(response.text, "Verified the synthetic fixture.");
            assert_eq!(response.tools.len(), 1);
            assert_eq!(response.tools[0].kind, "read_file");
            let saved = response.context_messages.unwrap();
            assert_eq!(saved.last().unwrap().phase.as_deref(), Some("final_answer"));
            let events: Vec<_> = std::iter::from_fn(|| observed.try_recv().ok()).collect();
            assert_eq!(
                events
                    .iter()
                    .filter(|event| event["type"] == "final_answer")
                    .count(),
                1
            );
            assert!(!events.iter().any(|event| event["kind"] == FINISH_TOOL));
            server.await.unwrap();
            std::fs::remove_dir_all(root).unwrap();
        }
    }
    #[tokio::test]
    async fn missing_completion_signals_fail_explicitly_without_false_final_answers() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            for _ in 0..4 {
                let (mut socket, _) = listener.accept().await.unwrap();
                read_request(&mut socket).await;
                respond(&mut socket, json!({"content":"The task is complete."})).await;
            }
        });
        let agent = Agent::new(
            format!("http://{address}/v1"),
            "fixture".into(),
            None,
            std::env::temp_dir().to_string_lossy().into(),
            32768,
            AgentSettings {
                final_answer_presentation: true,
                ..Default::default()
            },
            false,
            ApprovalGate::default(),
        )
        .unwrap();
        let (events, mut observed) = tokio::sync::mpsc::unbounded_channel();
        let error = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            agent.run_with_catalog(input(), events, Some(McpCatalog::empty())),
        )
        .await
        .unwrap()
        .unwrap_err();
        assert!(error.to_string().contains("completion signal"));
        assert!(
            !std::iter::from_fn(|| observed.try_recv().ok())
                .any(|event| event["type"] == "final_answer")
        );
        server.await.unwrap();
    }
}
