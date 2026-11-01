mod questions;
use anyhow::{Result, anyhow};
use jynx_sandbox::WorkspaceSandbox;
use jynx_security::scan_untrusted_text;
use futures_util::stream::{FuturesUnordered, StreamExt};
use ignore::WalkBuilder;
use regex::Regex;
use reqwest::Client;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader};
use std::process::Stdio;
use tokio::fs;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;
use tokio::sync::mpsc::UnboundedSender;

fn ui_unified_diff(path: &std::path::Path, before: &str, after: &str) -> String {
    if before == after {
        return String::new();
    }
    let old: Vec<&str> = before.lines().collect();
    let new: Vec<&str> = after.lines().collect();
    let mut prefix = 0usize;
    while prefix < old.len() && prefix < new.len() && old[prefix] == new[prefix] {
        prefix += 1;
    }
    let mut suffix = 0usize;
    while suffix < old.len().saturating_sub(prefix)
        && suffix < new.len().saturating_sub(prefix)
        && old[old.len() - 1 - suffix] == new[new.len() - 1 - suffix]
    {
        suffix += 1;
    }
    let context_start = prefix.saturating_sub(3);
    let old_change_end = old.len().saturating_sub(suffix);
    let new_change_end = new.len().saturating_sub(suffix);
    let old_end = (old_change_end + 3).min(old.len());
    let new_end = (new_change_end + 3).min(new.len());
    let display = path.to_string_lossy();
    let mut diff = format!(
        "--- {display}\n+++ {display}\n@@ -{},{} +{},{} @@\n",
        context_start + 1,
        old_end.saturating_sub(context_start),
        context_start + 1,
        new_end.saturating_sub(context_start),
    );
    for line in &old[context_start..prefix] {
        diff.push(' ');
        diff.push_str(line);
        diff.push('\n');
    }
    for line in &old[prefix..old_change_end] {
        diff.push('-');
        diff.push_str(line);
        diff.push('\n');
    }
    for line in &new[prefix..new_change_end] {
        diff.push('+');
        diff.push_str(line);
        diff.push('\n');
    }
    let suffix_start = old_change_end.max(context_start);
    for line in &old[suffix_start..old_end] {
        diff.push(' ');
        diff.push_str(line);
        diff.push('\n');
    }
    diff
}

fn configured_skills_root() -> Option<std::path::PathBuf> {
    let root = std::env::var_os("JYNX_SKILLS_DIR")?;
    std::path::PathBuf::from(root).canonicalize().ok()
}

fn ensure_skill_enabled(root: &std::path::Path, skill: &str) -> Result<()> {
    let state = std::fs::read_to_string(root.join("state.json")).unwrap_or_default();
    let disabled = serde_json::from_str::<Value>(&state)
        .ok()
        .and_then(|value| value["disabled"].as_array().cloned())
        .unwrap_or_default();
    if disabled
        .iter()
        .filter_map(Value::as_str)
        .any(|entry| entry.eq_ignore_ascii_case(skill))
    {
        return Err(anyhow!("the requested skill is disabled"));
    }
    Ok(())
}

fn resolve_skill_file(
    skill: &str,
    relative: &str,
    scripts_only: bool,
) -> Result<std::path::PathBuf> {
    let root = configured_skills_root()
        .ok_or_else(|| anyhow!("skill files are not available in this host"))?;
    if skill.is_empty()
        || !skill
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(anyhow!("invalid skill id"));
    }
    ensure_skill_enabled(&root, skill)?;
    let relative_path = std::path::Path::new(relative);
    if relative.is_empty()
        || relative_path.is_absolute()
        || relative_path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(anyhow!(
            "skill file path must be relative and cannot contain '..'"
        ));
    }
    let skill_root = root.join(skill).canonicalize()?;
    if !skill_root.starts_with(&root) {
        return Err(anyhow!(
            "skill directory escapes the configured skills root"
        ));
    }
    if scripts_only && !relative_path.starts_with("scripts") {
        return Err(anyhow!("skill scripts must be stored under scripts/"));
    }
    let target = skill_root.join(relative_path).canonicalize()?;
    if !target.starts_with(&skill_root) || !target.is_file() {
        return Err(anyhow!(
            "skill file was not found inside the selected skill"
        ));
    }
    Ok(target)
}

