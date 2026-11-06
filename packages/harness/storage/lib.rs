use anyhow::{Result, anyhow};
use fs2::FileExt;
use sha2::{Digest, Sha256};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use tokio::fs;

#[derive(Debug, Clone)]
pub struct JsonStore {
    root: PathBuf,
}

impl JsonStore {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }
    pub async fn write<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        fs::create_dir_all(&self.root).await?;
        let temporary = self.root.join(format!(".{key}-{}.tmp", uuid::Uuid::new_v4()));
        fs::write(&temporary, serde_json::to_vec_pretty(value)?).await?;
        if let Err(error) = fs::rename(&temporary, self.root.join(format!("{key}.json"))).await {
            let _ = fs::remove_file(&temporary).await;
            return Err(error.into());
        }
        Ok(())
    }
    pub async fn read<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        match fs::read(self.root.join(format!("{key}.json"))).await {
            Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChatStore {
    store: JsonStore,
}

impl ChatStore {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            store: JsonStore::new(root),
        }
    }

    async fn lock(&self) -> Result<std::fs::File> {
        let root = self.store.root.clone();
        tokio::task::spawn_blocking(move || -> Result<std::fs::File> {
            std::fs::create_dir_all(&root)?;
            let file = std::fs::OpenOptions::new().create(true).truncate(false).read(true).write(true).open(root.join("threads.lock"))?;
            file.lock_exclusive()?;
            Ok(file)
        }).await?
    }

    pub async fn import_legacy(&self, source: impl AsRef<Path>) -> Result<Value> {
        let source = source.as_ref();
        if source == self.store.root { return Ok(json!({"imported":0})); }
        let _guard = self.lock().await?;
        let marker = self.store.root.join(format!("imported-{}.json", format!("{:x}", Sha256::digest(source.to_string_lossy().as_bytes()))));
        if fs::try_exists(&marker).await? { return Ok(json!({"imported":0})); }
        let incoming = match fs::read(source.join("threads.json")).await {
            Ok(bytes) => serde_json::from_slice::<Vec<Value>>(&bytes)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(json!({"imported":0})),
            Err(error) => return Err(error.into()),
        };
        let mut threads = self.read_threads().await?;
        let mut imported = 0;
        for thread in incoming {
            let id = Self::id_of(&thread);
            if !id.is_empty() && !threads.iter().any(|entry| Self::id_of(entry) == id) {
                threads.push(thread);
                imported += 1;
            }
        }
        if imported > 0 { self.write_threads(&threads).await?; }
        fs::write(marker, b"true").await?;
        Ok(json!({"imported":imported}))
    }

    pub async fn import_known_legacy(&self, cwd: impl AsRef<Path>) -> Result<()> {
        if std::env::var("JYNX_LEGACY_IMPORT").is_ok_and(|value| value == "off")
            || (std::env::var_os("JYNX_STATE_DIR").is_some() && std::env::var_os("JYNX_HISTORY_DIR").is_none()) {
            return Ok(());
        }
        let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from);
        let app_data = if cfg!(target_os = "windows") {
            std::env::var_os("APPDATA").map(PathBuf::from).or_else(|| home.as_ref().map(|root|root.join("AppData/Roaming")))
        } else if cfg!(target_os = "macos") {
            home.as_ref().map(|root|root.join("Library/Application Support"))
        } else {
            std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).or_else(|| home.as_ref().map(|root|root.join(".config")))
        };
        for source in legacy_history_sources(cwd.as_ref(), app_data.as_deref()) { self.import_legacy(source).await?; }
        Ok(())
    }

    async fn read_threads(&self) -> Result<Vec<Value>> {
        Ok(self
            .store
            .read::<Vec<Value>>("threads")
            .await?
            .unwrap_or_default())
    }

    async fn write_threads(&self, threads: &[Value]) -> Result<()> {
        self.store.write("threads", &threads).await
    }

    fn id_of(value: &Value) -> &str {
        value["id"].as_str().unwrap_or_default()
    }

    pub async fn list(&self, archived: bool, search: Option<&str>) -> Result<Value> {
        let _guard = self.lock().await?;
        let mut threads = self.read_threads().await?;
        let needle = search.unwrap_or_default().trim().to_lowercase();
        threads.retain(|thread| {
            let is_archived = thread["archived"].as_bool().unwrap_or(false);
            if is_archived != archived {
                return false;
            }
            if needle.is_empty() {
                return true;
            }
            thread.to_string().to_lowercase().contains(&needle)
        });
        threads.sort_by(|a, b| {
            let left = a["updatedAt"]
                .as_i64()
                .or_else(|| a["updated_at"].as_i64())
                .unwrap_or(0);
            let right = b["updatedAt"]
                .as_i64()
                .or_else(|| b["updated_at"].as_i64())
                .unwrap_or(0);
            right.cmp(&left)
        });
        Ok(json!({"data":threads,"nextCursor":Value::Null}))
    }



    pub async fn list_summaries(&self, archived: bool, search: Option<&str>) -> Result<Value> {
        let mut response=self.list(archived,None).await?;
        let query=search.unwrap_or_default().to_lowercase();
        let terms=query.split_whitespace().collect::<Vec<_>>();
        if let Some(threads)=response["data"].as_array_mut(){
            if !terms.is_empty(){threads.retain(|thread|{let text=thread.to_string().to_lowercase();terms.iter().all(|term|text.contains(term))});}
            for thread in threads {
                let preview=thread["preview"].as_str().or_else(||thread["title"].as_str()).or_else(|| {
                    thread["turns"].as_array()?.iter().flat_map(|turn|turn["items"].as_array().into_iter().flatten())
                      .find(|item|item["type"]=="userMessage")?["content"].as_array()?.iter()
                      .find(|part|part["type"]=="text")?["text"].as_str()
                }).unwrap_or("").chars().take(240).collect::<String>();
                let turn_count=thread["turns"].as_array().map(Vec::len).unwrap_or(0);
                let mut original=std::mem::take(thread);
                let mut summary=serde_json::Map::new();
                for key in ["id","name","title","cwd","model","modelProvider","model_provider","createdAt","created_at","updatedAt","updated_at","archived","tokenUsage","token_usage","status"] {
                    if let Some(value)=original.as_object_mut().and_then(|object|object.remove(key)){summary.insert(key.into(),value);}
                }
                if !summary.contains_key("title")&&!preview.is_empty(){summary.insert("title".into(),json!(preview));}
                summary.insert("preview".into(),json!(preview));summary.insert("turnCount".into(),json!(turn_count));
                *thread=Value::Object(summary);
            }
        }
        Ok(response)
    }

    pub async fn save(&self, mut thread: Value) -> Result<Value> {
        let _guard = self.lock().await?;
        let id = Self::id_of(&thread).to_string();
        if id.is_empty() {
            return Err(anyhow!("thread is missing id"));
        }
        let mut threads = self.read_threads().await?;
        if let Some(existing) = threads.iter().find(|entry| Self::id_of(entry) == id) {
            if let (Some(old), Some(next)) = (existing.as_object(), thread.as_object_mut()) {
                for (key, value) in old { next.entry(key.clone()).or_insert_with(|| value.clone()); }
            }
        }
        threads.retain(|entry| Self::id_of(entry) != id);
        threads.insert(0, thread.clone());
        self.write_threads(&threads).await?;
        Ok(json!({"thread":thread}))
    }

    pub async fn set_message_feedback(&self,id:&str,message_id:&str,feedback:Option<&str>,updated_at:u64)->Result<Value>{
        let _guard = self.lock().await?;
        if feedback.is_some_and(|value|value!="like"&&value!="dislike"){return Err(anyhow!("invalid message rating"));}
        let mut threads=self.read_threads().await?;
        let thread=threads.iter_mut().find(|thread|Self::id_of(thread)==id).ok_or_else(||anyhow!("thread not found"))?;
        let turns=thread["turns"].as_array_mut().ok_or_else(||anyhow!("thread has no turns"))?;
        let mut found=false;
        for turn in turns{
            if !matches!(turn["status"].as_str(),Some("completed"|"complete"|"succeeded"|"success")){continue;}
            let Some(items)=turn["items"].as_array_mut() else{continue;};
            let last=items.iter().rposition(|item|item["type"]=="agentMessage"&&item["delivery"]!="async"&&!item["text"].as_str().unwrap_or("").trim().is_empty());
            let Some(index)=last else{continue;};
            if items[index]["id"]!=message_id{continue;}
            let item=items[index].as_object_mut().ok_or_else(||anyhow!("invalid answer"))?;
            if let Some(value)=feedback{item.insert("feedback".into(),json!(value));}else{item.remove("feedback");}
            item.insert("feedbackUpdatedAt".into(),json!(updated_at));
            found=true;break;
        }
        if !found{return Err(anyhow!("completed answer not found"));}
        self.write_threads(&threads).await?;
        Ok(json!({"ok":true}))
    }

    pub async fn read(&self, id: &str) -> Result<Value> {
        let _guard = self.lock().await?;
        let thread = self
            .read_threads()
            .await?
            .into_iter()
            .find(|entry| Self::id_of(entry) == id);
        Ok(json!({"thread":thread}))
    }

    pub async fn turns(&self, id: &str) -> Result<Value> {
        let _guard = self.lock().await?;
        let thread = self
            .read_threads()
            .await?
            .into_iter()
            .find(|entry| Self::id_of(entry) == id);
        let turns = thread
            .and_then(|entry| entry["turns"].as_array().cloned())
            .unwrap_or_default();
        Ok(json!({"data":turns,"nextCursor":Value::Null}))
    }

    pub async fn set_archived(&self, id: &str, archived: bool) -> Result<Value> {
        let _guard = self.lock().await?;
        let mut threads = self.read_threads().await?;
        for thread in &mut threads {
            if Self::id_of(thread) == id {
                thread["archived"] = Value::Bool(archived);
            }
        }
        self.write_threads(&threads).await?;
        Ok(json!({"ok":true}))
    }

    pub async fn rename(&self, id: &str, name: &str) -> Result<Value> {
        let _guard = self.lock().await?;
        let mut threads = self.read_threads().await?;
        for thread in &mut threads {
            if Self::id_of(thread) == id {
                thread["name"] = Value::String(name.to_string());
                thread["title"] = Value::String(name.to_string());
            }
        }
        self.write_threads(&threads).await?;
        Ok(json!({"ok":true}))
    }

    pub async fn delete(&self, id: &str) -> Result<Value> {
        let _guard = self.lock().await?;
        let mut threads = self.read_threads().await?;
        let before = threads.len();
        threads.retain(|entry| Self::id_of(entry) != id);
        self.write_threads(&threads).await?;
        Ok(json!({"ok":threads.len() != before}))
    }

    pub async fn fork(&self, id: &str) -> Result<Value> {
        let _guard = self.lock().await?;
        let mut threads = self.read_threads().await?;
        let Some(mut thread) = threads
            .iter()
            .find(|entry| Self::id_of(entry) == id)
            .cloned()
        else {
            return Ok(json!({}));
        };
        let new_id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        let title = thread["title"].as_str().unwrap_or("Chat").to_string();
        thread["id"] = Value::String(new_id);
        thread["title"] = Value::String(format!("{title} — Copy"));
        let copied_title = thread["title"].clone();
        thread["name"] = copied_title;
        thread["createdAt"] = json!(now);
        thread["updatedAt"] = json!(now);
        threads.insert(0, thread.clone());
        self.write_threads(&threads).await?;
        Ok(json!({"thread":thread}))
    }
}

