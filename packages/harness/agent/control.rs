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
    async fn pausing_before_paint_does_not_wait_for_an_invisible_frame() {
        let control = AgentControl::default();
        control.set_paused(true, false);
        tokio::time::timeout(std::time::Duration::from_millis(50), control.wait_for_compaction_paint("fixture", 1)).await.unwrap();
        control.set_paused(false, true);
        assert_eq!(control.state().restart_epoch, 1);
        assert!(!control.state().paused);
    }
}

#[derive(Clone)]
pub struct AgentControl {
    state: watch::Sender<State>,
}

impl Default for AgentControl {
    fn default() -> Self {
        let (state, _) = watch::channel(State::default());
        Self { state }
    }
}

impl AgentControl {
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
