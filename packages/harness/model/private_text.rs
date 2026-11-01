use regex::Regex;
use serde_json::Value;
use std::sync::LazyLock;

static TAG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^<\s*(/?)\s*(think|thinking|analysis|reasoning)(?:\s[^<>]*)?\s*>").unwrap()
});
const PRIVATE_CHANNELS: [&str; 3] = ["analysis", "reasoning", "thinking"];
const HEADERS: [&str; 4] = [
    "<|start|>assistant",
    "<|im_start|>assistant",
    "<|channel|>",
    "<|meta_sep|>",
];
const ENDINGS: [&str; 5] = [
    "<|end|>",
    "<|im_end|>",
    "<|eot_id|>",
    "<|return|>",
    "<|fim_suffix|>",
];

pub fn private_channel(value: &str) -> bool {
    PRIVATE_CHANNELS.contains(&value.trim().to_ascii_lowercase().as_str())
}
pub fn private_part(value: &Value) -> bool {
    let kind = value["type"].as_str().unwrap_or("").to_ascii_lowercase();
    kind.starts_with("reasoning")
        || matches!(
            kind.as_str(),
            "thinking"
                | "redacted_thinking"
                | "analysis"
                | "analysis_text"
                | "summary_text"
                | "internal_reasoning"
        )
        || value["channel"].as_str().is_some_and(private_channel)
        || value["role"].as_str().is_some_and(private_channel)
}

fn literal_end(rest: &str) -> Option<usize> {
    let delimiter = *rest.as_bytes().first()?;
    if delimiter != b'`' && delimiter != b'~' {
        return None;
    }
    let size = rest.bytes().take_while(|byte| *byte == delimiter).count();
    if delimiter == b'~' && size < 3 {
        return None;
    }
    let marker = &rest[..size];
    Some(
        rest[size..]
            .find(marker)
            .map(|offset| size + offset + size)
            .unwrap_or(rest.len()),
    )
}



pub fn public_text(text: &str, streaming: bool) -> String {
    let mut visible = String::new();
    let mut cursor = 0;
    let mut tags: Vec<String> = vec![];
    let mut channel_private = false;
    let mut seen_control = false;
    while cursor < text.len() {
        let rest = &text[cursor..];
        if tags.is_empty() && !channel_private {
            if let Some(end) = literal_end(rest) {
                visible.push_str(&rest[..end]);
                cursor += end;
                continue;
            }
        }
        if rest.starts_with('<') {
            if let Some(header) = HEADERS.iter().find(|header| rest.starts_with(**header)) {
                let after = &rest[header.len()..];
                let end = ["<|message|>", "<|im_sep|>", "\n"]
                    .iter()
                    .filter_map(|delimiter| {
                        after.find(delimiter).map(|index| (index, delimiter.len()))
                    })
                    .min_by_key(|(index, _)| *index);
                let Some((index, length)) = end else {
                    break;
                };
                let metadata = &after[..index];
                let channel = if header.ends_with("assistant") {
                    metadata
                        .split_once("<|channel|>")
                        .or_else(|| metadata.split_once("<|meta_sep|>"))
                        .map(|(_, channel)| channel.trim().split_whitespace().next().unwrap_or(""))
                } else {
                    Some(metadata.trim().split_whitespace().next().unwrap_or(""))
                };
                channel_private = channel.is_some_and(private_channel);
                seen_control = true;
                cursor += header.len() + index + length;
                continue;
            }
            if let Some(ending) = ENDINGS.iter().find(|ending| rest.starts_with(**ending)) {
                seen_control = true;
                cursor += ending.len();
                continue;
            }
            if let Some(found) = TAG.captures(rest) {
                let closing = !found[1].is_empty();
                let kind = found[2].to_ascii_lowercase();
                if closing {
                    if let Some(index) = tags.iter().rposition(|tag| tag == &kind) {
                        tags.truncate(index);
                    } else if !seen_control {
                        visible.clear();
                    }
                } else {
                    tags.push(kind);
                }
                seen_control = true;
                cursor += found[0].len();
                continue;
            }
            if streaming {
                let lower = rest.to_ascii_lowercase();
                let partial = [
                    "<think>",
                    "</think>",
                    "<thinking>",
                    "</thinking>",
                    "<analysis>",
                    "</analysis>",
                    "<reasoning>",
                    "</reasoning>",
                ];
                if partial.iter().any(|marker| marker.starts_with(&lower))
                    || HEADERS
                        .iter()
                        .chain(ENDINGS.iter())
                        .chain(["<|message|>", "<|im_sep|>"].iter())
                        .any(|marker| marker.starts_with(rest))
                {
                    break;
                }
                if !rest.contains('>')
                    && ["<think ", "<thinking ", "<analysis ", "<reasoning "]
                        .iter()
                        .any(|prefix| lower.starts_with(prefix))
                {
                    break;
                }
            }
        }
        let ch = rest.chars().next().unwrap();
        if tags.is_empty() && !channel_private {
            visible.push(ch);
        }
        cursor += ch.len_utf8();
    }
    visible
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_private_formats_do_not_leak() {
        for text in [
            "<THINK>PRIVATE</THINK>Answer",
            "<think data-mode='private'>PRIVATE</think>Answer",
            "<reasoning>PRIVATE</reasoning>Answer",
            "PRIVATE from prefill</think>Answer",
            "<think>PRIVATE<thinking>nested</thinking>PRIVATE</think>Answer",
            "<|start|>assistant<|channel|>analysis<|message|>PRIVATE<|end|><|start|>assistant<|channel|>final<|message|>Answer<|return|>",
            "<|channel|>analysis<|message|>PRIVATE<|end|><|channel|>final<|message|>Answer",
        ] {
            assert_eq!(public_text(text, false), "Answer", "{text}");
        }
        assert_eq!(public_text("<think>PRIVATE unfinished", false), "");
    }
    #[test]
    fn markers_split_at_every_boundary_are_held_and_examples_stay_literal() {
        for tag in [
            "<think>",
            "<reasoning>",
            "<|start|>assistant<|channel|>analysis<|message|>",
        ] {
            for index in 1..=tag.len() {
                assert_eq!(public_text(&tag[..index], true), "");
            }
        }
        let sample = "The user said to show `<think>` and `</think>`.\n```xml\n<analysis>example</analysis>\n```";
        assert_eq!(public_text(sample, false), sample);
    }
}