fn merge_json(target: &mut Value, patch: Value) {
    match (target, patch) {
        (Value::Object(target), Value::Object(patch)) => {
            for (key, value) in patch {
                if value.is_null() {
                    target.remove(&key);
                } else if let Some(existing) = target.get_mut(&key) {
                    merge_json(existing, value);
                } else {
                    target.insert(key, value);
                }
            }
        }
        (target, patch) => *target = patch,
    }
}

#[derive(Debug, Clone)]
pub struct RuntimeStateStore {
    store: JsonStore,
}

impl RuntimeStateStore {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            store: JsonStore::new(root),
        }
    }

    pub async fn read(&self) -> Result<Value> {
        Ok(self
            .store
            .read::<Value>("runtime-state")
            .await?
            .unwrap_or_else(|| json!({})))
    }

    pub async fn update(&self, patch: Value) -> Result<Value> {
        if !patch.is_object() {
            return Err(anyhow!("runtime state patch must be an object"));
        }
        let mut state = self.read().await?;
        if !state.is_object() {
            state = json!({});
        }
        merge_json(&mut state, patch);
        self.store.write("runtime-state", &state).await?;
        Ok(json!({"state":state}))
    }

    pub async fn clear(&self) -> Result<Value> {
        let state = json!({});
        self.store.write("runtime-state", &state).await?;
        Ok(json!({"state":state}))
    }
}