pub fn schemas(
    web_search_enabled: bool,
    browser_enabled: bool,
    host_browser_enabled: bool,
    host_terminal_enabled: bool,
) -> Value {
    let mut tools = vec![
        json!({"type":"function","function":{"name":"ask_user","description":"Ask necessary clarification questions. Call alone; execution stops and the next user message supplies the answer. Free text is always allowed.","parameters":{"type":"object","properties":{"questions":{"type":"array","minItems":1,"maxItems":3,"items":{"type":"object","properties":{"id":{"type":"string"},"question":{"type":"string"},"options":{"type":"array","maxItems":5,"items":{"type":"string"}}},"required":["question"]}}},"required":["questions"]}}}),
        json!({
            "type": "function",
            "function": {
                "name": "view_image",
                "description": "Open a workspace image for visual inspection. Its pixels are supplied to the model as image input, not read as text. Supports PNG, JPEG, WebP, GIF and BMP up to 20 MiB. Requires a vision-capable model.",
                "parameters": {
                    "type": "object",
                    "properties": { "path": { "type": "string" } },
                    "required": ["path"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "read_file",
                "description": "Read a bounded line range of a UTF-8 text file, with line numbers and truncation metadata. For large files, request the next range instead of loading the entire file into context. Use this instead of the terminal for reading files.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" },
                        "start_line": { "type": "integer", "minimum": 1 },
                        "max_lines": { "type": "integer", "minimum": 1, "maximum": 1000 }
                    },
                    "required": ["path"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "write_file",
                "description": "Create a UTF-8 file or replace its entire contents. For localized changes to an existing file, prefer edit_file.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" },
                        "content": { "type": "string" }
                    },
                    "required": ["path", "content"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "edit_file",
                "description": "Replace exactly one matching UTF-8 text span in an existing file. Fails if the old text is missing or ambiguous, avoiding accidental whole-file rewrites. Read the relevant range first. Request independent edits to multiple files together in the same tool_calls batch.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" },
                        "old_text": { "type": "string" },
                        "new_text": { "type": "string" }
                    },
                    "required": ["path", "old_text", "new_text"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "list_files",
                "description": "Discover files recursively, respecting ignore files and skipping hidden/build noise. Results are bounded; narrow path or increase max_results if truncated.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" },
                        "max_results": { "type": "integer", "minimum": 1, "maximum": 5000 },
                        "max_depth": { "type": "integer", "minimum": 1, "maximum": 64 }
                    }
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "search_files",
                "description": "Search literal text recursively in code files, respecting ignore files. Returns paths, line numbers, snippets, and truncation status. Narrow path for more results. Use this instead of shell search.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string" },
                        "path": { "type": "string" },
                        "case_sensitive": { "type": "boolean" },
                        "max_results": { "type": "integer", "minimum": 1, "maximum": 200 }
                    },
                    "required": ["query"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "run_command",
                "description": "Run a normal Harness shell command. Use for commands, builds, tests, package managers, and shell-native operations; never use it merely to read a file or search the web. This does not open the app's side-panel terminal.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "command": { "type": "string" },
                        "timeout_ms": { "type": "integer", "minimum": 1000, "maximum": 3600000 }
                    },
                    "required": ["command"]
                }
            }
        }),
    ];

    if configured_skills_root().is_some() {
        tools.push(json!({
            "type": "function",
            "function": {
                "name": "read_skill_file",
                "description": "Read a supporting text file from an enabled Jynx skill. Use only when that skill's instructions or manifest identify the file as relevant.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "skill": { "type": "string", "description": "Installed skill id from the skill instructions" },
                        "path": { "type": "string", "description": "Relative path inside the skill directory" },
                        "start_line": { "type": "integer", "minimum": 1 },
                        "max_lines": { "type": "integer", "minimum": 1, "maximum": 1000 }
                    },
                    "required": ["skill", "path"]
                }
            }
        }));
        tools.push(json!({
            "type": "function",
            "function": {
                "name": "run_skill_script",
                "description": "Run a script from an enabled Jynx skill's scripts/ directory with direct, non-shell arguments. Use only when the skill instructions call for that script.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "skill": { "type": "string", "description": "Installed skill id from the skill instructions" },
                        "path": { "type": "string", "description": "Relative script path under scripts/" },
                        "arguments": { "type": "array", "items": { "type": "string" }, "maxItems": 32 },
                        "timeout_ms": { "type": "integer", "minimum": 1000, "maximum": 3600000 }
                    },
                    "required": ["skill", "path"]
                }
            }
        }));
    }

    if web_search_enabled {
        tools.push(json!({
            "type": "function",
            "function": {
                "name": "web_search",
                "description": "Search the public web without an API key. Always pass queries as an array containing one or several independent search strings. Multiple queries are fetched in parallel.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "queries": { "type": "array", "items": { "type": "string", "minLength": 1 }, "minItems": 1, "maxItems": 8 },
                        "max_results": { "type": "integer", "minimum": 1, "maximum": 10 }
                    },
                    "required": ["queries"]
                }
            }
        }));
    }

    if web_search_enabled && browser_enabled {
        tools.push(json!({
            "type": "function",
            "function": {
                "name": "web_fetch",
                "description": "Fetch and read a public web page directly. Use after web_search when the contents of a result are needed. Request independent pages together in one tool batch; up to four page reads run concurrently.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "url": { "type": "string" },
                        "max_chars": { "type": "integer", "minimum": 1000, "maximum": 50000 }
                    },
                    "required": ["url"]
                }
            }
        }));
    }

    if browser_enabled && host_browser_enabled {
        tools.push(json!({
            "type": "function",
            "function": {
                "name": "open_browser",
                "description": "Open a public HTTP or HTTPS URL in Jynx's built-in browser side panel. Use this when the user asks to see a page in the app; use web_fetch when you need to read page contents yourself.",
                "parameters": {
                    "type": "object",
                    "properties": { "url": { "type": "string" } },
                    "required": ["url"]
                }
            }
        }));
    }

    if host_terminal_enabled {
        tools.push(json!({
            "type": "function",
            "function": {
                "name": "run_in_terminal_panel",
                "description": "Run a command while showing it in Jynx's Terminal side panel. Use this only when the user explicitly asks to use, show, or run something in that side-panel terminal. Use run_command for normal shell work.",
                "parameters": {
                    "type": "object",
                    "properties": { "command": { "type": "string" } },
                    "required": ["command"]
                }
            }
        }));
    }

    Value::Array(tools)
}

