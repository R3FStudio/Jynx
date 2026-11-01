use anyhow::{Result, anyhow};
use serde_json::{Value, json};
use std::collections::HashSet;

pub(super) fn ask_user(args: &Value) -> Result<String> {
    let items = args["questions"]
        .as_array()
        .ok_or_else(|| anyhow!("questions must be an array"))?;
    if items.is_empty() || items.len() > 3 {
        return Err(anyhow!("ask_user accepts one to three questions"));
    }
    let mut ids = HashSet::new();
    let mut questions = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let id = item["id"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| format!("question-{}", index + 1));
        let question = item["question"].as_str().unwrap_or_default().trim();
        if id.trim().is_empty()
            || id.len() > 64
            || !ids.insert(id.clone())
            || question.is_empty()
            || question.len() > 4000
        {
            return Err(anyhow!(
                "questions need unique ids and nonempty question text"
            ));
        }
        let options: Vec<String> = match item.get("options") {
            None | Some(Value::Null) => vec![],
            Some(Value::Array(values)) if values.len() <= 5 => values
                .iter()
                .map(|v| {
                    v.as_str()
                        .filter(|s| !s.trim().is_empty() && s.len() <= 500)
                        .map(str::to_owned)
                        .ok_or_else(|| anyhow!("options must be nonempty strings"))
                })
                .collect::<Result<_>>()?,
            _ => return Err(anyhow!("a question accepts up to five options")),
        };
        questions.push(json!({"id":id,"question":question,"options":options}));
    }
    Ok(json!({"questions":questions,"awaiting_user_input":true}).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn valid_questions_keep_choices_and_allow_free_text() {
        let result:Value=serde_json::from_str(&ask_user(&json!({"questions":[{"id":"scope","question":"Which scope?","options":["Small","Full"]},{"question":"Any constraints?"}]})).unwrap()).unwrap();
        assert_eq!(result["questions"][0]["options"][1], "Full");
        assert_eq!(result["questions"][1]["id"], "question-2");
        assert_eq!(result["awaiting_user_input"], true);
    }
    #[test]
    fn invalid_questions_are_rejected() {
        for value in [
            json!({"questions":[]}),
            json!({"questions":[{"question":""}]}),
            json!({"questions":[{"id":"x","question":"A"},{"id":"x","question":"B"}]}),
        ] {
            assert!(ask_user(&value).is_err());
        }
    }
}
