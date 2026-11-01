use jynx_protocol::ChatMessage;
use serde::{Deserialize, Serialize};

const CHECKPOINT_HEADER: &str = "Conversation checkpoint. Preserve these established facts, constraints, decisions, file changes, unresolved issues, and verification results:\n";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionConfig {
    pub context_size: usize,
    pub compact_at_ratio: f32,
    pub retain_recent_messages: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionResult {
    pub messages: Vec<ChatMessage>,
    pub compacted: bool,
}

pub fn estimate_tokens(messages: &[ChatMessage]) -> usize {
    messages
        .iter()
        .map(|message| {



            let mut characters = message.content.chars().count()
                + message.tool_call_id.as_deref().map(str::len).unwrap_or(0)
                + message.name.as_deref().map(str::len).unwrap_or(0);
            if let Some(tool_calls) = &message.tool_calls {
                characters += serde_json::to_string(tool_calls)
                    .map(|value| value.chars().count())
                    .unwrap_or(0);
            }


            characters.div_ceil(2) + 16 + message.images.len() * 2_048
        })
        .sum()
}

pub fn needs_compaction(messages: &[ChatMessage], config: &CompactionConfig) -> bool {
    let threshold = (config.context_size as f32 * config.compact_at_ratio) as usize;
    if estimate_tokens(messages) < threshold { return false; }
    let prefix = messages.first().cloned().filter(|m| m.role == "system");
    let start = usize::from(prefix.is_some());
    let limit = compaction_split_limit(messages);


    limit > start && estimate_tokens(&build_compacted(messages, &prefix, start, limit, compaction_target(config))) < estimate_tokens(messages)
}

fn compaction_split_limit(messages: &[ChatMessage]) -> usize {


    let mut end = messages.len();
    if messages.last().is_some_and(|m| m.role == "user" && !m.images.is_empty()) {
        end = end.saturating_sub(1);
    }
    if end > 0 && messages[end - 1].role == "tool" {
        let mut start = end - 1;
        while start > 0 && messages[start].role == "tool" { start -= 1; }
        if messages[start].tool_calls.is_some() {
            return start;
        }
    }
    messages.len()
}

fn compaction_target(config: &CompactionConfig) -> usize {





    let recovery_ratio = (config.compact_at_ratio * 0.40).clamp(0.16, 0.30);
    ((config.context_size as f32 * recovery_ratio) as usize).max(1)
}

fn checkpoint_base(mut system: ChatMessage) -> ChatMessage {
    if let Some(index) = system.content.find(CHECKPOINT_HEADER) {
        system.content.truncate(index);
        system.content = system.content.trim_end().to_string();
    }
    system
}

fn aligned_split(messages: &[ChatMessage], conversation_start: usize, mut split: usize) -> usize {
    split = split.max(conversation_start).min(messages.len());
    while split > conversation_start
        && messages
            .get(split)
            .is_some_and(|message| message.role.eq_ignore_ascii_case("tool"))
    {
        split -= 1;
    }
    split
}

fn next_valid_split(messages: &[ChatMessage], mut split: usize) -> usize {
    if split >= messages.len() {
        return messages.len();
    }
    split += 1;


    while split < messages.len() && messages[split].role.eq_ignore_ascii_case("tool") {
        split += 1;
    }
    split
}

fn build_compacted(
    messages: &[ChatMessage],
    prefix: &Option<ChatMessage>,
    conversation_start: usize,
    split: usize,
    target: usize,
) -> Vec<ChatMessage> {
    let older = &messages[conversation_start..split];
    let recent = &messages[split..];
    let user_anchor = if recent
        .iter()
        .any(|message| message.role.eq_ignore_ascii_case("user"))
    {
        None
    } else {
        messages[conversation_start..split]
            .iter()
            .rev()
            .find(|message| message.role.eq_ignore_ascii_case("user"))
            .cloned()
    };





    let summary_character_budget = target / 2;
    let mut summary_lines = Vec::new();
    let mut summary_characters = CHECKPOINT_HEADER.chars().count();
    for message in older.iter().rev() {
        let line = message.content.replace('\n', " ");
        let clipped: String = line.chars().take(420).collect();
        let rendered = format!("- {}: {}\n", message.role, clipped);
        let rendered_len = rendered.chars().count();
        if summary_characters + rendered_len > summary_character_budget {
            break;
        }
        summary_characters += rendered_len;
        summary_lines.push(rendered);
    }


    if let Some(system)=prefix {
        if let Some((_,previous))=system.content.split_once(CHECKPOINT_HEADER) {
            for line in previous.lines().rev().filter(|line|line.starts_with("- ")) {
                let rendered=format!("{}\n",line.chars().take(420).collect::<String>());
                let size=rendered.chars().count();
                if summary_characters+size>summary_character_budget {break;}
                summary_characters+=size;summary_lines.push(rendered);
            }
        }
    }
    summary_lines.reverse();
    let mut summary = String::from(CHECKPOINT_HEADER);
    for line in summary_lines {
        summary.push_str(&line);
    }

    let mut output = Vec::new();
    if let Some(system) = prefix.clone() {
        let mut system = checkpoint_base(system);
        if !system.content.trim().is_empty() {
            system.content.push_str("\n\n");
        }
        system.content.push_str(&summary);
        output.push(system);
    } else {
        output.push(ChatMessage {
            role: "system".into(),
            content: summary,
            images: Vec::new(),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        });
    }
    if let Some(user) = user_anchor {
        output.push(user);
    }
    output.extend_from_slice(recent);
    output
}

pub fn compact(messages: &[ChatMessage], config: &CompactionConfig) -> CompactionResult {
    if !needs_compaction(messages, config) {
        return CompactionResult {
            messages: messages.to_vec(),
            compacted: false,
        };
    }
    let target = compaction_target(config);
    let prefix = messages.first().cloned().filter(|m| m.role == "system");
    let conversation_start = usize::from(prefix.is_some());
    let keep = config.retain_recent_messages.min(messages.len());
    let split_limit = compaction_split_limit(messages);
    let mut split = aligned_split(
        messages,
        conversation_start,
        messages.len().saturating_sub(keep).min(split_limit),
    );
    let mut output = build_compacted(messages, &prefix, conversation_start, split, target);




    while estimate_tokens(&output) >= target && split < split_limit {
        split = next_valid_split(messages, split).min(split_limit);
        output = build_compacted(messages, &prefix, conversation_start, split, target);
    }

    CompactionResult {
        compacted: estimate_tokens(&output) < estimate_tokens(messages),
        messages: output,
    }
}


pub fn compact_on_request(messages: &[ChatMessage], context_size: usize) -> CompactionResult {
    compact(messages,&CompactionConfig { context_size:context_size.max(1),compact_at_ratio:0.0,retain_recent_messages:4 })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn message(role: &str, content: &str) -> ChatMessage {
        ChatMessage {
            role: role.into(),
            content: content.into(),
            images: Vec::new(),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }
    }

    fn forced_config(retain_recent_messages: usize) -> CompactionConfig {
        CompactionConfig {
            context_size: 1,
            compact_at_ratio: 0.0,
            retain_recent_messages,
        }
    }

    #[test]
    fn preserves_latest_user_query_during_a_long_tool_turn() {
        let mut messages = vec![
            message("system", "policy"),
            message("user", "Fix the application"),
        ];
        for index in 0..8 {
            messages.push(message("assistant", &format!("tool batch {index}")));
            messages.push(message("tool", &format!("result {index}")));
        }

        let result = compact(&messages, &forced_config(4));
        let users = result
            .messages
            .iter()
            .filter(|message| message.role == "user")
            .collect::<Vec<_>>();

        assert!(result.compacted);
        assert_eq!(users.len(), 1);
        assert_eq!(users[0].content, "Fix the application");
    }

    #[test]
    fn does_not_duplicate_a_user_already_in_the_recent_window() {
        let messages = vec![
            message("system", "policy"),
            message("user", "old request"),
            message("assistant", &"old answer ".repeat(200)),
            message("user", "current request"),
            message("assistant", "working"),
        ];

        let result = compact(&messages, &forced_config(2));
        let users = result
            .messages
            .iter()
            .filter(|message| message.role == "user")
            .collect::<Vec<_>>();

        assert_eq!(users.len(), 1);
        assert_eq!(users[0].content, "current request");
    }

    #[test]
    fn retains_the_assistant_declaration_for_recent_tool_results() {
        let messages = vec![
            message("system", "policy"),
            message("user", "inspect files"),
            message("assistant", &"earlier work ".repeat(1_000)),
            message("assistant", "calling tools"),
            message("tool", "first result"),
            message("tool", "second result"),
        ];

        let result = compact(
            &messages,
            &CompactionConfig {
                context_size: 2_000,
                compact_at_ratio: 0.8,
                retain_recent_messages: 2,
            },
        );
        let first_tool = result
            .messages
            .iter()
            .position(|message| message.role == "tool")
            .expect("tool result retained");

        assert!(first_tool > 0);
        assert_eq!(result.messages[first_tool - 1].role, "assistant");
        assert_eq!(result.messages[first_tool - 1].content, "calling tools");
    }

    #[test]
    fn counts_tool_arguments_without_discarding_the_unread_exchange() {
        let mut assistant = message("assistant", "writing the requested file");
        assistant.tool_calls = Some(json!([{
            "id": "write-1",
            "type": "function",
            "function": {
                "name": "write_file",
                "arguments": { "path": "large.txt", "content": "x".repeat(12_000) }
            }
        }]));
        let messages = vec![
            message("system", "policy"),
            message("user", "create the file"),
            assistant,
            message("tool", "file written"),
        ];
        let config = CompactionConfig {
            context_size: 1_000,
            compact_at_ratio: 0.8,
            retain_recent_messages: 12,
        };

        assert!(estimate_tokens(&messages) >= 800);
        let result = compact(&messages, &config);

        assert!(!result.compacted, "an irreducible latest exchange is not fake compaction work");
        assert_eq!(result.messages.len(), messages.len());
        assert_eq!(result.messages[2].tool_calls, messages[2].tool_calls);
        assert!(result.messages.iter().any(|message| message.role == "user"));
    }

    #[test]
    fn preserves_new_file_and_web_results_even_when_they_exceed_the_recovery_target() {
        let mut messages = vec![message("system", "policy"), message("user", "Inspect the fixture")];
        messages.push(message("assistant", &"older work ".repeat(3_000)));
        let mut declaration = message("assistant", "Reading two sources");
        declaration.tool_calls = Some(json!([
            {"id":"file","type":"function","function":{"name":"read_file","arguments":"{\"path\":\"fixture.txt\"}"}},
            {"id":"page","type":"function","function":{"name":"web_fetch","arguments":"{\"url\":\"https://example.test\"}"}}
        ]));
        messages.push(declaration);
        for (id, kind) in [("file", "read_file"), ("page", "web_fetch")] {
            let mut result = message("tool", &format!("{kind}: BEGIN{}END", "fresh content ".repeat(120)));
            result.tool_call_id = Some(id.into());
            result.name = Some(kind.into());
            messages.push(result);
        }
        let config = CompactionConfig { context_size:4_000, compact_at_ratio:0.72, retain_recent_messages:2 };
        let result = compact(&messages, &config);
        assert!(result.compacted);
        let suffix = &result.messages[result.messages.len()-3..];
        assert_eq!(suffix[0].tool_calls, messages[3].tool_calls);
        assert_eq!(suffix[1].content, messages[4].content);
        assert_eq!(suffix[2].content, messages[5].content);
        assert_eq!(suffix[1].tool_call_id.as_deref(), Some("file"));
        assert!(!needs_compaction(&result.messages, &config), "the protected exchange must not repeatedly announce no-op compaction");
    }

    #[test]
    fn an_oversized_current_prompt_does_not_trigger_a_compaction_animation() {
        let messages = vec![message("system", &"policy ".repeat(2_000)), message("user", "hello")];
        let config = CompactionConfig { context_size:2_000, compact_at_ratio:0.72, retain_recent_messages:12 };
        assert!(!needs_compaction(&messages, &config));
        assert!(!compact(&messages, &config).compacted);
    }

    #[test]
    fn compaction_recovers_well_below_the_trigger_boundary() {
        let mut messages = vec![
            message("system", "policy"),
            message(
                "user",
                "Continue implementing the requested application changes",
            ),
        ];
        for index in 0..40 {
            messages.push(message(
                "assistant",
                &format!(
                    "work item {index}: {}",
                    "implementation detail ".repeat(120)
                ),
            ));
            messages.push(message(
                "tool",
                &format!("tool result {index}: {}", "result detail ".repeat(120)),
            ));
        }
        let config = CompactionConfig {
            context_size: 10_000,
            compact_at_ratio: 0.72,
            retain_recent_messages: 12,
        };

        assert!(estimate_tokens(&messages) >= 7_200);
        let result = compact(&messages, &config);

        assert!(result.compacted);
        assert!(estimate_tokens(&result.messages) <= 3_000);
        assert!(result.messages.iter().any(|message| message.role == "user"));
    }
}