fn percent_encode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
                (byte as char).to_string()
            } else if byte == b' ' {
                "+".into()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}

fn percent_decode(value: &str) -> String {
    fn hex(byte: u8) -> Option<u8> {
        match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            b'A'..=b'F' => Some(byte - b'A' + 10),
            _ => None,
        }
    }
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let (Some(high), Some(low)) = (hex(bytes[index + 1]), hex(bytes[index + 2])) {
                output.push((high << 4) | low);
                index += 3;
                continue;
            }
        }
        output.push(if bytes[index] == b'+' {
            b' '
        } else {
            bytes[index]
        });
        index += 1;
    }
    String::from_utf8_lossy(&output).into_owned()
}

fn html_decode(value: &str) -> String {
    value
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
}

fn strip_tags(value: &str) -> String {
    let script = Regex::new(
        r"(?is)<script[^>]*>.*?</script>|<style[^>]*>.*?</style>|<noscript[^>]*>.*?</noscript>",
    )
    .unwrap();
    let tags = Regex::new(r"(?is)<[^>]+>").unwrap();
    let spaces = Regex::new(r"[ \t\r\f\v]+|\n{3,}").unwrap();
    let without_script = script.replace_all(value, " ");
    let plain = tags.replace_all(&without_script, "\n");
    let decoded = html_decode(&plain);
    spaces.replace_all(&decoded, " ").trim().to_string()
}

fn extract_redirect_url(raw: &str) -> String {
    let decoded = html_decode(raw);
    if let Some(pos) = decoded.find("uddg=") {
        let value = &decoded[pos + 5..];
        let value = value.split('&').next().unwrap_or(value);
        return percent_decode(value);
    }
    if decoded.starts_with("//") {
        return format!("https:{decoded}");
    }
    decoded
}

fn favicon_for(url: &str) -> String {
    let host = url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or_default();
    if host.is_empty() {
        String::new()
    } else {
        format!("https://{host}/favicon.ico")
    }
}

async fn search_one(client: &Client, query: String, max_results: usize) -> Value {
    let url = format!(
        "https://html.duckduckgo.com/html/?q={}",
        percent_encode(&query)
    );
    let response = match client.get(url).send().await {
        Ok(response) => response,
        Err(error) => {
            return json!({"query":query,"error":error.to_string(),"results":[]});
        }
    };
    let status = response.status().as_u16();
    let body = match response.text().await {
        Ok(body) => body,
        Err(error) => {
            return json!({"query":query,"status":status,"error":error.to_string(),"results":[]});
        }
    };
    let result_re =
        Regex::new(r#"(?is)<a[^>]*class="[^"]*result__a[^"]*"[^>]*href="([^"]+)"[^>]*>(.*?)</a>"#)
            .unwrap();
    let snippet_re = Regex::new(r#"(?is)<a[^>]*class="[^"]*result__snippet[^"]*"[^>]*>(.*?)</a>|<div[^>]*class="[^"]*result__snippet[^"]*"[^>]*>(.*?)</div>"#).unwrap();
    let snippets: Vec<String> = snippet_re
        .captures_iter(&body)
        .map(|capture| {
            strip_tags(
                capture
                    .get(1)
                    .or_else(|| capture.get(2))
                    .map(|m| m.as_str())
                    .unwrap_or_default(),
            )
        })
        .collect();
    let results: Vec<Value> = result_re
        .captures_iter(&body)
        .take(max_results)
        .enumerate()
        .filter_map(|(index, capture)| {
            let url = extract_redirect_url(capture.get(1)?.as_str());
            if !url.starts_with("http://") && !url.starts_with("https://") {
                return None;
            }
            let title = strip_tags(capture.get(2)?.as_str());
            Some(json!({
                "query": &query,
                "title": title,
                "url": url,
                "snippet": snippets.get(index).cloned().unwrap_or_default(),
                "favicon": favicon_for(&url)
            }))
        })
        .collect();
    json!({"query":&query,"status":status,"results":results})
}

async fn web_search(
    args: &Value,
    progress: Option<(&UnboundedSender<Value>, &str)>,
) -> Result<String> {
    let mut queries: Vec<String> = args["queries"]
        .as_array()
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    if let Some(query) = args["query"]
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        queries.insert(0, query.to_string());
    }
    queries.sort();
    queries.dedup();
    queries.truncate(8);
    if queries.is_empty() {
        return Err(anyhow!("missing search query"));
    }
    let max_results = args["max_results"].as_u64().unwrap_or(6).clamp(1, 10) as usize;
    let client = Client::builder()
        .user_agent("Mozilla/5.0 (compatible; Jynx/0.1; +https://localhost)")
        .connect_timeout(std::time::Duration::from_secs(12))
        .timeout(std::time::Duration::from_secs(25))
        .build()?;
    let mut pending = FuturesUnordered::new();
    for query in queries.iter().cloned() {
        pending.push(search_one(&client, query, max_results));
    }
    let mut pages = Vec::new();
    let mut results = Vec::new();
    while let Some(page) = pending.next().await {
        if let Some(entries) = page["results"].as_array() {
            results.extend(entries.iter().cloned());
        }
        pages.push(page);
        if let Some((events, tool_id)) = progress {
            let partial =
                json!({"queries":&queries,"results":&results,"sources":&pages}).to_string();
            let _ = events.send(
                json!({"type":"tool_progress","id":tool_id,"kind":"web_search","output":partial}),
            );
        }
    }
    Ok(json!({"queries":&queries,"results":&results,"sources":&pages}).to_string())
}

async fn web_fetch(args: &Value) -> Result<String> {
    let url = args["url"]
        .as_str()
        .ok_or_else(|| anyhow!("missing url"))?
        .trim();
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(anyhow!("web_fetch only accepts http or https URLs"));
    }
    let max_chars = args["max_chars"]
        .as_u64()
        .unwrap_or(20000)
        .clamp(1000, 50000) as usize;
    let client = Client::builder()
        .user_agent("Mozilla/5.0 (compatible; Jynx/0.1; +https://localhost)")
        .connect_timeout(std::time::Duration::from_secs(12))
        .timeout(std::time::Duration::from_secs(30))
        .build()?;
    let response = client.get(url).send().await?;
    let status = response.status().as_u16();
    let final_url = response.url().to_string();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let body = response.text().await?;
    let title_re = Regex::new(r"(?is)<title[^>]*>(.*?)</title>").unwrap();
    let title = title_re
        .captures(&body)
        .and_then(|capture| capture.get(1))
        .map(|value| strip_tags(value.as_str()))
        .unwrap_or_default();
    let mut content = if content_type.contains("html") {
        strip_tags(&body)
    } else {
        body
    };
    if content.len() > max_chars {
        let mut cut = max_chars;
        while !content.is_char_boundary(cut) && cut > 0 {
            cut -= 1;
        }
        content.truncate(cut);
    }
    let injection_scan = scan_untrusted_text(&content);
    Ok(json!({
        "url": &final_url,
        "title": title,
        "status": status,
        "content_type": content_type,
        "content": content,
        "favicon": favicon_for(&final_url),
        "injection_scan": injection_scan
    })
    .to_string())
}

