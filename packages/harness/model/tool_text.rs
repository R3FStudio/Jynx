//! Recover explicitly terminated tool JSON without exposing protocol text.
//! Bare JSON is never promoted into execution, and quoted examples stay literal.
use super::{ModelToolCall, EMULATED_TOOL_CALL_ID};
use serde_json::Value;
use std::sync::atomic::Ordering;

fn key_prefix(raw: &str) -> bool {
    let Some(rest) = raw.strip_prefix('{') else { return false; };
    let rest = rest.trim_start();
    if rest.is_empty() { return true; }
    let Some(rest) = rest.strip_prefix('"') else { return false; };
    let key = rest.split('"').next().unwrap_or_default();
    ["tool_name", "tool_args", "name", "tool", "arguments", "args", "id"]
        .iter().any(|candidate| candidate.starts_with(key) || key == *candidate)
}

fn fields(value: &Value) -> Option<(&str, Option<&Value>)> {
    let name = value.get("tool_name").or_else(|| value.get("name")).or_else(|| value.get("tool"))?.as_str()?;
    let args = value.get("tool_args").or_else(|| value.get("arguments")).or_else(|| value.get("args"));
    args.map(|_| (name, args))
}

pub(super) fn recover(text: &str, streaming: bool) -> (String, Vec<ModelToolCall>) {
    let mut visible = String::new();
    let mut calls = Vec::new();
    let mut cursor = 0;
    let mut inline_ticks = 0;
    let mut fence: Option<(char, usize)> = None;
    while cursor < text.len() {
        let rest = &text[cursor..];
        if cursor == 0 || text.as_bytes()[cursor - 1] == b'\n' {
            let line_len = rest.find('\n').map(|index| index + 1).unwrap_or(rest.len());
            let line = &rest[..line_len];
            let trimmed = line.trim_start();
            let marker = trimmed.chars().next().unwrap_or(' ');
            let count = trimmed.chars().take_while(|ch| *ch == marker).count();
            if let Some((open, length)) = fence {
                if marker == open && count >= length && trimmed[count..].trim().is_empty() { fence = None; }
                visible.push_str(line); cursor += line_len; continue;
            }
            if matches!(marker, '`' | '~') && count >= 3 {
                fence = Some((marker, count)); visible.push_str(line); cursor += line_len; continue;
            }
        }
        let ch = rest.chars().next().unwrap();
        if ch == '`' {
            let count = rest.chars().take_while(|ch| *ch == '`').count();
            if inline_ticks == 0 { inline_ticks = count; } else if inline_ticks == count { inline_ticks = 0; }
            visible.push_str(&rest[..count]); cursor += count; continue;
        }
        if inline_ticks == 0 && ch == '{' {
            let mut values = serde_json::Deserializer::from_str(rest).into_iter::<Value>();
            match values.next() {
                Some(Ok(value)) => {
                    let length = values.byte_offset();
                    if let Some((name, arguments)) = fields(&value) {
                        let tail = &rest[length..];
                        let whitespace = tail.len() - tail.trim_start().len();
                        let suffix = &tail[whitespace..];
                        let close = ["</tool_call>", "</jynx_tool_call>"].into_iter().find(|close| suffix.starts_with(close));
                        if let Some(close) = close {
                            let arguments = match arguments {
                                Some(Value::String(raw)) => serde_json::from_str::<Value>(raw).ok(),
                                Some(value) => Some(value.clone()), _ => None,
                            };
                            if !name.trim().is_empty() && arguments.as_ref().is_some_and(Value::is_object) {
                                let id = EMULATED_TOOL_CALL_ID.fetch_add(1, Ordering::Relaxed);
                                calls.push(ModelToolCall { id: format!("jynx-emulated-{id}"), name: name.trim().into(), arguments: arguments.unwrap() });
                            }
                            cursor += length + whitespace + close.len();
                        } else {
                            // Hold protocol JSON even before the closing tag arrives.
                            // A complete bare object is hidden, never executed.
                            cursor += length;
                        }
                        continue;
                    }
                    visible.push_str(&rest[..length]); cursor += length; continue;
                }
                _ if (streaming && (rest[1..].trim_start().starts_with('"') || rest[1..].trim().is_empty()))
                    || rest.contains("\"tool_name\"") || rest.contains("\"tool_args\"") => {
                    // Metadata may precede the tool name. Hold JSON until it is
                    // complete enough to distinguish prose data from protocol.
                    break;
                }
                _ if streaming && key_prefix(rest) => break,
                _ => {}
            }
        }
        if inline_ticks == 0 {
            if let Some(close) = ["</tool_call>", "</jynx_tool_call>"].into_iter().find(|close| rest.starts_with(close)) {
                cursor += close.len(); continue;
            }
            if streaming && ["</tool_call>", "</jynx_tool_call>"].iter().any(|close| close.starts_with(rest)) { break; }
        }
        visible.push(ch); cursor += ch.len_utf8();
    }
    (visible, calls)
}

