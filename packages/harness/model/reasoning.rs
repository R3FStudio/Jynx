use regex::Regex;
use serde::Serialize;
use serde_json::{Value, json};
pub const NONE: &str = "@jynx/none";
pub const ON: &str = "@jynx/thinking:on";
pub const OFF: &str = "@jynx/thinking:off";
const PREFIX: &str = "@jynx/value:";
#[derive(Debug, Clone, Serialize)]
pub struct EffortOption {
    pub value: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

pub fn rank(value: &str) -> Option<usize> {
    [
        "none", "minimal", "low", "medium", "high", "xhigh", "max", "ultra",
    ]
    .iter()
    .position(|item| *item == value)
}
pub fn normalize(value: &str) -> String {
    value.trim().to_owned()
}
pub fn key(value: &Value) -> String {
    if let Some(value) = value.as_str().filter(|value| !value.starts_with("@jynx/")) {
        value.to_owned()
    } else {
        format!("{PREFIX}{value}")
    }
}
pub fn native_value(value: &str) -> Option<Value> {
    if [NONE, ON, OFF, "default"].contains(&value) {
        return None;
    }
    if let Some(value) = value.strip_prefix(PREFIX) {
        return serde_json::from_str::<Value>(value)
            .ok()
            .filter(|value| !value.is_object() && !value.is_array());
    }
    Some(json!(value))
}
fn option(entry: &Value) -> Option<EffortOption> {
    let value = if entry.is_object() {
        [
            "value",
            "id",
            "effort",
            "reasoningEffort",
            "reasoning_effort",
            "name",
        ]
        .iter()
        .find_map(|key| entry.get(key))?
    } else {
        entry
    };
    if value.is_object() || value.is_array() || value.as_str() == Some("") {
        return None;
    }
    let label = ["label", "displayName", "display_name", "title", "name"]
        .iter()
        .find_map(|key| entry.get(key).and_then(Value::as_str))
        .map(str::to_owned)
        .unwrap_or_else(|| {
            value
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| value.to_string())
        });
    Some(EffortOption {
        value: key(value),
        label,
        description: entry
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_owned),
    })
}
fn unique(entries: &[Value]) -> Vec<EffortOption> {
    let mut seen = std::collections::HashSet::new();
    entries
        .iter()
        .filter_map(option)
        .filter(|option| seen.insert(option.value.clone()))
        .collect()
}
fn template_order(mut values: Vec<String>) -> Vec<String> {
    if values.iter().all(|value| rank(value).is_some()) {
        values.sort_by_key(|value| rank(value).unwrap());
    }
    values
}
fn quoted(text: &str) -> Vec<String> {
    Regex::new(r#"['"]([^'"\r\n]+)['"]"#)
        .unwrap()
        .captures_iter(text)
        .map(|m| m[1].to_owned())
        .collect()
}
pub fn capability_options(props: &Value) -> Option<Vec<EffortOption>> {
    let raw = [
        "/chat_template_caps/reasoning_effort_levels",
        "/reasoning_effort_levels",
        "/reasoning/supported_efforts",
        "/reasoning_efforts",
        "/supportedReasoningEfforts",
        "/supported_reasoning_efforts",
        "/parameters/reasoning_effort/enum",
    ]
    .iter()
    .find_map(|path| props.pointer(path));
    if let Some(entries) = raw.and_then(|value| {
        value
            .as_array()
            .or_else(|| value.get("enum").and_then(Value::as_array))
    }) {
        let options = unique(entries);
        if !options.is_empty() {
            return Some(options);
        }
    }
    let template = props["chat_template"].as_str().unwrap_or("");
    let allowed = Regex::new(r"reasoning_effort\s+not\s+in\s*[\[(]([^\])]+)[\])]").unwrap();
    if let Some(found) = allowed.captures(template) {
        let entries = template_order(quoted(&found[1]))
            .into_iter()
            .map(Value::String)
            .collect::<Vec<_>>();
        let options = unique(&entries);
        if !options.is_empty() {
            return Some(options);
        }
    }
    let branches = Regex::new(r#"reasoning_effort\s*==\s*['"]([^'"\r\n]+)['"]"#)
        .unwrap()
        .captures_iter(template)
        .map(|m| m[1].to_owned())
        .collect::<Vec<_>>();
    if !branches.is_empty() {
        return Some(unique(
            &template_order(branches)
                .into_iter()
                .map(Value::String)
                .collect::<Vec<_>>(),
        ));
    }
    if template.contains("enable_thinking") {
        return Some(vec![
            EffortOption {
                value: OFF.into(),
                label: "Off".into(),
                description: None,
            },
            EffortOption {
                value: ON.into(),
                label: "On".into(),
                description: None,
            },
        ]);
    }
    if raw.is_some()
        || props
            .pointer("/chat_template_caps/supports_reasoning_effort")
            .and_then(Value::as_bool)
            == Some(false)
    {
        return Some(vec![]);
    }
    None
}
pub fn capabilities(props: &Value) -> Option<Vec<String>> {
    capability_options(props)
        .map(|options| options.into_iter().map(|option| option.value).collect())
}
pub fn select(requested: &str, levels: &[String]) -> Option<String> {
    let requested = normalize(requested);
    if requested.is_empty() || requested == "default" || requested == NONE || levels.is_empty() {
        return None;
    }
    if levels.contains(&requested) {
        return Some(requested);
    }
    if requested == "extra-high" && levels.iter().any(|value| value == "xhigh") {
        return Some("xhigh".into());
    }
    if levels.iter().any(|value| value == ON) {
        return Some(if requested == "none" { OFF } else { ON }.into());
    }
    if let Some(value) = levels.iter().find(|value| {
        value.starts_with(PREFIX)
            && native_value(value).is_some_and(|value| value.to_string() == requested)
    }) {
        return Some(value.clone());
    }
    if requested == "max" || requested == "ultra" {
        return levels.last().cloned();
    }
    rank(&requested)
        .and_then(|rank| {
            levels
                .iter()
                .find(|value| self::rank(value).is_some_and(|candidate| candidate >= rank))
        })
        .cloned()
}
pub fn error_levels(body: &str) -> Vec<String> {
    let message = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| {
            value
                .pointer("/error/message")
                .or_else(|| value.get("message"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| body.to_owned());
    let lower = message.to_ascii_lowercase();
    let Some(index) = [
        "supported types are",
        "supported values are",
        "supported levels are",
        "supported efforts are",
        "supported values:",
        "supported types:",
        "valid values are",
        "valid values:",
    ]
    .iter()
    .find_map(|marker| lower.find(marker).map(|index| index + marker.len())) else {
        return vec![];
    };
    let tail = message[index..]
        .split(['\n', '\r'])
        .next()
        .unwrap_or("")
        .trim()
        .trim_end_matches('.');
    let mut values = Vec::new();
    for part in tail.split(',').flat_map(|part| part.split(" and ")) {
        let value = part
            .trim()
            .trim_start_matches("and ")
            .trim()
            .trim_matches(|ch| matches!(ch, '[' | ']' | ' '));
        let value = value.strip_suffix("(default)").unwrap_or(value).trim();
        if value.is_empty() {
            continue;
        }
        let quoted = (value.starts_with('\'') && value.ends_with('\''))
            || (value.starts_with('"') && value.ends_with('"'));
        let value = if quoted {
            Value::String(value[1..value.len() - 1].to_owned())
        } else {
            serde_json::from_str::<Value>(value)
                .ok()
                .filter(|value| !value.is_object() && !value.is_array())
                .unwrap_or_else(|| Value::String(value.to_owned()))
        };
        values.push(key(&value));
    }
    let mut seen = std::collections::HashSet::new();
    template_order(
        values
            .into_iter()
            .filter(|value| seen.insert(value.clone()))
            .collect(),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_values_are_not_limited_to_a_named_allowlist() {
        let props = json!({"reasoning_efforts":[{"value":"Deep-V3","label":"Careful exploration"},"ultra-next",2,false]});
        let options = capability_options(&props).unwrap();
        assert_eq!(options[0].value, "Deep-V3");
        assert_eq!(options[0].label, "Careful exploration");
        assert_eq!(native_value(&options[2].value), Some(json!(2)));
        assert_eq!(native_value(&options[3].value), Some(json!(false)));
        assert_eq!(
            select("Deep-V3", &capabilities(&props).unwrap()),
            Some("Deep-V3".into())
        );
    }
    #[test]
    fn stale_high_uses_supported_xhigh() {
        let levels = capabilities(
            &json!({"chat_template_caps":{"reasoning_effort_levels":["low","medium","xhigh"]}}),
        )
        .unwrap();
        assert_eq!(select("high", &levels).as_deref(), Some("xhigh"));
        assert_eq!(select("max", &levels).as_deref(), Some("xhigh"));
    }
    #[test]
    fn explicit_jinja_support_list_preserves_unfamiliar_names() {
        assert_eq!(
            error_levels(
                r#"{"error":{"message":"Unexpected reasoning effort high. Supported types are xhigh (default), medium, and low."}}"#
            ),
            vec!["low", "medium", "xhigh"]
        );
        assert_eq!(
            error_levels("Supported values are 'Adaptive-V2', 'Careful-Pro'."),
            vec!["Adaptive-V2", "Careful-Pro"]
        );
    }
    #[test]
    fn fallback_does_not_send_none_as_an_unsupported_native_effort() {
        assert_eq!(native_value(NONE), None);
        assert!(
            capabilities(&json!({"chat_template_caps":{"supports_reasoning_effort":false}}))
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            capabilities(&json!({"chat_template":"{% if enable_thinking %}think{% endif %}"}))
                .unwrap(),
            vec![OFF, ON]
        );
    }
}