fn code_walk(root: &std::path::Path, max_depth: Option<usize>) -> ignore::Walk {
    let mut builder = WalkBuilder::new(root);
    builder.max_depth(max_depth);
    builder.require_git(false);


    let selected_root = root.to_path_buf();
    builder.filter_entry(move |entry| {
        if entry.path() == selected_root {
            return true;
        }
        !entry.file_type().is_some_and(|kind| kind.is_dir())
            || !matches!(
                entry
                    .file_name()
                    .to_string_lossy()
                    .to_ascii_lowercase()
                    .as_str(),
                ".git" | "node_modules" | "target" | "dist" | "build" | ".runtime-staging"
            )
    });
    builder.build()
}

fn read_file_range(path: &std::path::Path, start_line: usize, max_lines: usize) -> Result<String> {
    let reader = BufReader::new(std::fs::File::open(path)?);
    let mut rendered = String::new();
    let mut last_line = start_line.saturating_sub(1);
    let mut truncated = false;
    const MAX_CHARS: usize = 40_000;
    for (index, line) in reader.lines().enumerate() {
        let number = index + 1;
        if number < start_line {
            continue;
        }
        if number >= start_line.saturating_add(max_lines) {
            truncated = true;
            break;
        }
        let line = line?;
        let prefix = format!("{number:>5} | ");
        let remaining = MAX_CHARS.saturating_sub(rendered.chars().count() + prefix.chars().count());
        if remaining == 0 {
            truncated = true;
            break;
        }
        let line_chars = line.chars().count();
        rendered.push_str(&prefix);
        rendered.extend(line.chars().take(remaining));
        rendered.push('\n');
        last_line = number;
        if line_chars > remaining {
            truncated = true;
            break;
        }
    }
    let finding = scan_untrusted_text(&rendered);
    Ok(json!({
        "path": path,
        "content": rendered,
        "start_line": start_line,
        "end_line": last_line,
        "truncated": truncated,
        "next_start_line": if truncated { Some(last_line + 1) } else { None },
        "injection_scan": finding
    })
    .to_string())
}

fn list_workspace_files(
    root: std::path::PathBuf,
    requested_path: String,
    max_results: usize,
    max_depth: Option<usize>,
) -> Result<String> {
    std::fs::metadata(&root)?;
    let mut files = Vec::new();
    let mut truncated = false;
    for entry in code_walk(&root, max_depth).filter_map(Result::ok) {
        if !entry.file_type().is_some_and(|kind| kind.is_file()) {
            continue;
        }
        if files.len() == max_results {
            truncated = true;
            break;
        }
        files.push(entry.path().display().to_string());
    }
    Ok(
        json!({"path":root,"requested_path":requested_path,"files":files,"truncated":truncated})
            .to_string(),
    )
}

