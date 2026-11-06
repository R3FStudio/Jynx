use anyhow::Result;
use std::time::Instant;
use jynx_model::{ModelClient, ModelReply, progress_text_is_fragment};
use jynx_protocol::ChatMessage;
use serde_json::{Value, json};
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

#[derive(Default)]
pub(super) struct StepPresentation {
    enabled: bool,
    tool_intent: bool,
    pending: String,
    generated: String,
    first_text_at: Option<Instant>,
    last_text_at: Option<Instant>,
    initial_words: usize,
}

impl StepPresentation {
    fn consume(&mut self, event: Value, events: &UnboundedSender<Value>) {
        self.consume_at(event, events, Instant::now());
    }



    fn consume_at(&mut self, event: Value, events: &UnboundedSender<Value>, now: Instant) {
        if event["type"] == "stream_retry" {
            self.pending.clear(); self.tool_intent = false;
            self.generated.clear(); self.first_text_at=None; self.last_text_at=None; self.initial_words=0;
        }
        if event["type"] == "text_delta" {
            if let Some(text)=event["accumulated_text"].as_str(){self.generated=text.into();}
            else if let Some(text)=event["text"].as_str(){self.generated.push_str(text);}
            if !self.generated.is_empty(){
                if self.first_text_at.is_none(){self.first_text_at=Some(now);self.initial_words=self.generated.split_whitespace().count();}
                self.last_text_at=Some(now);
            }
        }
        if event["type"] == "status" && (event["tool_kind"].is_string()
            || event["label"] == "Preparing tool call") {
            self.tool_intent = true;
            self.commentary(events);
        }
        if event["type"] == "text_delta" && !self.tool_intent {
            if let Some(accumulated) = event["accumulated_text"].as_str() {
                self.pending = accumulated.into();
            } else if let Some(text) = event["text"].as_str() {
                self.pending.push_str(text);
            }
        } else if event["type"] == "text_delta" && !self.pending.is_empty() {
            if let Some(text) = event["text"].as_str() { self.pending.push_str(text); }
            self.commentary(events);
        } else {
            let _ = events.send(self.with_pace(event));
        }
    }

    fn with_pace(&self, mut event:Value)->Value {
        if event["type"]!="text_delta" && event["type"]!="final_answer" {return event;}
        if let (Some(first),Some(last))=(self.first_text_at,self.last_text_at){
            let words=self.generated.split_whitespace().count().saturating_sub(self.initial_words);
            let elapsed=last.saturating_duration_since(first).as_secs_f64()*1000.0;
            if words>=2 && elapsed>=20.0 {event["word_interval_ms"]=json!(elapsed/words as f64);}
        }
        event
    }

    pub(super) fn commentary(&mut self, events: &UnboundedSender<Value>) {
        if !progress_text_is_fragment(&self.pending) {
            let text = std::mem::take(&mut self.pending);
            let _ = events.send(self.with_pace(json!({"type":"text_delta","text":text,"accumulated_text":text})));
        }
    }

    pub(super) fn final_answer(&mut self, text: &str, events: &UnboundedSender<Value>) {
        self.pending.clear();
        if self.enabled { let _ = events.send(self.with_pace(json!({"type":"final_answer","text":text}))); }
    }
}