fn legacy_history_sources(cwd: &Path, app_data: Option<&Path>) -> Vec<PathBuf> {
    let mut sources = vec![cwd.join(".jynx-state"),cwd.join(".daemon-state")];
    if let Some(root) = app_data {
        for app in ["Jynx", "Daemon Studio"] {
            for directory in ["jynx-harness-state", "daemon-harness-state"] { sources.push(root.join(app).join(directory)); }
        }
    }
    sources
}

pub fn default_history_root() -> PathBuf {
    std::env::var_os("JYNX_HISTORY_DIR").or_else(|| std::env::var_os("JYNX_STATE_DIR")).map(PathBuf::from).unwrap_or_else(|| {
        let home = std::env::var_os("JYNX_HOME").map(PathBuf::from).unwrap_or_else(|| {
            std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from).unwrap_or_else(|| PathBuf::from(".")).join(".jynx")
        });
        home.join("history")
    })
}

pub fn default_state_root() -> PathBuf {
    std::env::var_os("JYNX_STATE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            migrate_state_root(&std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        })
}


fn migrate_state_root(parent: &Path) -> PathBuf {
    let current = parent.join(".jynx-state");
    let legacy = parent.join(".daemon-state");
    if current.exists() || !legacy.exists() { return current; }
    match std::fs::rename(&legacy, &current) {
        Ok(()) => current,
        Err(_) => legacy,
    }
}

#[cfg(test)]
mod feedback_tests{
    use super::*;
    #[tokio::test]
    async fn independent_ratings_preserve_other_messages_and_clear_after_reopening(){
        let root=std::env::temp_dir().join(format!("jynx-feedback-test-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let store=ChatStore::new(&root);
        store.save(json!({"id":"synthetic","turns":[
            {"id":"one","status":"completed","items":[{"id":"one","type":"agentMessage","text":"Synthetic first answer"}]},
            {"id":"two","status":"completed","items":[{"id":"two","type":"agentMessage","text":"Synthetic second answer"}]},
            {"id":"live","status":"inProgress","items":[{"id":"live","type":"agentMessage","text":"Synthetic active content"}]}
        ]})).await.unwrap();
        store.set_message_feedback("synthetic","one",Some("like"),1).await.unwrap();
        store.set_message_feedback("synthetic","two",Some("dislike"),2).await.unwrap();
        let reopened=ChatStore::new(&root);
        let read=reopened.read("synthetic").await.unwrap();
        assert_eq!(read["thread"]["turns"][0]["items"][0]["feedback"],"like");
        assert_eq!(read["thread"]["turns"][1]["items"][0]["feedback"],"dislike");
        assert_eq!(read["thread"]["turns"][2]["items"][0]["text"],"Synthetic active content");
        assert!(reopened.set_message_feedback("synthetic","live",Some("like"),3).await.is_err());
        assert!(reopened.set_message_feedback("synthetic","one",Some("other"),3).await.is_err());
        reopened.set_message_feedback("synthetic","one",None,4).await.unwrap();
        assert!(reopened.read("synthetic").await.unwrap()["thread"]["turns"][0]["items"][0].get("feedback").is_none());
        std::fs::remove_file(root.join("threads.json")).unwrap();std::fs::remove_file(root.join("threads.lock")).unwrap();std::fs::remove_dir(root).unwrap();
    }
}

#[cfg(test)]
mod summary_tests {
 use super::*;
 #[tokio::test]
 async fn sidebar_summaries_preserve_search_and_full_storage_without_sending_audits(){
  let root=std::env::temp_dir().join(format!("jynx-summary-test-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
  let store=ChatStore::new(&root);let raw="SYNTHETIC_AUDIT_MATCH ".repeat(50_000);
  let thread=json!({"id":"synthetic","cwd":"synthetic-workspace","updatedAt":42,"archived":false,"contextCheckpoint":{"messages":[{"role":"assistant","content":raw}],"throughTurnId":"old"},"turns":[{"id":"old","status":"completed","items":[{"id":"user","type":"userMessage","content":[{"type":"text","text":"Synthetic project question"}]},{"id":"tool","type":"fileRead","output":raw}]}]});
  store.save(thread.clone()).await.unwrap();
  let full=store.list(false,None).await.unwrap();let summary=store.list_summaries(false,None).await.unwrap();
  assert_eq!(summary["data"][0]["preview"],"Synthetic project question");assert_eq!(summary["data"][0]["turnCount"],1);
  assert!(summary["data"][0].get("turns").is_none());assert!(summary["data"][0].get("contextCheckpoint").is_none());
  assert!(summary.to_string().len()*1000<full.to_string().len(),"summary should be over 1000x smaller than synthetic full data");
  assert_eq!(store.list_summaries(false,Some("SYNTHETIC_AUDIT_MATCH")).await.unwrap()["data"].as_array().unwrap().len(),1);
  assert_eq!(store.list_summaries(false,Some("SYNTHETIC_AUDIT_MATCH project")).await.unwrap()["data"].as_array().unwrap().len(),1);
  assert_eq!(store.read("synthetic").await.unwrap()["thread"],thread,"summaries must not alter stored context");
  println!("synthetic list bytes: full={}, summary={}",full.to_string().len(),summary.to_string().len());
  std::fs::remove_file(root.join("threads.json")).unwrap();std::fs::remove_file(root.join("threads.lock")).unwrap();std::fs::remove_dir(root).unwrap();
 }
}

#[cfg(test)]
mod branding_migration_tests {
    use super::*;
    #[test]
    fn standalone_state_moves_once_without_reading_or_replacing_existing_state() {
        let parent=std::env::temp_dir().join(format!("jynx-branding-{}",std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir(&parent).unwrap();
        let old=parent.join(".daemon-state");std::fs::create_dir(&old).unwrap();
        std::fs::write(old.join("fixture-marker.txt"),"synthetic marker").unwrap();
        let current=migrate_state_root(&parent);assert_eq!(current,parent.join(".jynx-state"));
        assert_eq!(std::fs::read_to_string(current.join("fixture-marker.txt")).unwrap(),"synthetic marker");
        std::fs::create_dir(&old).unwrap();
        assert_eq!(migrate_state_root(&parent),current);assert!(old.exists());
        std::fs::remove_file(current.join("fixture-marker.txt")).unwrap();
        for directory in [&old,&current,&parent] {std::fs::remove_dir(directory).unwrap();}
    }
}

#[cfg(test)]
mod shared_history_tests {
    use super::*;
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn independent_clients_keep_all_threads_and_preserve_archive_metadata() {
        let root = std::env::temp_dir().join(format!("jynx-shared-test-{}", uuid::Uuid::new_v4()));
        let mut tasks = Vec::new();
        for index in 0..24 {
            let store = ChatStore::new(&root);
            tasks.push(tokio::spawn(async move { store.save(json!({"id":format!("synthetic-{index}"),"title":"Synthetic","turns":[]})).await.unwrap(); }));
        }
        for task in tasks { task.await.unwrap(); }
        let store = ChatStore::new(&root);
        assert_eq!(store.list(false,None).await.unwrap()["data"].as_array().unwrap().len(),24);
        store.set_archived("synthetic-0",true).await.unwrap();
        store.save(json!({"id":"synthetic-0","title":"Updated","turns":[]})).await.unwrap();
        assert_eq!(store.list(true,None).await.unwrap()["data"][0]["archived"],true);
        store.delete("synthetic-1").await.unwrap();
        assert!(store.read("synthetic-1").await.unwrap()["thread"].is_null());
        std::fs::remove_file(root.join("threads.json")).unwrap();
        std::fs::remove_file(root.join("threads.lock")).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
    #[tokio::test]
    async fn legacy_import_keeps_originals_and_does_not_resurrect_deleted_chats() {
        let root = std::env::temp_dir().join(format!("jynx-import-test-{}",uuid::Uuid::new_v4()));
        let source = root.join("legacy");
        let target = root.join("shared");
        fs::create_dir_all(&source).await.unwrap();
        fs::write(source.join("threads.json"), br#"[{"id":"synthetic","title":"Legacy","turns":[]}]"#).await.unwrap();
        let store = ChatStore::new(&target);
        assert_eq!(store.import_legacy(&source).await.unwrap()["imported"],1);
        store.delete("synthetic").await.unwrap();
        assert_eq!(store.import_legacy(&source).await.unwrap()["imported"],0);
        assert!(fs::try_exists(source.join("threads.json")).await.unwrap());
        for folder in [&source,&target] {
            let mut entries = fs::read_dir(folder).await.unwrap();
            while let Some(entry) = entries.next_entry().await.unwrap() { fs::remove_file(entry.path()).await.unwrap(); }
            fs::remove_dir(folder).await.unwrap();
        }
        fs::remove_dir(&root).await.unwrap();
    }
}

#[cfg(test)]
mod desktop_history_import_tests {
    use super::*;
    #[tokio::test]
    async fn known_desktop_and_workspace_histories_merge_without_replacing_existing_chats() {
        let root = std::env::temp_dir().join(format!("jynx-desktop-import-test-{}",uuid::Uuid::new_v4()));
        let app_data = root.join("app-data");
        let cwd = root.join("workspace");
        let shared = root.join("shared");
        let sources = legacy_history_sources(&cwd,Some(&app_data));
        let desktop = app_data.join("Jynx/jynx-harness-state");
        assert!(sources.contains(&desktop));
        let workspace = cwd.join(".jynx-state");
        for (directory,id) in [(&desktop,"desktop-synthetic"),(&workspace,"workspace-synthetic")] {
            fs::create_dir_all(directory).await.unwrap();
            fs::write(directory.join("threads.json"),serde_json::to_vec(&json!([{"id":id,"title":"Synthetic imported chat","turns":[]}])).unwrap()).await.unwrap();
        }
        let store = ChatStore::new(&shared);
        store.save(json!({"id":"existing-synthetic","title":"Keep existing","turns":[]})).await.unwrap();
        for source in sources {store.import_legacy(source).await.unwrap();}
        assert_eq!(store.list(false,None).await.unwrap()["data"].as_array().unwrap().len(),3);
        assert_eq!(store.read("existing-synthetic").await.unwrap()["thread"]["title"],"Keep existing");
        for folder in [&desktop,&workspace,&shared] {
            let mut entries = fs::read_dir(folder).await.unwrap();
            while let Some(entry) = entries.next_entry().await.unwrap() { fs::remove_file(entry.path()).await.unwrap(); }
            fs::remove_dir(folder).await.unwrap();
        }
        fs::remove_dir(app_data.join("Jynx")).await.unwrap();
        fs::remove_dir(app_data).await.unwrap();
        fs::remove_dir(cwd).await.unwrap();
        fs::remove_dir(root).await.unwrap();
    }
}
