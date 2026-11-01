use serde_json::Value;

pub fn format_response(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        if lines[index].trim().starts_with("```") && lines[index].trim() != "```chart" {
            let start = index;
            index += 1;
            while index < lines.len() && lines[index].trim() != "```" {
                index += 1;
            }
            if index < lines.len() {
                index += 1;
            }
            out.push(lines[start..index].join("\n"));
            continue;
        }
        if lines[index].trim() == "```chart" {
            let start = index;
            index += 1;
            let mut data = Vec::new();
            while index < lines.len() && lines[index].trim() != "```" {
                data.push(lines[index]);
                index += 1;
            }
            if index < lines.len() {
                if let Some(chart) = text_chart(&data.join("\n")) {
                    out.push(chart);
                } else {
                    out.push(lines[start..=index].join("\n"));
                }
                index += 1;
            } else {
                out.push(lines[start..].join("\n"));
            }
        } else {
            out.push(lines[index].to_string());
            index += 1;
        }
    }
    let mut result = out.join("\n");
    if text.ends_with('\n') {
        result.push('\n');
    }
    result
}
fn text_chart(text: &str) -> Option<String> {
    let v: Value = serde_json::from_str(text).ok()?;
    let kind = v["type"].as_str()?;
    if !matches!(kind, "bar" | "line" | "area" | "pie" | "donut") {
        return None;
    }
    let labels = v
        .get("labels")
        .or_else(|| v.get("data")?.get("labels"))?
        .as_array()?;
    if labels.is_empty() || labels.len() > 100 {
        return None;
    }
    let shorthand = serde_json::json!([{"name":v["title"],"values":v["values"]}]);
    let series = v
        .get("series")
        .or_else(|| v.get("data")?.get("datasets"))
        .unwrap_or(&shorthand)
        .as_array()?;
    if series.is_empty() || series.len() > 6 || matches!(kind, "pie" | "donut") && series.len() != 1
    {
        return None;
    }
    let mut out = String::new();
    if let Some(title) = v["title"].as_str() {
        out.push_str(title);
        out.push('\n');
    }
    for item in series {
        let values = item
            .get("values")
            .or_else(|| item.get("data"))?
            .as_array()?;
        if values.len() != labels.len() {
            return None;
        }
        let values: Vec<f64> = values.iter().map(Value::as_f64).collect::<Option<_>>()?;
        if values.iter().any(|value| !value.is_finite()) {
            return None;
        }
        if matches!(kind, "pie" | "donut")
            && (values.iter().any(|value| *value < 0.0) || values.iter().all(|value| *value == 0.0))
        {
            return None;
        }
        let scale = values.iter().fold(0_f64, |a, b| a.max(b.abs()));
        let max = if scale == 0.0 { 1.0 } else { scale };
        if let Some(name) = item["name"].as_str().or_else(|| item["label"].as_str()) {
            out.push_str(name);
            out.push('\n');
        }
        for (label, value) in labels.iter().zip(values) {
            let name = label.as_str()?.chars().take(22).collect::<String>();
            let count = (value.abs() / max * 28.0).round() as usize;
            out.push_str(&format!(
                "  {name:22} {}{} {value}\n",
                if value < 0.0 { "-" } else { "" },
                "#".repeat(count)
            ));
        }
    }
    Some(out.trim_end().into())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn charts_become_terminal_graphics_without_modifying_context_text() {
        let source = "Before\n```chart\n{\"type\":\"bar\",\"labels\":[\"A\",\"B\"],\"values\":[2,4]}\n```\nAfter";
        let formatted = format_response(source);
        assert!(formatted.contains("####"));
        assert!(formatted.contains("After"));
        assert!(!formatted.contains("```chart"));
    }
    #[test]
    fn invalid_charts_and_regular_code_are_preserved() {
        for source in [
            "```chart\nnot json\n```",
            "```rust\nfn main() {}\n```",
            "```chart\n{\"type\":\"bad\"}\n```",
        ] {
            assert_eq!(format_response(source), source);
        }
    }
}