fn search_workspace_files(
    root: std::path::PathBuf,
    requested_path: String,
    query: String,
    case_sensitive: bool,
    max_results: usize,
) -> Result<String> {
    std::fs::metadata(&root)?;
    if query.is_empty() {
        return Err(anyhow!("search query cannot be empty"));
    }
    let needle = if case_sensitive {
        query.clone()
    } else {
        query.to_lowercase()
    };
    let mut matches = Vec::new();
    let mut truncated = false;
    let mut scanned_files = 0usize;
    'files: for entry in code_walk(&root, None).filter_map(Result::ok) {
        if !entry.file_type().is_some_and(|kind| kind.is_file()) {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if metadata.len() > 4 * 1024 * 1024 {
            continue;
        }
        let Ok(file) = std::fs::File::open(entry.path()) else {
            continue;
        };
        scanned_files += 1;
        for (line_index, line) in BufReader::new(file).lines().enumerate() {
            let Ok(line) = line else {
                break;
            };
            let found = if case_sensitive {
                line.contains(&needle)
            } else {
                line.to_lowercase().contains(&needle)
            };
            if found {
                if matches.len() == max_results {
                    truncated = true;
                    break 'files;
                }
                matches.push(json!({
                    "path":entry.path().display().to_string(),
                    "line":line_index + 1,
                    "text":line.trim().chars().take(400).collect::<String>()
                }));
            }
        }
    }
    Ok(json!({"path":root,"requested_path":requested_path,"query":query,"matches":matches,"truncated":truncated,"scanned_files":scanned_files}).to_string())
}

async fn collect_bounded<R: AsyncRead + Unpin>(
    mut reader: R,
    limit: usize,
) -> std::io::Result<(String, bool)> {
    let mut visible = Vec::new();
    let mut buffer = [0u8; 8192];
    let mut truncated = false;
    loop {
        let count = reader.read(&mut buffer).await?;
        if count == 0 {
            break;
        }
        let remaining = limit.saturating_sub(visible.len());
        visible.extend_from_slice(&buffer[..count.min(remaining)]);
        truncated |= count > remaining;
    }
    Ok((String::from_utf8_lossy(&visible).into_owned(), truncated))
}