#[cfg(test)]
mod tests {
    use super::*;
    const RAW: &str = "{\"tool_name\":\"read_file\",\"tool_args\":{\"path\":\"Cargo.toml\"}}";
    #[test]
    fn missing_open_tag_recovers_only_a_complete_explicitly_terminated_call() {
        let (text,calls)=recover(&format!("Inspecting.\n{RAW} </tool_call>\nNext."), false);
        assert_eq!(text.trim(),"Inspecting.\n\nNext.");
        assert_eq!(calls.len(),1);assert_eq!(calls[0].name,"read_file");assert_eq!(calls[0].arguments["path"],"Cargo.toml");
        let (text,calls)=recover(RAW,false);assert!(text.is_empty());assert!(calls.is_empty());
        let (_,calls)=recover("{\"tool_name\":\"write_file\",\"tool_args\":\"broken\"}</tool_call>",false);assert!(calls.is_empty());
    }
    #[test]
    fn every_stream_prefix_holds_raw_protocol_including_partial_keys_and_closing_tags() {
        let raw=format!("{RAW} </tool_call>");
        for end in 1..=raw.len() {
            let (text,_)=recover(&raw[..end],true);
            assert!(text.trim().is_empty(),"leaked prefix {end}: {text:?}");
        }
    }
    #[test]
    fn ordinary_json_and_quoted_examples_are_not_calls() {
        for text in [format!("Example: `{RAW} </tool_call>`"),format!("```json\n{RAW}\n```"),"{\"name\":\"ordinary configuration\",\"version\":1}".into(),"A literal brace { in prose.".into()] {
            let (visible,calls)=recover(&text,false);assert_eq!(visible,text);assert!(calls.is_empty());
        }
    }
    #[test]
    fn shared_stream_never_publishes_tool_json_and_native_duplicates_execute_once() {
        use super::super::{ModelClient, ModelReply};
        use std::collections::BTreeMap;
        for protocol in [format!("{RAW} </tool_call>"),format!("<tool_call>{RAW}</tool_call>"),format!("{{\"type\":\"function\",\"tool_name\":\"read_file\",\"tool_args\":{{\"path\":\"Cargo.toml\"}}}} </tool_call>")] {
            let raw=format!("Inspecting café.\n{protocol}\nI will verify the manifest.");
            let (events,mut observed)=tokio::sync::mpsc::unbounded_channel();
            let mut text=String::new();let mut calls=BTreeMap::new();
            for ch in raw.chars() {
                ModelClient::process_stream_value(&serde_json::json!({"choices":[{"delta":{"content":ch.to_string()}}]}),&mut text,&mut calls,&events);
                while let Ok(event)=observed.try_recv() {
                    if event["type"]=="text_delta" {
                        let visible=event["accumulated_text"].as_str().unwrap_or_default();
                        assert!(!visible.contains('{')&&!visible.contains('<'),"protocol escaped to a client: {visible}");
                    }
                }
            }
            let reply=ModelClient::normalize_tool_markup(ModelReply {phase:None,input_tokens:None,output_tokens:None,finish_reason:None,text,tool_calls:vec![ModelToolCall{id:"native".into(),name:"read_file".into(),arguments:serde_json::json!({"path":"Cargo.toml"})}]});
            assert_eq!(reply.tool_calls.len(),1);assert_eq!(reply.tool_calls[0].id,"native");
            assert!(reply.text.contains("Inspecting café."));assert!(!reply.text.contains("tool_args"));assert!(!reply.text.contains("</tool_call>"));
        }
    }

}
