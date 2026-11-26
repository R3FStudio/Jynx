use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn missing_opening_is_generated_once_before_tools_and_later_brief_updates_stay_silent() {
    let root = std::env::temp_dir().join(format!("jynx-silent-tools-{}", Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("fixture.txt"), "synthetic fixture").unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for step in 0..5 {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            let (header_end, length) = loop {
                let mut chunk = [0u8; 4096];
                let count = socket.read(&mut chunk).await.unwrap();
                assert!(count > 0); bytes.extend_from_slice(&chunk[..count]);
                if let Some(offset) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&bytes[..offset]).to_ascii_lowercase();
                    let length = headers.lines().find_map(|line| line.strip_prefix("content-length: ")).unwrap().parse::<usize>().unwrap();
                    break (offset + 4, length);
                }
            };
            while bytes.len() < header_end + length {
                let mut chunk = [0u8; 4096]; let count = socket.read(&mut chunk).await.unwrap();
                assert!(count > 0); bytes.extend_from_slice(&chunk[..count]);
            }
            let request: Value = serde_json::from_slice(&bytes[header_end..header_end + length]).unwrap();
            if step == 1 {
                assert!(request.get("tools").is_none(), "only the missing opening needs a prose generation request");
            } else {
                assert!(!request["tools"].as_array().unwrap().is_empty(), "later silent or brief batches must not generate extra narration");
            }
            if step == 2 { assert!(request["messages"][0]["content"].as_str().unwrap().contains("same file-inspection pass")); }
            if step == 4 {
                let history = request["messages"].to_string();
                assert!(!history.contains("HIDDEN_PROGRESS_CODE"));
                assert!(!history.contains("Let me read another file"));
            }
            let paragraph = "I will inspect the fixture and its configuration together to identify the cause, then make the smallest relevant change and verify the result before reporting back.";
            let message = if step == 1 {
                json!({"content":format!("{paragraph}\n```rust\nHIDDEN_PROGRESS_CODE\n```")})
            } else if step < 4 {
                let content = match step {
                    0 => String::new(),
                    2 => "Let me read another file.".into(),
                    _ => format!("{paragraph} The identifier is `fixture`.\n~~~rust\nHIDDEN_PROGRESS_CODE\n~~~"),
                };
                json!({"content":content,"tool_calls":[{"id":format!("read-{step}"),"type":"function","function":{"name":"read_file","arguments":"{\"path\":\"fixture.txt\"}"}}]})
            } else {
                json!({"content":"","tool_calls":[{"id":"finish","type":"function","function":{"name":"finish_turn","arguments":json!({"status":"completed","answer":"Verified the synthetic fixture.\n```text\nFINAL_CODE_ALLOWED\n```"}).to_string()}}]})
            };
            let data = json!({"choices":[{"message":message}]}).to_string();
            socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{data}", data.len()).as_bytes()).await.unwrap();
        }
    });
    let agent = Agent::new(format!("http://{address}/v1"), "fixture".into(), None, root.to_string_lossy().into(), 32768,
        AgentSettings { permission_mode: ":danger-full-access".into(), final_answer_presentation: true, ..Default::default() }, false, ApprovalGate::default()).unwrap();
    let (events, mut observed) = tokio::sync::mpsc::unbounded_channel();
    let input = vec![ChatMessage {phase:None,role:"user".into(),content:"Read the synthetic fixture twice, then finish.".into(),images:vec![],tool_calls:None,tool_call_id:None,name:None}];
    let result = tokio::time::timeout(Duration::from_secs(60), agent.run(input, events)).await.unwrap().unwrap();
    assert_eq!(result.tools.len(), 3);
    assert!(result.text.contains("FINAL_CODE_ALLOWED"));
    server.await.unwrap();
    let events: Vec<Value> = std::iter::from_fn(|| observed.try_recv().ok()).collect();
    let updates: Vec<_> = events.iter().filter(|event| event["type"] == "text_delta").collect();
    assert_eq!(updates.len(), 2, "one generated opening and one substantial update, no small let-me phases");
    assert!(updates.iter().all(|event| !event.to_string().contains("HIDDEN_PROGRESS_CODE")));
    assert!(updates[1]["text"].as_str().unwrap().contains("`fixture`"));
    let opening = events.iter().position(|event| event["type"] == "text_delta").unwrap();
    for kind in ["tool_queued", "tool_started"] {
        assert!(opening < events.iter().position(|event| event["type"] == kind).unwrap());
    }
    assert!(events.iter().any(|event| event["type"] == "final_answer" && event["text"].as_str().unwrap().contains("FINAL_CODE_ALLOWED")));
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test(start_paused = true)]
async fn quick_reads_edits_and_listings_have_visible_live_events_before_completion() {
    let root = std::env::temp_dir().join(format!("jynx-file-activity-{}", Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("fixture.txt"), "before").unwrap();
    let agent = Agent::new("http://127.0.0.1:1/v1".into(), "fixture".into(), None, root.to_string_lossy().into(), 32768,
        AgentSettings {permission_mode:":danger-full-access".into(),final_answer_presentation:true,..Default::default()},false,ApprovalGate::default()).unwrap();
    for (name, arguments, minimum_ms) in [
        ("read_file", json!({"path":"fixture.txt"}), 11_500),
        ("edit_file", json!({"path":"fixture.txt","old_text":"before","new_text":"after"}), 11_500),
        ("list_files", json!({"path":"."}), 2400),
    ] {
        let (events, mut observed) = tokio::sync::mpsc::unbounded_channel();
        let call = ModelToolCall {id:name.into(),name:name.into(),arguments};
        let start = tokio::time::Instant::now();
        let result = agent.execute_agent_call(&McpCatalog::empty(), &events, &call, name, "File activity", &Some("fixture.txt".into()), "batch", 0, 1).await.unwrap();
        assert_eq!(result.status,"completed");
        assert!(start.elapsed() >= Duration::from_millis(minimum_ms));
        let events:Vec<Value> = std::iter::from_fn(||observed.try_recv().ok()).collect();
        assert!(events.iter().position(|event|event["type"]=="tool_started").unwrap() < events.iter().position(|event|event["type"]=="tool_completed").unwrap());
    }
    assert_eq!(std::fs::read_to_string(root.join("fixture.txt")).unwrap(),"after");
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test(start_paused = true)]
async fn cancelling_a_file_display_interval_does_not_publish_a_late_completion() {
    let root = std::env::temp_dir().join(format!("jynx-file-display-cancel-{}", Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("fixture.txt"), "synthetic").unwrap();
    let agent = Agent::new("http://127.0.0.1:1/v1".into(), "fixture".into(), None, root.to_string_lossy().into(), 32768,
        AgentSettings {permission_mode:":danger-full-access".into(),final_answer_presentation:true,..Default::default()},false,ApprovalGate::default()).unwrap();
    let (events, mut observed) = tokio::sync::mpsc::unbounded_channel();
    let task = tokio::spawn(async move {
        let call = ModelToolCall {id:"read".into(),name:"read_file".into(),arguments:json!({"path":"fixture.txt"})};
        agent.execute_agent_call(&McpCatalog::empty(), &events, &call, "read", "Reading fixture", &Some("fixture.txt".into()), "batch", 0, 1).await
    });
    loop { if observed.recv().await.unwrap()["type"] == "tool_started" { break; } }
    tokio::time::advance(Duration::from_secs(10)).await;
    assert!(!task.is_finished(), "quick file work stays visible for more than ten seconds");
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    tokio::time::advance(Duration::from_secs(30)).await;
    while let Ok(event) = observed.try_recv() { assert_ne!(event["type"], "tool_completed"); }
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn invalid_opening_does_not_cancel_or_replay_terminal_batches() {
    let root=std::env::temp_dir().join(format!("jynx-optional-opening-{}",Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address=listener.local_addr().unwrap();
    let server=tokio::spawn(async move {
        for step in 0..4 {
            let (mut socket,_)=listener.accept().await.unwrap();
            let mut bytes=Vec::new();
            let (header_end,length)=loop {
                let mut chunk=[0u8;4096];let count=socket.read(&mut chunk).await.unwrap();assert!(count>0);bytes.extend_from_slice(&chunk[..count]);
                if let Some(offset)=bytes.windows(4).position(|part|part==b"\r\n\r\n") {
                    let headers=String::from_utf8_lossy(&bytes[..offset]).to_ascii_lowercase();
                    let length=headers.lines().find_map(|line|line.strip_prefix("content-length: ")).unwrap().parse::<usize>().unwrap();
                    break (offset+4,length);
                }
            };
            while bytes.len()<header_end+length {let mut chunk=[0u8;4096];let count=socket.read(&mut chunk).await.unwrap();assert!(count>0);bytes.extend_from_slice(&chunk[..count]);}
            let request:Value=serde_json::from_slice(&bytes[header_end..header_end+length]).unwrap();
            assert_eq!(request.get("tools").is_none(),step==1,"the optional opening is attempted once, never again for later silent batches");
            let message=if step==1 {json!({"content":"```text\nnot a prose update\n```"})}
            else if step==3 {json!({"tool_calls":[{"id":"finish","type":"function","function":{"name":"finish_turn","arguments":json!({"status":"completed","answer":"Verified both terminal batches."}).to_string()}}]})}
            else {json!({"content":"","tool_calls":[{"id":format!("command-{step}"),"type":"function","function":{"name":"run_command","arguments":json!({"command":"sh <<'VERIFY'\nprintf x >> executed.txt\nVERIFY"}).to_string()}}]})};
            let data=json!({"choices":[{"message":message}]}).to_string();
            socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{data}",data.len()).as_bytes()).await.unwrap();
        }
    });
    let agent=Agent::new(format!("http://{address}/v1"),"fixture".into(),None,root.to_string_lossy().into(),32768,
        AgentSettings{permission_mode:":danger-full-access".into(),final_answer_presentation:true,..Default::default()},false,ApprovalGate::default()).unwrap();
    let (events,mut observed)=tokio::sync::mpsc::unbounded_channel();
    let input=vec![ChatMessage{phase:None,role:"user".into(),content:"Run the verification script twice, then finish.".into(),images:vec![],tool_calls:None,tool_call_id:None,name:None}];
    let response=tokio::time::timeout(Duration::from_secs(10),agent.run(input,events)).await.unwrap().unwrap();
    assert_eq!(response.tools.len(),2);
    assert!(response.tools.iter().all(|event|event.status=="completed"));
    assert_eq!(std::fs::read_to_string(root.join("executed.txt")).unwrap(),"xx","each accepted command executes exactly once");
    while let Ok(event)=observed.try_recv(){if event["type"]=="text_delta"{panic!("No model prose was available; do not invent an opening: {event}");}}
    server.await.unwrap();std::fs::remove_dir_all(root).unwrap();
}