pub async fn execute(
    sandbox: &WorkspaceSandbox,
    name: &str,
    args: &Value,
    web_search_enabled: bool,
    browser_enabled: bool,
    host_browser_enabled: bool,
    host_terminal_enabled: bool,
    progress: Option<(&UnboundedSender<Value>, &str)>,
) -> Result<String> {
    match name {
        "ask_user" => questions::ask_user(args),
        "view_image" => {
            let path = sandbox.resolve(args["path"].as_str().ok_or_else(|| anyhow!("missing path"))?, false)?;
            let metadata = fs::metadata(&path).await?;
            if !metadata.is_file() || metadata.len() > 20 * 1024 * 1024 {
                return Err(anyhow!("view_image requires an image file no larger than 20 MiB"));
            }
            let mut file = fs::File::open(&path).await?;
            let mut header = [0u8; 16];
            let count = file.read(&mut header).await?;
            let header = &header[..count];
            let mime = if header.starts_with(b"\x89PNG\r\n\x1a\n") { "image/png" }
                else if header.starts_with(b"\xff\xd8\xff") { "image/jpeg" }
                else if header.starts_with(b"GIF87a") || header.starts_with(b"GIF89a") { "image/gif" }
                else if header.starts_with(b"BM") { "image/bmp" }
                else if header.starts_with(b"RIFF") && header.get(8..12) == Some(b"WEBP") { "image/webp" }
                else { return Err(anyhow!("unsupported image format; use PNG, JPEG, WebP, GIF or BMP")); };
            Ok(json!({ "path":path, "mime":mime, "bytes":metadata.len() }).to_string())
        }
        "read_file" => {
            let path = sandbox.resolve(
                args["path"]
                    .as_str()
                    .ok_or_else(|| anyhow!("missing path"))?,
                false,
            )?;
            let start_line = args["start_line"].as_u64().unwrap_or(1).max(1) as usize;
            let max_lines = args["max_lines"].as_u64().unwrap_or(400).clamp(1, 1000) as usize;
            tokio::task::spawn_blocking(move || read_file_range(&path, start_line, max_lines))
                .await?
        }
        "write_file" => {
            let path = sandbox.resolve(
                args["path"]
                    .as_str()
                    .ok_or_else(|| anyhow!("missing path"))?,
                true,
            )?;
            let content = args["content"]
                .as_str()
                .ok_or_else(|| anyhow!("missing content"))?;
            let existed = fs::metadata(&path).await.is_ok();
            let previous = if existed {
                fs::read_to_string(&path).await.unwrap_or_default()
            } else {
                String::new()
            };
            let previous_bytes = previous.len();
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).await?;
            }
            fs::write(&path, content).await?;
            let ui_diff = ui_unified_diff(&path, &previous, content);
            Ok(json!({
                "path":path,
                "written":path,
                "bytes":content.len(),
                "previous_bytes":previous_bytes,
                "change_kind": if existed { "edit" } else { "create" },
                "_ui_diff":ui_diff
            })
            .to_string())
        }
        "edit_file" => {
            let path = sandbox.resolve(
                args["path"]
                    .as_str()
                    .ok_or_else(|| anyhow!("missing path"))?,
                true,
            )?;
            let old_text = args["old_text"]
                .as_str()
                .ok_or_else(|| anyhow!("missing old_text"))?;
            let new_text = args["new_text"]
                .as_str()
                .ok_or_else(|| anyhow!("missing new_text"))?;
            if old_text.is_empty() {
                return Err(anyhow!("old_text cannot be empty"));
            }
            let content = fs::read_to_string(&path).await?;
            let occurrences = content.matches(old_text).count();
            if occurrences != 1 {
                return Err(anyhow!(
                    "edit_file expected exactly one old_text match; found {occurrences}"
                ));
            }
            let updated = content.replacen(old_text, new_text, 1);
            fs::write(&path, &updated).await?;
            let ui_diff = ui_unified_diff(&path, &content, &updated);
            Ok(json!({"path":path,"written":path,"change_kind":"edit","replacements":1,"bytes":updated.len(),"_ui_diff":ui_diff}).to_string())
        }
        "list_files" => {
            let raw = args["path"].as_str().unwrap_or(".");
            let root = sandbox.resolve(raw, false)?;
            let max_results = args["max_results"].as_u64().unwrap_or(500).clamp(1, 5000) as usize;
            let max_depth = args["max_depth"]
                .as_u64()
                .map(|depth| depth.clamp(1, 64) as usize);
            let requested_path = raw.to_string();
            tokio::task::spawn_blocking(move || {
                list_workspace_files(root, requested_path, max_results, max_depth)
            })
            .await?
        }
        "search_files" => {
            let query = args["query"]
                .as_str()
                .ok_or_else(|| anyhow!("missing query"))?
                .to_string();
            let raw = args["path"].as_str().unwrap_or(".");
            let root = sandbox.resolve(raw, false)?;
            let case_sensitive = args["case_sensitive"].as_bool().unwrap_or(false);
            let max_results = args["max_results"].as_u64().unwrap_or(80).clamp(1, 200) as usize;
            let requested_path = raw.to_string();
            tokio::task::spawn_blocking(move || {
                search_workspace_files(root, requested_path, query, case_sensitive, max_results)
            })
            .await?
        }
        "read_skill_file" => {
            let skill = args["skill"]
                .as_str()
                .ok_or_else(|| anyhow!("missing skill id"))?;
            let relative = args["path"]
                .as_str()
                .ok_or_else(|| anyhow!("missing path"))?;
            let path = resolve_skill_file(skill, relative, false)?;
            let start_line = args["start_line"].as_u64().unwrap_or(1).max(1) as usize;
            let max_lines = args["max_lines"].as_u64().unwrap_or(400).clamp(1, 1000) as usize;
            tokio::task::spawn_blocking(move || read_file_range(&path, start_line, max_lines))
                .await?
        }
        "run_skill_script" => {
            let skill = args["skill"]
                .as_str()
                .ok_or_else(|| anyhow!("missing skill id"))?;
            let relative = args["path"]
                .as_str()
                .ok_or_else(|| anyhow!("missing path"))?;
            let script = resolve_skill_file(skill, relative, true)?;
            let script_args = args["arguments"]
                .as_array()
                .map(|values| {
                    values
                        .iter()
                        .filter_map(Value::as_str)
                        .take(32)
                        .map(|value| value.chars().take(4096).collect::<String>())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let extension = script
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            let mut process = match extension.as_str() {
                "py" => {
                    let mut command = Command::new(if cfg!(windows) {
                        "python.exe"
                    } else {
                        "python3"
                    });
                    command.arg(&script);
                    command
                }
                "js" | "mjs" | "cjs" => {
                    let executable =
                        std::env::var_os("JYNX_NODE_EXECUTABLE").unwrap_or_else(|| {
                            if cfg!(windows) {
                                "node.exe".into()
                            } else {
                                "node".into()
                            }
                        });
                    let mut command = Command::new(executable);
                    command.env("ELECTRON_RUN_AS_NODE", "1");
                    command.arg(&script);
                    command
                }
                "ps1" if cfg!(windows) => {
                    let mut command = Command::new("powershell.exe");
                    command.args(["-NoLogo", "-NoProfile", "-NonInteractive", "-File"]);
                    command.arg(&script);
                    command
                }
                "cmd" | "bat" if cfg!(windows) => {
                    let mut command = Command::new("cmd.exe");
                    command.args(["/d", "/s", "/c"]);
                    command.arg(&script);
                    command
                }
                "sh" | "bash" | "zsh" if !cfg!(windows) => {
                    let shell = if extension == "sh" {
                        "sh"
                    } else {
                        extension.as_str()
                    };
                    let mut command = Command::new(shell);
                    command.arg(&script);
                    command
                }
                "rb" => {
                    let mut command = Command::new("ruby");
                    command.arg(&script);
                    command
                }
                "php" => {
                    let mut command = Command::new("php");
                    command.arg(&script);
                    command
                }
                "lua" => {
                    let mut command = Command::new("lua");
                    command.arg(&script);
                    command
                }
                _ => return Err(anyhow!("unsupported skill script type: .{extension}")),
            };
            let timeout_ms = args["timeout_ms"]
                .as_u64()
                .unwrap_or(600_000)
                .clamp(1_000, 3_600_000);
            let working_directory = script
                .parent()
                .ok_or_else(|| anyhow!("script directory is missing"))?;
            let mut child = process
                .args(&script_args)
                .current_dir(working_directory)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true)
                .spawn()?;
            let stdout = child
                .stdout
                .take()
                .ok_or_else(|| anyhow!("missing script stdout"))?;
            let stderr = child
                .stderr
                .take()
                .ok_or_else(|| anyhow!("missing script stderr"))?;
            let result =
                tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), async {
                    tokio::try_join!(
                        child.wait(),
                        collect_bounded(stdout, 64_000),
                        collect_bounded(stderr, 64_000)
                    )
                })
                .await;
            let (status, (stdout, stdout_truncated), (stderr, stderr_truncated)) = match result {
                Ok(value) => value?,
                Err(_) => {
                    let _ = child.kill().await;
                    return Err(anyhow!("skill script timed out after {timeout_ms} ms"));
                }
            };
            Ok(json!({"skill":skill,"path":relative,"exit_code":status.code(),"stdout":stdout,"stderr":stderr,"stdout_truncated":stdout_truncated,"stderr_truncated":stderr_truncated}).to_string())
        }
        "run_command" | "run_in_terminal_panel"
            if name == "run_command" || host_terminal_enabled =>
        {
            let command = args["command"]
                .as_str()
                .ok_or_else(|| anyhow!("missing command"))?;



            if !sandbox.is_full_access() {
                let normalized = command.trim().to_ascii_lowercase();
                let first = normalized.split_whitespace().next().unwrap_or_default();
                let file_read_command = matches!(first, "cat" | "type" | "get-content" | "gc");
                let list_command = matches!(first, "dir" | "ls");
                let web_command = matches!(first, "curl" | "wget" | "invoke-webrequest" | "iwr");
                let search_command = matches!(first, "grep" | "rg" | "findstr");
                if file_read_command {
                    return Err(anyhow!(
                        "use read_file for file contents instead of the terminal"
                    ));
                }
                if list_command {
                    return Err(anyhow!(
                        "use list_files for directory inspection instead of the terminal"
                    ));
                }
                if search_command {
                    return Err(anyhow!(
                        "use search_files for workspace text search instead of the terminal"
                    ));
                }
                if web_command {
                    return Err(anyhow!(
                        "use web_search or web_fetch for web access instead of the terminal"
                    ));
                }
            }

            let mut child = if cfg!(windows) {
                let mut process = Command::new("cmd.exe");
                process.args(["/d", "/s", "/c", command]);
                process
            } else {
                let mut process = Command::new("sh");
                process.args(["-lc", command]);
                process
            };
            let timeout_ms = args["timeout_ms"]
                .as_u64()
                .unwrap_or(600_000)
                .clamp(1_000, 3_600_000);
            let mut child = child
                .current_dir(sandbox.root())
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true)
                .spawn()?;
            let stdout = child
                .stdout
                .take()
                .ok_or_else(|| anyhow!("missing command stdout"))?;
            let stderr = child
                .stderr
                .take()
                .ok_or_else(|| anyhow!("missing command stderr"))?;
            let result =
                tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), async {
                    tokio::try_join!(
                        child.wait(),
                        collect_bounded(stdout, 64_000),
                        collect_bounded(stderr, 64_000)
                    )
                })
                .await;
            let (status, (stdout, stdout_truncated), (stderr, stderr_truncated)) = match result {
                Ok(value) => value?,
                Err(_) => {
                    let _ = child.kill().await;
                    return Err(anyhow!("command timed out after {timeout_ms} ms"));
                }
            };
            Ok(json!({"exit_code":status.code(),"stdout":stdout,"stderr":stderr,"stdout_truncated":stdout_truncated,"stderr_truncated":stderr_truncated}).to_string())
        }
        "web_search" if web_search_enabled => web_search(args, progress).await,
        "web_fetch" if web_search_enabled && browser_enabled => web_fetch(args).await,
        "open_browser" if browser_enabled && host_browser_enabled => {
            let raw = args["url"].as_str().ok_or_else(|| anyhow!("missing URL"))?;
            let url = reqwest::Url::parse(raw).map_err(|_| anyhow!("invalid browser URL"))?;
            if !matches!(url.scheme(), "http" | "https") {
                return Err(anyhow!(
                    "the built-in browser only accepts HTTP and HTTPS URLs"
                ));
            }
            Ok(json!({"url":url.as_str(),"opened_in":"jynx-side-panel"}).to_string())
        }
        "web_search" => Err(anyhow!("web search is disabled in Jynx settings")),
        "web_fetch" => Err(anyhow!(
            "web page reading is disabled in Jynx settings"
        )),
        "open_browser" => Err(anyhow!(
            "the built-in browser tool is disabled in Jynx settings"
        )),
        "run_in_terminal_panel" => Err(anyhow!(
            "the side-panel terminal tool is not supported by this client"
        )),
        _ => Err(anyhow!("unknown tool: {name}")),
    }
}

