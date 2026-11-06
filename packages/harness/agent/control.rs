use std::{sync::{Arc,Mutex},collections::HashMap};
use tokio::sync::{oneshot,mpsc::UnboundedSender};
use serde_json::{Value,json};
use tokio::sync::watch;

#[derive(Clone, Default)]
pub(crate) struct State {
    pub paused: bool,
    pub restart_epoch: u64,
    pub visible_compaction: Option<(String, u64)>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn editor_requests_are_matched_and_cancel_without_pending_entries(){
        let control=AgentControl::default();let(events,mut observed)=tokio::sync::mpsc::unbounded_channel();
        let caller=control.clone();let tx=events.clone();let task=tokio::spawn(async move{caller.request_editor_tool("get_diagnostics",json!({}),&tx).await});
        let request=observed.recv().await.unwrap();let id=request["id"].as_str().unwrap();
        assert!(!control.resolve_editor_tool("wrong",json!([])));
        assert!(control.resolve_editor_tool(id,json!([{"message":"fixture"}])));
        assert_eq!(task.await.unwrap().unwrap()[0]["message"],"fixture");
        assert!(!control.resolve_editor_tool(id,json!([])));
        let caller=control.clone();let task=tokio::spawn(async move{caller.request_editor_tool("list_tasks",json!({}),&events).await});observed.recv().await.unwrap();task.abort();let _=task.await;
        assert!(control.editor.lock().unwrap().is_empty());
    }
    #[tokio::test]
    async fn pausing_before_paint_does_not_wait_for_an_invisible_frame() {
        let control = AgentControl::default();
        control.set_paused(true, false);
        tokio::time::timeout(std::time::Duration::from_millis(50), control.wait_for_compaction_paint("fixture", 1)).await.unwrap();
        control.set_paused(false, true);
        assert_eq!(control.state().restart_epoch, 1);
        assert!(!control.state().paused);
    }
}

struct EditorRegistration { pending:Arc<Mutex<HashMap<String,oneshot::Sender<Value>>>>,id:String }
impl Drop for EditorRegistration {fn drop(&mut self){self.pending.lock().unwrap_or_else(|e|e.into_inner()).remove(&self.id);}}
#[derive(Clone)]
pub struct AgentControl {
    state: watch::Sender<State>,
    editor: Arc<Mutex<HashMap<String,oneshot::Sender<Value>>>>,
}

impl Default for AgentControl {
    fn default() -> Self {
        let (state, _) = watch::channel(State::default());
        Self { state, editor:Arc::new(Mutex::new(HashMap::new())) }
    }
}

impl AgentControl {
    pub fn resolve_editor_tool(&self,id:&str,value:Value)->bool {
        self.editor.lock().unwrap_or_else(|e|e.into_inner()).remove(id).is_some_and(|sender|sender.send(value).is_ok())
    }
    pub(crate) async fn request_editor_tool(&self,name:&str,arguments:Value,events:&UnboundedSender<Value>)->anyhow::Result<Value> {
        let id=uuid::Uuid::new_v4().to_string();let(sender,receiver)=oneshot::channel();
        self.editor.lock().unwrap_or_else(|e|e.into_inner()).insert(id.clone(),sender);
        let _registration=EditorRegistration{pending:self.editor.clone(),id:id.clone()};
        if events.send(json!({"type":"editor_tool_required","id":id,"kind":name,"arguments":arguments})).is_err(){self.editor.lock().unwrap_or_else(|e|e.into_inner()).remove(&id);return Err(anyhow::anyhow!("Host disconnected"));}
        let result=tokio::select!{value=receiver=>value.map_err(|_|anyhow::anyhow!("Host response closed")),_=tokio::time::sleep(std::time::Duration::from_secs(if name=="run_in_terminal_panel" { 660 } else { 30 }))=>Err(anyhow::anyhow!("Editor tool timed out")),_=events.closed()=>Err(anyhow::anyhow!("Host disconnected"))};
        self.editor.lock().unwrap_or_else(|e|e.into_inner()).remove(&id);result
    }
    pub fn set_paused(&self, paused: bool, restart_compaction: bool) {
        self.state.send_modify(|state| {
            if state.paused && !paused && restart_compaction {
                state.restart_epoch += 1;
            }
            state.paused = paused;
        });
    }

    pub(crate) fn state(&self) -> State { self.state.borrow().clone() }

    pub fn acknowledge_compaction(&self, id: String, attempt: u64) {
        self.state.send_modify(|state| state.visible_compaction = Some((id, attempt)));
    }

    pub(crate) async fn wait_for_compaction_paint(&self, id: &str, attempt: u64) {
        let mut receiver = self.state.subscribe();
        loop {
            {
                let state = receiver.borrow_and_update();
                if state.paused || state.visible_compaction.as_ref().is_some_and(|(visible_id, visible_attempt)| visible_id == id && *visible_attempt == attempt) { return; }
            }
            if receiver.changed().await.is_err() { return; }
        }
    }

    pub(crate) async fn wait_until_resumed(&self) {
        let mut receiver = self.state.subscribe();
        loop {
            if !receiver.borrow_and_update().paused { return; }
            if receiver.changed().await.is_err() { return; }
        }
    }
}
