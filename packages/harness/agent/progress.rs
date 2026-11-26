use jynx_protocol::ToolEvent;

pub(super) fn is_successful_inspection(event: &ToolEvent) -> bool {
    event.status == "completed"
        && matches!(event.kind.as_str(), "read_file" | "read_skill_file" | "list_files" | "list_dir" | "search_files" | "find_files" | "workspace_symbols")
}

pub(super) async fn optional_opening(
    request: impl std::future::Future<Output = anyhow::Result<String>>,
) -> Option<String> {
    tokio::time::timeout(std::time::Duration::from_secs(15), request)
        .await.ok().and_then(Result::ok)
        .map(|text| public_update(&text, true))
        .filter(|text| !text.is_empty())
}



pub(super) fn public_update(text: &str, allow_brief: bool) -> String {
    let mut fence: Option<(char, usize)> = None;
    let mut prose = Vec::new();
    let mut paragraph_break = false;
    for line in text.lines() {
        let trimmed = line.trim_start();
        let marker = trimmed.chars().next().unwrap_or(' ');
        let count = trimmed.chars().take_while(|ch| *ch == marker).count();
        if let Some((open, length)) = fence {
            if marker == open && count >= length && trimmed[count..].trim().is_empty() { fence = None; }
            continue;
        }
        if matches!(marker, '`' | '~') && count >= 3 {
            fence = Some((marker, count));
            continue;
        }
        if trimmed.is_empty() {
            paragraph_break = !prose.is_empty();
            continue;
        }
        if line.starts_with("    ") || line.starts_with('\t') { continue; }
        if paragraph_break {
            prose.push("");
            paragraph_break = false;
        }
        prose.push(line);
    }
    let text = prose.join("\n").trim().to_owned();
    if jynx_model::progress_text_is_fragment(&text) { return String::new(); }


    if allow_brief || text.split_whitespace().count() >= 24
        || (!text.contains(char::is_whitespace) && text.chars().count() >= 80) { text }
    else { String::new() }
}

#[cfg(test)]
mod tests {
    use super::*;
    const UPDATE: &str = "I will inspect the fixture and its configuration together to identify the cause, then make the smallest relevant change and verify the result before reporting back.";
    #[test]
    fn routine_fragments_are_silent_but_necessary_questions_are_preserved() {
        assert!(public_update("Let me read the next file.", false).is_empty());
        assert_eq!(public_update("Which workspace should I use?", true), "Which workspace should I use?");
        assert_eq!(public_update(UPDATE, false), UPDATE);
    }
    #[test]
    fn progress_preserves_one_blank_line_between_paragraphs() {
        let second = "The next check will confirm whether it works.";
        let update = format!("{UPDATE}\r\n\r\n \r\n{second}\r\n\r\n");
        assert_eq!(public_update(&update, false), format!("{UPDATE}\n\n{second}"));
        assert_eq!(public_update(UPDATE, false), UPDATE);
    }
    #[test]
    fn progress_removes_code_blocks_but_keeps_inline_names_and_prose() {
        let update = format!("{UPDATE}\n```rust\nfn main() {{}}\n```\nThe change belongs in `main.rs`.");
        let cleaned = public_update(&update, false);
        assert!(!cleaned.contains("fn main"));
        assert!(!cleaned.contains("```"));
        assert!(cleaned.contains("`main.rs`"));
        assert!(public_update("```rust\nfn main() {}\n```", true).is_empty());
        assert!(public_update("~~~python\nprint('unfinished')", true).is_empty());
        assert!(!public_update(&format!("{UPDATE}\n    const secret = 1;"), false).contains("const secret"));
    }
}

#[cfg(test)]
mod opening_tests {
    use super::*;
    #[tokio::test(start_paused = true)]
    async fn slow_or_invalid_opening_is_optional_and_bounded() {
        let start=tokio::time::Instant::now();
        assert!(optional_opening(async {tokio::time::sleep(std::time::Duration::from_secs(60)).await;Ok("Late opening".into())}).await.is_none());
        assert_eq!(start.elapsed(),std::time::Duration::from_secs(15));
        assert!(optional_opening(async {Err(anyhow::anyhow!("Synthetic narration error"))}).await.is_none());
        assert!(optional_opening(async {Ok("```rust\nfn main() {}\n```".into())}).await.is_none());
        assert_eq!(optional_opening(async {Ok("I will verify the fixture with its tests.".into())}).await.as_deref(),Some("I will verify the fixture with its tests."));
    }
}