pub(super) async fn model_step(
    model: &ModelClient, messages: &[ChatMessage], tools: Value,
    events: &UnboundedSender<Value>, enabled: bool,
) -> (Result<ModelReply>, StepPresentation) {
    if !enabled {
        return (model.chat_stream(messages, tools, events).await, StepPresentation::default());
    }
    let (sender, mut receiver) = unbounded_channel();
    let mut presentation = StepPresentation { enabled:true, ..Default::default() };
    let request = model.chat_stream(messages, tools, &sender);
    tokio::pin!(request);
    let result = loop {
        tokio::select! {
            result = &mut request => break result,
            Some(event) = receiver.recv() => presentation.consume(event, events),
        }
    };

    while let Ok(event) = receiver.try_recv() { presentation.consume(event, events); }
    (result, presentation)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn text_is_not_a_final_answer_until_the_agent_accepts_it() {
        let (events, mut received) = unbounded_channel();
        let mut step = StepPresentation { enabled:true, ..Default::default() };
        step.consume(json!({"type":"text_delta","text":"Potential final text"}), &events);
        assert!(received.try_recv().is_err());
        step.final_answer("Accepted final text", &events);
        assert_eq!(received.try_recv().unwrap(), json!({"type":"final_answer","text":"Accepted final text"}));
        assert!(received.try_recv().is_err());
    }
    #[test]
    fn buffered_text_uses_one_current_snapshot_and_retries_clear_it() {
        let (events, mut received) = unbounded_channel();
        let mut step = StepPresentation { enabled:true, ..Default::default() };
        step.consume(json!({"type":"text_delta","text":"first","accumulated_text":"first"}), &events);
        step.consume(json!({"type":"text_delta","text":" second","accumulated_text":"first second"}), &events);
        assert_eq!(step.pending, "first second");
        step.consume(json!({"type":"stream_retry","attempt":1,"max_attempts":2}), &events);
        assert!(step.pending.is_empty());
        assert_eq!(received.try_recv().unwrap()["type"], "stream_retry");
        step.consume(json!({"type":"text_delta","text":"replacement"}), &events);
        step.commentary(&events);
        assert_eq!(received.try_recv().unwrap()["text"], "replacement");
        assert!(received.try_recv().is_err());
    }
    #[test]
    fn a_single_letter_is_held_across_tool_intent_until_the_dialogue_arrives() {
        let (events, mut received) = unbounded_channel();
        let mut step = StepPresentation { enabled: true, ..Default::default() };
        step.consume(json!({"type":"text_delta","text":"I","accumulated_text":"I"}), &events);
        step.consume(json!({"type":"status","tool_kind":"read_file"}), &events);
        assert_eq!(received.try_recv().unwrap()["type"], "status");
        assert!(received.try_recv().is_err());
        step.consume(json!({"type":"text_delta","text":" will check the fixture.","accumulated_text":"I will check the fixture."}), &events);
        assert_eq!(received.try_recv().unwrap()["text"], "I will check the fixture.");
        step.commentary(&events);
        assert!(received.try_recv().is_err());
    }
    #[test]
    fn tool_intent_releases_commentary_before_the_preview_without_finalizing() {
        let (events, mut received) = unbounded_channel();
        let mut step = StepPresentation { enabled:true, ..Default::default() };
        step.consume(json!({"type":"text_delta","text":"I will inspect the fixture."}), &events);
        step.consume(json!({"type":"status","label":"Preparing read file","tool_kind":"read_file"}), &events);
        assert_eq!(received.try_recv().unwrap()["type"], "text_delta");
        assert_eq!(received.try_recv().unwrap()["type"], "status");
        step.commentary(&events);
        assert!(received.try_recv().is_err());
    }
}

#[cfg(test)]
mod pacing_tests {
    use super::*;
    #[test]
    fn buffered_commentary_and_final_carry_actual_generation_pace() {
        let (events,mut received)=unbounded_channel();
        let mut step=StepPresentation{enabled:true,..Default::default()};
        let start=Instant::now();
        step.consume_at(json!({"type":"reasoning_delta","text":"Synthetic private reasoning"}),&events,start);
        step.consume_at(json!({"type":"text_delta","text":"First ","accumulated_text":"First "}),&events,start+std::time::Duration::from_secs(10));
        step.consume_at(json!({"type":"text_delta","text":"second third fourth fifth", "accumulated_text":"First second third fourth fifth"}),&events,start+std::time::Duration::from_millis(10400));
        step.commentary(&events);
        let commentary=std::iter::from_fn(||received.try_recv().ok()).find(|event|event["type"]=="text_delta").unwrap();
        assert_eq!(commentary["word_interval_ms"],100.0);
        step.final_answer("First second third fourth fifth",&events);
        assert_eq!(received.try_recv().unwrap()["word_interval_ms"],100.0);
        step.consume_at(json!({"type":"stream_retry"}),&events,start+std::time::Duration::from_secs(20));
        let _=received.try_recv();
        step.final_answer("One-shot replacement",&events);
        assert!(received.try_recv().unwrap().get("word_interval_ms").is_none());
    }
}
