use jynx_security::wrap_tool_output;
use jynx_protocol::ChatMessage;
use serde_json::{Value, json};




pub(super) fn model_tool_output(kind: &str, output: &str, budget: usize) -> String {
    let mut payload = serde_json::from_str::<Value>(output)
        .unwrap_or_else(|_| Value::String(output.into()));
    if kind == "web_search" && payload["results"].is_array() {


        if let Some(sources) = payload["sources"].as_array() {
            let diagnostics: Vec<Value> = sources.iter().map(|source| json!({
                "query":source["query"], "status":source["status"],
                "error":source["error"],
            })).collect();
            payload["sources"] = json!(diagnostics);
        }
    }
    let serialized = payload.to_string();
    if serialized.chars().count() > budget {
        let content_key = ["content", "output", "stdout"].into_iter()
            .find(|key| payload[*key].is_string());
        if let Some(key) = content_key {
            let body = payload[key].as_str().unwrap().to_owned();
            payload[key] = Value::String(String::new());
            let overhead = payload.to_string().chars().count() + 160;
            payload[key] = Value::String(excerpt(&body, budget.saturating_sub(overhead).max(128)));
            payload["model_excerpt"] = json!(true);
            payload["original_content_characters"] = json!(body.chars().count());
        }
        if payload.to_string().chars().count() > budget + 256 {
            payload = json!({
                "model_excerpt":true,
                "original_output_characters":serialized.chars().count(),
                "excerpt":excerpt(&serialized, budget.max(128)),
            });
        }
    }
    wrap_tool_output(&format!("tool:{kind}"), &payload.to_string())
}



pub(super) fn restore_model_tool_outputs(mut messages: Vec<ChatMessage>, context_size: usize) -> Vec<ChatMessage> {
    let mut batch_size=1;
    for message in &mut messages {
        if let Some(calls)=message.tool_calls.as_ref().and_then(Value::as_array) { batch_size=calls.len().max(1); }
        if message.role != "tool" { continue; }
        let Ok(wrapper)=serde_json::from_str::<Value>(&message.content) else { continue; };
        if wrapper["trust"] != "untrusted_tool_output" { continue; }
        let Some(payload)=wrapper.get("content") else { continue; };
        let kind=message.name.as_deref().or_else(||wrapper["source"].as_str().and_then(|value|value.strip_prefix("tool:"))).unwrap_or("tool");
        let budget=(context_size.saturating_mul(2)/5/batch_size).clamp(256,40_000);
        if payload["model_excerpt"] == true && payload.to_string().chars().count() <= budget + 256 { continue; }
        message.content=model_tool_output(kind,&payload.to_string(),budget);
    }
    messages
}

fn excerpt(text: &str, limit: usize) -> String {
    let count = text.chars().count();
    if count <= limit { return text.into(); }
    let head = limit * 3 / 4;
    let tail = limit - head;
    let beginning: String = text.chars().take(head).collect();
    let ending: String = text.chars().skip(count - tail).collect();
    format!("{beginning}\n[Model excerpt: {} characters omitted; request a narrower range for the missing content.]\n{ending}", count - limit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_and_web_content_are_structured_and_not_double_encoded() {
        for kind in ["read_file", "web_fetch"] {
            let output = json!({"path":"fixture.txt","url":"https://example.test","content":"Real fixture content\nsecond line"}).to_string();
            let delivered: Value = serde_json::from_str(&model_tool_output(kind, &output, 4_000)).unwrap();
            assert_eq!(delivered["content"]["content"], "Real fixture content\nsecond line");
            assert_eq!(delivered["trust"], "untrusted_tool_output");
        }
    }

    #[test]
    fn large_unicode_read_keeps_visible_content_and_an_explicit_notice() {
        let original = json!({"path":"fixture.txt","content":format!("BEGIN{}END", "星".repeat(30_000)),"next_start_line":200});
        let delivered: Value = serde_json::from_str(&model_tool_output("read_file", &original.to_string(), 2_000)).unwrap();
        let body = delivered["content"]["content"].as_str().unwrap();
        assert!(body.starts_with("BEGIN"));
        assert!(body.ends_with("END"));
        assert!(body.contains("characters omitted"));
        assert_eq!(delivered["content"]["path"], "fixture.txt");
        assert_eq!(delivered["content"]["next_start_line"], 200);
        assert!(delivered.to_string().chars().count() < 2_500);
        assert_eq!(original["content"].as_str().unwrap().chars().count(), 30_008);
    }

    #[test]
    fn searches_deliver_results_once_and_preserve_query_errors() {
        let output = json!({"results":[{"url":"https://example.test","snippet":"A real result"}],"sources":[{"query":"fixture","status":200,"results":[{"snippet":"A real result"}]},{"query":"failed","error":"timeout"}]}).to_string();
        let delivered = model_tool_output("web_search", &output, 4_000);
        assert_eq!(delivered.matches("A real result").count(), 1);
        assert!(delivered.contains("timeout"));
        assert!(delivered.contains("https://example.test"));
    }
}
