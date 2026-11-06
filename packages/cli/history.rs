use anyhow::{Result, anyhow};
use jynx_protocol::{ChatMessage, ToolEvent};
use jynx_storage::{ChatStore, default_history_root, default_state_root};
use serde_json::{Value, json};

pub struct History {
    thread: Value,
}
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn restored_context(thread: &Value) -> Vec<ChatMessage> {
    let turns = thread["turns"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    let checkpoint = &thread["contextCheckpoint"];
    let anchor = checkpoint["throughTurnId"].as_str().and_then(|id| {
        turns
            .iter()
            .position(|turn| turn["id"].as_str() == Some(id))
    });
    let restored = anchor.and_then(|index| {
        serde_json::from_value::<Vec<ChatMessage>>(checkpoint["messages"].clone())
            .ok()
            .map(|messages| (index, messages))
    });
    let (start, mut messages) = restored
        .map(|(index, messages)| (index + 1, messages))
        .unwrap_or((0, Vec::new()));
    for turn in &turns[start..] {
        if turn["manualCompaction"] == true {
            continue;
        }
        for item in turn["items"].as_array().into_iter().flatten() {
            let (role, body) = match item["type"].as_str() {
                Some("userMessage") => (
                    "user",
                    item["content"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|part| part["text"].as_str())
                        .collect::<Vec<_>>()
                        .join("\n"),
                ),
                Some("agentMessage") => {
                    ("assistant", item["text"].as_str().unwrap_or("").to_owned())
                }
                _ => continue,
            };
            if !body.is_empty() {
                messages.push(ChatMessage {
                    phase: if role == "assistant" {
                        item["phase"].as_str().map(str::to_owned).or_else(|| {
                            item["delivery"].as_str().map(|delivery| {
                                if delivery == "async" {
                                    "commentary".into()
                                } else {
                                    "final_answer".into()
                                }
                            })
                        })
                    } else {
                        None
                    },
                    role: role.into(),
                    content: body,
                    images: Vec::new(),
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                });
            }
        }
    }
    messages
}

impl History {
    pub fn new(cwd: &str, model: &str) -> Self {
        Self {
            thread: json!({"id":uuid::Uuid::new_v4().to_string(),"cwd":cwd,"model":model,"createdAt":now(),"updatedAt":now(),"turns":[]}),
        }
    }
    async fn store() -> Result<ChatStore> {
        let store = ChatStore::new(default_history_root());
        store.import_legacy(default_state_root()).await?;
        store.import_known_legacy(std::env::current_dir()?).await?;
        Ok(store)
    }
    pub async fn select(cwd: &str) -> Result<Option<(Self, Vec<ChatMessage>)>> {
        let store = Self::store().await?;
        let mut archived = false;
        loop {
            let data = store.list_summaries(archived, None).await?;
            let entries = data["data"]
                .as_array()
                .ok_or_else(|| anyhow!("Invalid history"))?
                .iter()
                .filter(|entry| entry["cwd"] == cwd)
                .collect::<Vec<_>>();
            let mut labels = entries
                .iter()
                .map(|entry| {
                    entry["name"]
                        .as_str()
                        .or_else(|| entry["title"].as_str())
                        .unwrap_or("Conversation")
                        .to_owned()
                })
                .collect::<Vec<_>>();
            labels.push(
                if archived {
                    "Show active chats"
                } else {
                    "Show archived chats"
                }
                .into(),
            );
            let selected = dialoguer::FuzzySelect::new()
                .with_prompt("Jynx chats")
                .items(&labels)
                .interact_opt()?;
            let Some(index) = selected else {
                return Ok(None);
            };
            if index == entries.len() {
                archived = !archived;
                continue;
            }
            let id = entries[index]["id"]
                .as_str()
                .ok_or_else(|| anyhow!("Missing conversation ID"))?;
            let response = store.read(id).await?;
            let thread = response["thread"].clone();
            let messages = restored_context(&thread);
            return Ok(Some((Self { thread }, messages)));
        }
    }
    pub async fn save_turn(
        &mut self,
        prompt: &str,
        answer: &str,
        tools: &[ToolEvent],
        context: &[ChatMessage],
        failed: bool,
    ) -> Result<()> {
        self.save_turn_in(
            &Self::store().await?,
            prompt,
            answer,
            tools,
            context,
            failed,
        )
        .await
    }
    async fn save_turn_in(
        &mut self,
        store: &ChatStore,
        prompt: &str,
        answer: &str,
        tools: &[ToolEvent],
        context: &[ChatMessage],
        failed: bool,
    ) -> Result<()> {
        let id = uuid::Uuid::new_v4().to_string();
        let mut items = vec![
            json!({"id":format!("{id}-user"),"type":"userMessage","content":[{"type":"text","text":prompt}]}),
        ];
        for tool in tools {
            let mut item = serde_json::to_value(tool)?;
            item["type"] = json!(match tool.kind.as_str() {
                "run_command" => "commandExecution",
                "write_file" | "edit_file" | "apply_patch" => "fileChange",
                "web_search" => "webSearch",
                "web_fetch" => "webFetch",
                "read_file" => "fileRead",
                "list_dir" | "list_files" => "fileList",
                _ => "toolCall",
            });
            item["tool"] = json!(tool.kind);
            item["displayLabel"] = json!(tool.label);
            item["modelOutput"] = json!(tool.output);
            items.push(item);
        }
        if !answer.is_empty() {
            items.push(json!({"id":format!("{id}-answer"),"type":"agentMessage","text":answer,"delivery":"final"}));
        }
        if !self.thread["turns"].is_array() {
            self.thread["turns"] = json!([]);
        }
        self.thread["turns"].as_array_mut().unwrap().push(json!({"id":id,"status":if failed {"failed"}else{"completed"},"startedAt":now(),"items":items}));
        if self.thread["title"].as_str().unwrap_or("").is_empty() {
            self.thread["title"] = json!(prompt.chars().take(120).collect::<String>());
        }
        self.thread["updatedAt"] = json!(now());
        self.thread["contextCheckpoint"] = json!({"messages":context,"throughTurnId":id});
        self.thread.as_object_mut().unwrap().remove("archived");
        store.save(self.thread.clone()).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restored_chats_append_the_checkpoint_tail_and_keep_threads_separate() {
        let record = |id: &str, marker: &str| json!({"id":id,"contextCheckpoint":{"throughTurnId":"anchor","messages":[{"role":"user","content":marker}]},"turns":[{"id":"anchor","items":[{"type":"agentMessage","text":"Old audit"}]},{"id":"tail","items":[{"type":"userMessage","content":[{"text":format!("{marker} follow-up")}]},{"type":"agentMessage","text":format!("{marker} answer")}]}]});
        let a = restored_context(&record("a", "Synthetic A"));
        let b = restored_context(&record("b", "Synthetic B"));
        assert_eq!(a.len(), 3);
        assert_eq!(b.len(), 3);
        assert!(
            a.iter()
                .all(|message| !message.content.contains("Synthetic B"))
        );
        assert!(
            b.iter()
                .all(|message| !message.content.contains("Synthetic A"))
        );
        assert_eq!(a[1].content, "Synthetic A follow-up");
        let mut stale = record("a", "Synthetic A");
        stale["contextCheckpoint"]["throughTurnId"] = json!("missing");
        let fallback = restored_context(&stale);
        assert_eq!(fallback[0].content, "Old audit");
        assert_eq!(fallback.len(), 3);
    }
    #[tokio::test]
    async fn cli_turns_are_shared_records_with_context_and_stable_archive_state() {
        let root =
            std::env::temp_dir().join(format!("jynx-cli-history-test-{}", uuid::Uuid::new_v4()));
        let store = ChatStore::new(&root);
        let mut history = History::new("/synthetic", "synthetic-model");
        let messages = vec![ChatMessage {
            phase: None,
            role: "user".into(),
            content: "Synthetic question".into(),
            images: vec![],
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }];
        history
            .save_turn_in(
                &store,
                "Synthetic question",
                "Synthetic answer",
                &[],
                &messages,
                false,
            )
            .await
            .unwrap();
        let reopened = ChatStore::new(&root);
        let entries = reopened.list(false, None).await.unwrap();
        let id = entries["data"][0]["id"].as_str().unwrap();
        assert_eq!(
            entries["data"][0]["turns"][0]["items"][0]["type"],
            "userMessage"
        );
        assert_eq!(
            entries["data"][0]["contextCheckpoint"]["messages"][0]["content"],
            "Synthetic question"
        );
        reopened.set_archived(id, true).await.unwrap();
        history
            .save_turn_in(
                &store,
                "Synthetic follow-up",
                "Synthetic continuation",
                &[],
                &messages,
                false,
            )
            .await
            .unwrap();
        assert_eq!(
            reopened.list(true, None).await.unwrap()["data"][0]["turns"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        std::fs::remove_file(root.join("threads.json")).unwrap();
        std::fs::remove_file(root.join("threads.lock")).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}