#[cfg(test)]
mod tests {
    use super::{execute, list_workspace_files, read_file_range, schemas, search_workspace_files};
    use jynx_sandbox::{ApprovalMode, WorkspaceSandbox};
    use std::fs;

    fn scratch() -> std::path::PathBuf {
        let name = format!(
            "harness-tools-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let root = std::env::temp_dir().join(name);
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn has_tool(value: &serde_json::Value, name: &str) -> bool {
        value.as_array().is_some_and(|tools| {
            tools
                .iter()
                .any(|tool| tool["function"]["name"].as_str() == Some(name))
        })
    }

    #[tokio::test]
    async fn view_image_returns_validated_workspace_image_and_rejects_text() {
        let root = scratch();
        fs::write(root.join("pixel.png"), include_bytes!("testdata/pixel.png")).unwrap();
        fs::write(root.join("fake.png"), b"not an image").unwrap();
        let sandbox = WorkspaceSandbox::new(root.to_string_lossy().to_string(), ApprovalMode::Workspace).unwrap();
        assert!(has_tool(&schemas(false, false, false, false), "view_image"));
        let output = execute(&sandbox, "view_image", &serde_json::json!({"path":"pixel.png"}), false, false, false, false, None).await.unwrap();
        let image: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(image["mime"], "image/png");
        assert!(image["path"].as_str().unwrap().ends_with("pixel.png"));
        assert!(execute(&sandbox, "view_image", &serde_json::json!({"path":"fake.png"}), false, false, false, false, None).await.is_err());
        assert!(execute(&sandbox, "view_image", &serde_json::json!({"path":"missing.png"}), false, false, false, false, None).await.is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn web_search_requires_queries_in_a_flat_native_tool_schema() {


        let tools = schemas(true, false, false, false);
        let search = tools.as_array().unwrap().iter().find(|tool| tool["function"]["name"] == "web_search").unwrap();
        let parameters = &search["function"]["parameters"];
        assert_eq!(parameters["required"], serde_json::json!(["queries"]));
        assert_eq!(parameters["properties"]["queries"]["minItems"], 1);
        assert!(parameters.get("anyOf").is_none());
        assert!(parameters.get("oneOf").is_none());
    }

    #[test]
    fn browser_host_action_is_only_advertised_for_capable_hosts() {
        assert!(!has_tool(
            &schemas(true, true, false, false),
            "open_browser"
        ));
        assert!(has_tool(&schemas(true, true, true, false), "open_browser"));
        assert!(!has_tool(
            &schemas(true, false, true, false),
            "open_browser"
        ));
    }

    #[test]
    fn terminal_panel_action_is_only_advertised_for_capable_hosts() {
        assert!(!has_tool(
            &schemas(true, true, true, false),
            "run_in_terminal_panel"
        ));
        assert!(has_tool(
            &schemas(true, true, true, true),
            "run_in_terminal_panel"
        ));
    }

    #[test]
    fn deep_discovery_respects_ignore_files_and_reports_truncation() {
        let root = scratch();
        fs::write(root.join(".gitignore"), "ignored.txt\n").unwrap();
        fs::write(root.join("ignored.txt"), "needle").unwrap();
        fs::create_dir_all(root.join("a/b/c/d/e/f/g/h/i")).unwrap();
        fs::write(root.join("a/b/c/d/e/f/g/h/i/deep.rs"), "needle\n").unwrap();
        fs::write(root.join("other.rs"), "needle\n").unwrap();
        let listed: serde_json::Value =
            serde_json::from_str(&list_workspace_files(root.clone(), ".".into(), 1, None).unwrap())
                .unwrap();
        assert_eq!(listed["files"].as_array().unwrap().len(), 1);
        assert_eq!(listed["truncated"], true);
        let found: serde_json::Value = serde_json::from_str(
            &search_workspace_files(root.clone(), ".".into(), "needle".into(), false, 10).unwrap(),
        )
        .unwrap();
        let matches = found["matches"].as_array().unwrap();
        assert_eq!(matches.len(), 2);
        assert!(
            matches
                .iter()
                .any(|item| item["path"].as_str().unwrap().ends_with("deep.rs"))
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn bounded_reads_return_line_numbers_and_next_range() {
        let root = scratch();
        let file = root.join("sample.rs");
        fs::write(&file, "one\ntwo\nthree\n").unwrap();
        let value: serde_json::Value =
            serde_json::from_str(&read_file_range(&file, 2, 1).unwrap()).unwrap();
        assert!(value["content"].as_str().unwrap().contains("2 | two"));
        assert_eq!(value["next_start_line"], 3);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_directory_is_an_error_not_an_empty_codebase() {
        let root = scratch();
        let missing = root.join("not-a-workspace");
        assert!(list_workspace_files(missing.clone(), "not-a-workspace".into(), 10, None).is_err());
        assert!(
            search_workspace_files(
                missing,
                "not-a-workspace".into(),
                "needle".into(),
                false,
                10
            )
            .is_err()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn edit_file_rejects_ambiguous_matches_without_changing_the_file() {
        let root = scratch();
        let file = root.join("sample.rs");
        fs::write(&file, "same\nsame\n").unwrap();
        let sandbox = WorkspaceSandbox::new(&root, ApprovalMode::Workspace).unwrap();
        let args = serde_json::json!({"path":"sample.rs","old_text":"same","new_text":"changed"});
        let result = execute(
            &sandbox,
            "edit_file",
            &args,
            false,
            false,
            false,
            false,
            None,
        )
        .await;
        assert!(result.is_err());
        assert_eq!(fs::read_to_string(&file).unwrap(), "same\nsame\n");
        fs::remove_dir_all(root).unwrap();
    }
}
