use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TrustLevel {
    System,
    User,
    Workspace,
    ToolOutput,
    External,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InjectionFinding {
    pub suspicious: bool,
    pub reasons: Vec<String>,
}

pub fn scan_untrusted_text(text: &str) -> InjectionFinding {
    let rules = [
        (
            r"(?i)ignore (all|any|the) previous instructions",
            "instruction override",
        ),
        (r"(?i)system prompt", "system prompt reference"),
        (r"(?i)developer message", "developer instruction reference"),
        (
            r"(?i)exfiltrat|steal|upload.*secret",
            "possible exfiltration request",
        ),
        (r"(?i)do not tell the user", "concealment request"),
    ];
    let mut reasons = Vec::new();
    for (pattern, reason) in rules {
        if Regex::new(pattern)
            .map(|r| r.is_match(text))
            .unwrap_or(false)
        {
            reasons.push(reason.to_string());
        }
    }
    InjectionFinding {
        suspicious: !reasons.is_empty(),
        reasons,
    }
}

pub fn wrap_tool_output(source: &str, payload: &str) -> String {
    let escaped = serde_json::json!({
        "trust": "untrusted_tool_output",
        "source": source,


        "content": serde_json::from_str::<serde_json::Value>(payload)
            .unwrap_or_else(|_| serde_json::Value::String(payload.into()))
    });
    escaped.to_string()
}
