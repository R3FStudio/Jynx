use jynx_context::estimate_tokens;
use jynx_protocol::ChatMessage;
use serde_json::Value;

#[derive(Default)]
pub(super) struct ContextBudget {
    anchor: Option<(usize, usize)>,
}
impl ContextBudget {
    fn estimate(messages: &[ChatMessage], tools: &Value) -> usize {
        estimate_tokens(messages).saturating_add(tools.to_string().chars().count().div_ceil(2))
    }
    pub fn observe(&mut self, messages: &[ChatMessage], tools: &Value, tokens: usize) {
        self.anchor = Some((Self::estimate(messages, tools), tokens));
    }
    pub fn is_measured(&self) -> bool {
        self.anchor.is_some()
    }
    pub fn reset(&mut self) {
        self.anchor = None;
    }
    pub fn tokens(&self, messages: &[ChatMessage], tools: &Value) -> usize {
        let estimate = Self::estimate(messages, tools);
        match self.anchor {
            Some((anchor, tokens)) if estimate >= anchor => {
                tokens.saturating_add(estimate - anchor)
            }
            _ => estimate,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn real_prompt_usage_replaces_inflated_estimates_but_new_tool_outputs_still_count() {
        let mut messages = vec![ChatMessage {
            phase: None,
            role: "user".into(),
            content: "synthetic source ".repeat(6000),
            images: vec![],
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }];
        let tools = serde_json::json!([]);
        let mut budget = ContextBudget::default();
        assert!(budget.tokens(&messages, &tools) > 40_000);
        assert!(!budget.is_measured());
        budget.observe(&messages, &tools, 11_000);
        assert!(budget.is_measured());
        assert_eq!(budget.tokens(&messages, &tools), 11_000);
        messages.push(ChatMessage {
            phase: None,
            role: "assistant".into(),
            content: "new content ".repeat(1000),
            images: vec![],
            tool_calls: None,
            tool_call_id: None,
            name: None,
        });
        assert!(budget.tokens(&messages, &tools) > 15_000);
        assert!(budget.tokens(&messages, &tools) < 25_000);
        budget.reset();
        assert!(budget.tokens(&messages, &tools) > 40_000);
    }
}

#[cfg(test)]
mod compaction_tests {
    use super::*;
    use crate::{Agent, ApprovalGate};
    use jynx_protocol::AgentSettings;
    use serde_json::json;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };
    #[tokio::test]
    async fn missing_counter_routes_do_not_disable_live_auto_compaction() {
        let root = std::env::temp_dir().join(format!("jynx-live-budget-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        std::fs::write(
            root.join("fixture.txt"),
            "synthetic tool content ".repeat(1000),
        )
        .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let mut calls = 0;
            while calls < 2 {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut bytes = Vec::new();
                let (offset, length, line) = loop {
                    let mut buffer = [0u8; 8192];
                    let count = socket.read(&mut buffer).await.unwrap();
                    assert!(count > 0);
                    bytes.extend_from_slice(&buffer[..count]);
                    if let Some(end) = bytes.windows(4).position(|value| value == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&bytes[..end]).to_ascii_lowercase();
                        let length = headers
                            .lines()
                            .find_map(|line| line.strip_prefix("content-length: "))
                            .map(|length| length.parse::<usize>().unwrap())
                            .unwrap_or(0);
                        break (end + 4, length, headers.lines().next().unwrap().to_owned());
                    }
                };
                while bytes.len() < offset + length {
                    let mut buffer = [0u8; 8192];
                    let count = socket.read(&mut buffer).await.unwrap();
                    assert!(count > 0);
                    bytes.extend_from_slice(&buffer[..count]);
                }
                let (status, body) = if line.starts_with("post /v1/chat/completions ") {
                    let request: Value =
                        serde_json::from_slice(&bytes[offset..offset + length]).unwrap();
                    let message = if calls == 0 {
                        json!({"content":"Read the synthetic fixture.","tool_calls":[{"id":"read","type":"function","function":{"name":"read_file","arguments":"{\"path\":\"fixture.txt\"}"}}]})
                    } else {
                        assert!(
                            !request["messages"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .any(|message| message["content"]
                                    .as_str()
                                    .unwrap_or("")
                                    .starts_with("OLD_SYNTHETIC_CONTEXT"))
                        );
                        json!({"content":"Verified the synthetic fixture.","phase":"final_answer"})
                    };
                    calls += 1;
                    (
                        "200 OK",
                        json!({"choices":[{"message":message,"finish_reason":"stop"}],"usage":{"prompt_tokens":if calls==1{11400}else{7000},"completion_tokens":30}}),
                    )
                } else {
                    ("404 Not Found", json!({}))
                };
                let body = body.to_string();
                socket.write_all(format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            }
        });
        let mut messages = Vec::new();
        for _ in 0..8 {
            messages.push(ChatMessage {
                phase: None,
                role: "user".into(),
                content: "Synthetic earlier request".into(),
                images: vec![],
                tool_calls: None,
                tool_call_id: None,
                name: None,
            });
            messages.push(ChatMessage {
                phase: Some("final_answer".into()),
                role: "assistant".into(),
                content: format!("OLD_SYNTHETIC_CONTEXT{}", " synthetic history".repeat(150)),
                images: vec![],
                tool_calls: None,
                tool_call_id: None,
                name: None,
            });
        }
        messages.push(ChatMessage {
            phase: None,
            role: "user".into(),
            content: "Read fixture.txt and verify it.".into(),
            images: vec![],
            tool_calls: None,
            tool_call_id: None,
            name: None,
        });
        let agent = Agent::new(
            format!("http://{address}/v1"),
            "fixture".into(),
            None,
            root.to_string_lossy().into(),
            16000,
            AgentSettings {
                permission_mode: ":danger-full-access".into(),
                final_answer_presentation: true,
                ..Default::default()
            },
            true,
            ApprovalGate::default(),
        )
        .unwrap();
        let (events, mut observed) = tokio::sync::mpsc::unbounded_channel();
        let response = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            agent.run_with_catalog(messages, events, Some(jynx_mcp::Catalog::empty())),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(response.compacted);
        assert_eq!(response.tools.len(), 1);
        assert_eq!(response.context_used, 7030);
        let events: Vec<_> = std::iter::from_fn(|| observed.try_recv().ok()).collect();
        let updates: Vec<_> = events.iter().filter(|event| event["type"] == "context_update").collect();
        assert_eq!(updates.first().unwrap()["estimated"], true);
        assert_eq!(updates.first().unwrap()["context_reset"], false);
        assert!(!response.context_estimated);
        assert!(updates.iter().any(|event| event["context_reset"] == true));
        assert!(
            events
                .iter()
                .any(|event| event["type"] == "context_compaction" && event["status"] == "started")
        );
        assert!(
            events.iter().any(
                |event| event["type"] == "context_compaction" && event["status"] == "completed"
            )
        );
        server.await.unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }
}
