mod reasoning;
mod private_text;
mod token_count;
use anyhow::{Result, anyhow};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use jynx_protocol::ChatMessage;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fmt;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, AtomicBool, Ordering},
};
use tokio::{
    sync::mpsc::UnboundedSender,
    time::{Duration, sleep},
};

const MODEL_IDLE_TIMEOUT: Duration = Duration::from_secs(1800);
const STREAM_RECOVERY_ATTEMPTS: usize = 2;

#[derive(Debug)]
struct StreamBodyError(String);

impl fmt::Display for StreamBodyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "model response stream was interrupted: {}",
            self.0
        )
    }
}

impl std::error::Error for StreamBodyError {}

#[derive(Debug)]
struct EmptyResponseError;

impl fmt::Display for EmptyResponseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("The model returned no visible text or tool calls after repeated attempts. Previously executed tool calls are preserved; you can continue this conversation.")
    }
}
impl std::error::Error for EmptyResponseError {}


#[derive(Debug, Clone)]
pub struct ModelClient {
    temperature: Option<f64>,
    max_output_tokens: Option<u32>,
    endpoint: String,
    model: String,
    api_key: Option<String>,
    local_compat: bool,
    reasoning_effort: String,
    reasoning_catalog: Arc<Mutex<Option<Value>>>,
    local_reasoning_caps: Arc<Mutex<Option<Value>>>,
    client: reqwest::Client,
    token_count_route_missing: Arc<Mutex<bool>>,
    tokenizer_routes_missing: Arc<AtomicBool>,
    stream_usage_supported: Arc<AtomicBool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelReply {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
    #[serde(default)]
    pub input_tokens: Option<usize>,
    #[serde(default)]
    pub output_tokens: Option<usize>,
    pub text: String,
    pub tool_calls: Vec<ModelToolCall>,
    #[serde(default)]
    pub finish_reason: Option<String>,
}
impl ModelReply {
    pub fn output_limit_reached(&self) -> bool {
        matches!(self.finish_reason.as_deref(), Some("length" | "max_tokens" | "max_output_tokens"))
    }
}

pub fn progress_text_is_fragment(text: &str) -> bool {
    let text = text.trim();
    text.is_empty()
        || text.chars().all(|ch| ch.is_ascii_punctuation() || ch.is_whitespace())
        || (text.len() == 1 && text.as_bytes()[0].is_ascii_alphabetic())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

#[derive(Default)]
struct ToolAccumulator {
    id: String,
    name: String,
    arguments: String,
}

static EMULATED_TOOL_CALL_ID: AtomicU64 = AtomicU64::new(1);

impl ModelClient {
    pub fn with_generation_settings(mut self, temperature: Option<f64>, max_output_tokens: Option<u32>) -> Self {
        self.temperature = temperature.filter(|value| value.is_finite()).map(|value| value.clamp(0.0,1.5));
        self.max_output_tokens = max_output_tokens.map(|value|value.clamp(1,1_048_576));
        self
    }
    pub fn new(
        endpoint: impl Into<String>,
        model: impl Into<String>,
        api_key: Option<String>,
        local_compat: bool,
        reasoning_effort: impl Into<String>,
    ) -> Self {
        Self {
            temperature: None,
            max_output_tokens: None,
            endpoint: endpoint.into(),
            model: model.into(),
            api_key,
            local_compat,
            reasoning_effort: reasoning_effort.into(),
            reasoning_catalog: Arc::new(Mutex::new(None)),
            local_reasoning_caps: Arc::new(Mutex::new(None)),
            token_count_route_missing: Arc::new(Mutex::new(false)),
            tokenizer_routes_missing: Arc::new(AtomicBool::new(false)),
            stream_usage_supported: Arc::new(AtomicBool::new(true)),
            client: reqwest::Client::builder()
                .connect_timeout(std::time::Duration::from_secs(20))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
        }
    }

    fn payload_with_reasoning(&self,mut payload:Value)->Value{
        if self.local_compat{payload["reasoning_format"]=json!("deepseek");}
        let effort=self.reasoning_effort.trim();
        if effort==reasoning::ON||effort==reasoning::OFF{payload["chat_template_kwargs"]["enable_thinking"]=json!(effort==reasoning::ON);}
        else if !effort.is_empty(){if let Some(value)=reasoning::native_value(effort){payload["reasoning_effort"]=value;}}
        payload
    }

    async fn configure_local_reasoning(&self,payload:&mut Value,events:Option<&UnboundedSender<Value>>){
        let cached=self.local_reasoning_caps.lock().ok().and_then(|value|value.clone());
        let props=if let Some(value)=cached{value}else{
            let endpoint=self.endpoint.trim_end_matches('/');
            let root=endpoint.strip_suffix("/v1").unwrap_or(endpoint);
            let mut request=self.client.get(format!("{root}/props")).timeout(Duration::from_secs(2));
            if let Some(key)=self.api_key.as_ref().filter(|key|!key.trim().is_empty()){request=request.bearer_auth(key);}
            let props=match request.send().await{Ok(response)if response.status().is_success()=>response.json::<Value>().await.unwrap_or(Value::Null),_=>Value::Null};
            if let Ok(mut cache)=self.local_reasoning_caps.lock(){*cache=Some(props.clone());}
            props
        };
        if let Some(options)=reasoning::capability_options(&props){
            let levels=options.iter().map(|option|option.value.clone()).collect::<Vec<_>>();
            let selected=reasoning::select(&self.reasoning_effort,&levels);
            Self::apply_local_effort(payload,selected.as_deref());
            let selected=selected.unwrap_or_else(||if levels.is_empty(){reasoning::NONE}else{"default"}.into());
            if let Some(events)=events{let _=events.send(json!({"type":"reasoning_capabilities","efforts":levels,"options":options,"selected":selected}));}
        }
    }

    fn apply_local_effort(payload:&mut Value,effort:Option<&str>){
        payload.as_object_mut().unwrap().remove("reasoning_effort");
        if let Some(effort)=effort{
            if effort==reasoning::ON||effort==reasoning::OFF{payload["chat_template_kwargs"]["enable_thinking"]=json!(effort==reasoning::ON);}
            else if let Some(value)=reasoning::native_value(effort){payload["reasoning_effort"]=value;}
        }
    }

    fn is_max_reasoning(&self)->bool{self.reasoning_effort.trim()=="max"}

    fn apply_max_capabilities(&self,payload:&mut Value,model:&Value)->Result<()>{
        let selected=if let Some(levels)=reasoning::capabilities(model){
            let enabled=levels.into_iter().filter(|value|value!="none"&&value!=reasoning::OFF).collect::<Vec<_>>();
            reasoning::select("max",&enabled).ok_or_else(||anyhow!("This model does not advertise an enabled reasoning level for Max. Select a reasoning model."))?
        }else{"max".into()};
        let effort=reasoning::native_value(&selected).unwrap_or(json!("max"));
        if self.endpoint.to_ascii_lowercase().contains("openrouter.ai"){
            payload.as_object_mut().unwrap().remove("reasoning_effort");payload["reasoning"]=json!({"effort":effort,"enabled":true});payload["provider"]["require_parameters"]=json!(true);
        }else{payload["reasoning_effort"]=effort;}
        payload.as_object_mut().unwrap().remove("temperature");Ok(())
    }

    async fn configure_max_reasoning(&self, payload: &mut Value) -> Result<()> {
        let cached = self
            .reasoning_catalog
            .lock()
            .ok()
            .and_then(|value| value.clone());
        let catalog = if let Some(catalog) = cached {
            catalog
        } else {
            let mut request = self
                .client
                .get(self.models_url())
                .timeout(Duration::from_secs(5));
            if let Some(key) = self.api_key.as_ref().filter(|key| !key.trim().is_empty()) {
                request = request.bearer_auth(key.trim());
            }
            let catalog = match request.send().await {
                Ok(response) if response.status().is_success() => {
                    response.json::<Value>().await.unwrap_or(Value::Null)
                }
                _ => Value::Null,
            };
            if let Ok(mut cache) = self.reasoning_catalog.lock() {
                *cache = Some(catalog.clone());
            }
            catalog
        };
        let id = payload
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or(&self.model);
        let model = catalog
            .get("data")
            .and_then(Value::as_array)
            .and_then(|models| {
                models
                    .iter()
                    .find(|model| model.get("id").and_then(Value::as_str) == Some(id))
            })
            .unwrap_or(&Value::Null);
        self.apply_max_capabilities(payload, model)
    }

    fn adapt_max_reasoning(payload: &mut Value) -> bool {
        let current = payload.get("reasoning_effort").and_then(Value::as_str);
        let next = match current {
            Some("max") => "xhigh",
            Some("xhigh") => "high",
            _ => return false,
        };
        payload["reasoning_effort"] = json!(next);
        true
    }

    fn reasoning_parameter_rejected(
        status: reqwest::StatusCode,
        body: &str,
        payload: &Value,
    ) -> bool {
        if !matches!(status.as_u16(),400|422|500)
            || (payload.get("reasoning_effort").is_none() && payload.get("reasoning").is_none())
        {
            return false;
        }
        let body = body.to_ascii_lowercase();
        body.contains("reasoning_effort")
            || (body.contains("reasoning effort") && (body.contains("unexpected")||body.contains("unsupported")||body.contains("supported types")))
            || (body.contains("reasoning")
                && (body.contains("unsupported")
                    || body.contains("unknown")
                    || body.contains("invalid")))

    }

    fn retryable_status(status: reqwest::StatusCode) -> bool {
        status == reqwest::StatusCode::REQUEST_TIMEOUT
            || status == reqwest::StatusCode::TOO_MANY_REQUESTS
            || status.as_u16() == 425
            || status.is_server_error()
    }

    fn models_url(&self) -> String {
        let root = self.endpoint.trim_end_matches('/');
        if root.ends_with("/v1") {
            format!("{root}/models")
        } else {
            format!("{root}/v1/models")
        }
    }

    async fn send_payload_with_retry(
        &self,
        payload: &Value,
        events: Option<&UnboundedSender<Value>>,
        failure_prefix: &str,
    ) -> Result<reqwest::Response> {
        let mut last_error = String::new();
        let mut active_payload = self.payload_with_reasoning(payload.clone());

        let max_attempts = 5;
        let mut configured_model = String::new();
        if self.local_compat{self.configure_local_reasoning(&mut active_payload,events).await;}

        for attempt in 1..=max_attempts {
            if attempt > 1 {
                if let Some(events) = events {
                    let _ = events.send(json!({
                        "type":"connection_status",
                        "state":"connecting",
                        "attempt":attempt,
                        "max_attempts":max_attempts,
                        "message":format!("Connecting... {attempt}/{max_attempts}")
                    }));
                }
            }

            let current_model = active_payload
                .get("model")
                .and_then(Value::as_str)
                .unwrap_or(&self.model)
                .to_string();
            if self.is_max_reasoning() && !self.local_compat && configured_model != current_model {
                self.configure_max_reasoning(&mut active_payload).await?;
                configured_model = current_model;
            }

            let mut request = self
                .client
                .post(self.request_url())
                .header("content-type", "application/json");
            if let Some(key) = self
                .api_key
                .as_ref()
                .filter(|value| !value.trim().is_empty())
            {
                request = request.bearer_auth(key.trim());
            }



            let sent =
                tokio::time::timeout(MODEL_IDLE_TIMEOUT, request.json(&active_payload).send())
                    .await;
            match sent {
                Err(_) => {
                    last_error = format!(
                        "{failure_prefix}: no response from the model for {} minutes",
                        MODEL_IDLE_TIMEOUT.as_secs() / 60
                    );
                    if attempt == max_attempts {
                        return Err(anyhow!(
                            "connection failed after {max_attempts} attempts: {last_error}"
                        ));
                    }
                }
                Ok(Ok(response)) if response.status().is_success() => {
                    if let Some(events) = events {
                        let _ = events.send(json!({
                            "type":"connection_status",
                            "state":"connected",
                            "attempt":attempt,
                            "max_attempts":max_attempts,
                            "message":"Connected"
                        }));
                    }
                    return Ok(response);
                }
                Ok(Ok(response)) => {
                    let status = response.status();
                    let body = response.text().await.unwrap_or_default();
                    if Self::reasoning_parameter_rejected(status,&body,&active_payload){
                        let levels=reasoning::error_levels(&body);
                        if !levels.is_empty(){
                            let selected=reasoning::select(&self.reasoning_effort,&levels).unwrap_or_else(||"default".into());
                            let value=reasoning::native_value(&selected);
                            let current=active_payload.get("reasoning_effort").or_else(||active_payload.pointer("/reasoning/effort"));
                            if current!=value.as_ref(){
                                if let Some(value)=value{
                                    if active_payload.get("reasoning").is_some(){active_payload["reasoning"]["effort"]=value;}else{active_payload["reasoning_effort"]=value;}
                                }else{
                                    active_payload.as_object_mut().unwrap().remove("reasoning_effort");
                                    if let Some(reasoning)=active_payload.get_mut("reasoning").and_then(Value::as_object_mut){reasoning.remove("effort");}
                                }
                                let props=json!({"reasoning_effort_levels":levels.iter().filter_map(|level|reasoning::native_value(level)).collect::<Vec<_>>()});
                                let options=reasoning::capability_options(&props).unwrap_or_default();
                                if self.local_compat{if let Ok(mut cache)=self.local_reasoning_caps.lock(){*cache=Some(props);}}
                                if let Some(events)=events{let _=events.send(json!({"type":"reasoning_capabilities","efforts":levels,"options":options,"selected":selected}));}
                                continue;
                            }
                        }
                    }
                    if self.is_max_reasoning()
                        && Self::reasoning_parameter_rejected(status, &body, &active_payload)
                    {
                        if !self.local_compat && Self::adapt_max_reasoning(&mut active_payload) {
                            if let Some(events) = events {
                                let _ = events.send(json!({"type":"status","label":"Selecting highest supported reasoning"}));
                            }
                            continue;
                        }
                        return Err(anyhow!(
                            "Max reasoning is not supported by this model/provider: {body}. Select a reasoning-capable model."
                        ));
                    }
                    if !self.is_max_reasoning()&&Self::reasoning_parameter_rejected(status,&body,&active_payload){
                        return Err(anyhow!("The model/provider rejected its native reasoning setting: {body}. Choose a supported effort or Default; Jynx will not replace reasoning with prompt instructions."));
                    }
                    last_error = format!("{failure_prefix} ({}): {}", status, body);
                    let retryable = Self::retryable_status(status);

                    if retryable && attempt == 1 {
                        if let Some(events) = events {
                            let _ = events.send(json!({
                                "type":"connection_status",
                                "state":"connecting",
                                "attempt":1,
                                "max_attempts":max_attempts,
                                "message":format!("Connecting... 1/{max_attempts}")
                            }));
                        }
                    }

                    if !retryable || attempt == max_attempts {
                        if let Some(events) = events {
                            let _ = events.send(json!({
                                "type":"connection_status", "state":"failed", "attempt":attempt,
                                "max_attempts":max_attempts, "message":last_error.clone()
                            }));
                        }
                        if attempt == max_attempts && retryable {
                            return Err(anyhow!(
                                "connection failed after {max_attempts} attempts: {last_error}"
                            ));
                        }
                        return Err(anyhow!("{}", last_error));
                    }
                }
                Ok(Err(error)) => {
                    last_error = format!("{failure_prefix}: {error}");
                    let retryable = error.is_connect() || error.is_timeout() || error.is_request();
                    if retryable && attempt == 1 {
                        if let Some(events) = events {
                            let _ = events.send(json!({
                                "type":"connection_status",
                                "state":"connecting",
                                "attempt":1,
                                "max_attempts":max_attempts,
                                "message":format!("Connecting... 1/{max_attempts}")
                            }));
                        }
                    }
                    if !retryable || attempt == max_attempts {
                        if let Some(events) = events {
                            let _ = events.send(json!({
                                "type":"connection_status",
                                "state":"failed",
                                "attempt":attempt,
                                "max_attempts":max_attempts,
                                "message":if last_error.trim().is_empty() { format!("Unable to connect after {attempt}/{max_attempts} attempts") } else { last_error.clone() }
                            }));
                        }
                        if attempt == max_attempts && retryable {
                            return Err(anyhow!(
                                "connection failed after {max_attempts} attempts: {last_error}"
                            ));
                        }
                        return Err(anyhow!("{}", last_error));
                    }
                }
            }
            sleep(Duration::from_millis(
                (attempt as u64) * 450,
            ))
            .await;
        }
        Err(anyhow!(
            "{}",
            if last_error.is_empty() {
                "connection failed".to_string()
            } else {
                last_error
            }
        ))
    }

    async fn send_json_with_body_retry(
        &self,
        payload: &Value,
        events: Option<&UnboundedSender<Value>>,
        failure_prefix: &str,
    ) -> Result<Value> {
        for attempt in 0..=STREAM_RECOVERY_ATTEMPTS {
            let response = self
                .send_payload_with_retry(payload, events, failure_prefix)
                .await?;
            match tokio::time::timeout(MODEL_IDLE_TIMEOUT, response.json::<Value>()).await {
                Ok(Ok(value)) => return Ok(value),
                Ok(Err(error))
                    if (error.is_body() || error.is_decode())
                        && attempt < STREAM_RECOVERY_ATTEMPTS => {}
                Ok(Err(error)) => {
                    return Err(anyhow!(
                        "{failure_prefix}: response body could not be read: {error:#}"
                    ));
                }
                Err(_) if attempt < STREAM_RECOVERY_ATTEMPTS => {}
                Err(_) => {
                    return Err(anyhow!(
                        "{failure_prefix}: no response data for {} minutes",
                        MODEL_IDLE_TIMEOUT.as_secs() / 60
                    ));
                }
            }
            if let Some(events) = events {
                let _ = events
                    .send(json!({"type":"status","label":"Retrying interrupted model response"}));
            }
            sleep(Duration::from_millis(500 * (attempt as u64 + 1))).await;
        }
        unreachable!("response body recovery loop always returns")
    }
    fn request_url(&self) -> String {
        let root = self.endpoint.trim_end_matches('/');
        if root.ends_with("/chat/completions") {
            root.to_string()
        } else if root.ends_with("/v1") {
            format!("{root}/chat/completions")
        } else {
            format!("{root}/v1/chat/completions")
        }
    }

    fn normalized_messages(&self, messages: &[ChatMessage]) -> Vec<ChatMessage> {




        if !self.local_compat {
            return messages.to_vec();
        }
        let mut system_parts = Vec::new();
        let mut conversation = Vec::with_capacity(messages.len());
        for message in messages {
            if message.role.eq_ignore_ascii_case("system") {
                let content = message.content.trim();
                if !content.is_empty() {
                    system_parts.push(content.to_string());
                }
            } else {
                conversation.push(message.clone());
            }
        }
        if system_parts.is_empty() {
            return conversation;
        }
        let mut normalized = Vec::with_capacity(conversation.len() + 1);
        normalized.push(ChatMessage {
            phase: None, role: "system".into(),
            content: system_parts.join("\n\n"),
            images: Vec::new(),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        });
        normalized.extend(conversation);
        normalized
    }

    fn text_from_content(content: &Value) -> String {
        match content {
            Value::String(value) => value.clone(),
            Value::Array(parts) => parts
                .iter()
                .map(Self::text_from_content)
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join(""),
            Value::Object(object) => {
                if private_text::private_part(content){return String::new();}
                if let Some(text) = object.get("text").and_then(Value::as_str) {
                    return text.to_string();
                }
                if let Some(content) = object.get("content") {
                    return Self::text_from_content(content);
                }
                if let Some(value) = object.get("value").and_then(Value::as_str) {
                    return value.to_string();
                }
                String::new()
            }
            _ => String::new(),
        }
    }

    fn image_data_url(source: &str) -> Result<String> {
        let source = source.trim();
        if source.starts_with("data:image/")
            || source.starts_with("https://")
            || source.starts_with("http://")
        {
            return Ok(source.to_string());
        }
        let bytes = std::fs::read(source)
            .map_err(|error| anyhow!("could not read attached image {}: {error}", source))?;
        let extension = std::path::Path::new(source)
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let mime = match extension.as_str() {
            "jpg" | "jpeg" => "image/jpeg",
            "webp" => "image/webp",
            "gif" => "image/gif",
            "bmp" => "image/bmp",
            _ => "image/png",
        };
        Ok(format!(
            "data:{mime};base64,{}",
            BASE64_STANDARD.encode(bytes)
        ))
    }

    fn request_messages(&self, messages: &[ChatMessage]) -> Result<Vec<Value>> {
        self.normalized_messages(messages)
            .into_iter()
            .map(|message| -> Result<Value> {
                let images = message.images.clone();
                let assistant = message.role == "assistant";
                let mut value = serde_json::to_value(message).unwrap_or(Value::Null);
                if !assistant { value.as_object_mut().unwrap().remove("phase"); }
                if images.is_empty() {
                    return Ok(value);
                }
                let mut parts = Vec::with_capacity(images.len() + 1);
                if let Some(text) = value
                    .get("content")
                    .and_then(Value::as_str)
                    .filter(|text| !text.is_empty())
                {
                    parts.push(json!({"type":"text","text":text}));
                }
                for source in images {
                    let url = Self::image_data_url(&source)?;
                    parts.push(json!({"type":"image_url","image_url":{"url":url}}));
                }
                if let Some(object) = value.as_object_mut() {
                    object.insert("content".into(), Value::Array(parts));
                }
                Ok(value)
            })
            .collect()
    }

    fn parse_tool_calls(container: &Value) -> Vec<ModelToolCall> {
        container["tool_calls"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|call| {
                let function = &call["function"];
                let name = function["name"].as_str()?.to_string();
                let arguments = match &function["arguments"] {
                    Value::String(raw) => {
                        serde_json::from_str(raw).unwrap_or(Value::Object(Default::default()))
                    }
                    Value::Object(_) | Value::Array(_) => function["arguments"].clone(),
                    _ => Value::Object(Default::default()),
                };
                Some(ModelToolCall {
                    id: call["id"].as_str().unwrap_or_default().to_string(),
                    name,
                    arguments,
                })
            })
            .collect()
    }

    fn parse_emulated_tool_calls(text: &str) -> ModelReply {
        fn parse_tagged(text: &str, open: &str, close: &str) -> (String, Vec<ModelToolCall>) {
            let mut visible = String::new();
            let mut calls = Vec::new();
            let mut cursor = 0usize;
            while let Some(relative_start) = text[cursor..].find(open) {
                let start = cursor + relative_start;
                visible.push_str(&text[cursor..start]);
                let payload_start = start + open.len();
                let Some(relative_end) = text[payload_start..].find(close) else {


                    cursor = text.len();
                    break;
                };
                let end = payload_start + relative_end;
                let raw = text[payload_start..end].trim();
                match serde_json::from_str::<Value>(raw) {
                    Ok(value) => {
                        let name = value
                            .get("name")
                            .or_else(|| value.get("tool"))
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .trim();
                        if !name.is_empty() {
                            let argument_value =
                                value.get("arguments").or_else(|| value.get("args"));
                            let arguments = match argument_value {
                                Some(Value::String(raw)) => serde_json::from_str(raw)
                                    .unwrap_or(Value::Object(Default::default())),
                                Some(Value::Object(_)) | Some(Value::Array(_)) => {
                                    argument_value.cloned().unwrap_or_default()
                                }
                                _ => Value::Object(Default::default()),
                            };
                            let id = EMULATED_TOOL_CALL_ID.fetch_add(1, Ordering::Relaxed);
                            calls.push(ModelToolCall {
                                id: format!("jynx-emulated-{id}"),
                                name: name.to_string(),
                                arguments,
                            });
                        }
                    }



                    Err(_) => {}
                }
                cursor = end + close.len();
            }
            if cursor < text.len() {
                visible.push_str(&text[cursor..]);
            }
            (visible, calls)
        }

        fn parse_function_xml(text: &str) -> (String, Vec<ModelToolCall>) {
            let mut visible = String::new();
            let mut calls = Vec::new();
            let mut cursor = 0usize;
            while let Some(relative_start) = text[cursor..].find("<tool_call>") {
                let start = cursor + relative_start;
                visible.push_str(&text[cursor..start]);
                let payload_start = start + "<tool_call>".len();
                let Some(relative_end) = text[payload_start..].find("</tool_call>") else {


                    cursor = text.len();
                    break;
                };
                let end = payload_start + relative_end;
                let payload = &text[payload_start..end];
                let Some(function_start) = payload.find("<function=") else {
                    visible.push_str(&text[start..end + "</tool_call>".len()]);
                    cursor = end + "</tool_call>".len();
                    continue;
                };
                let name_start = function_start + "<function=".len();
                let Some(name_end_offset) = payload[name_start..].find('>') else {
                    cursor = end + "</tool_call>".len();
                    continue;
                };
                let name_end = name_start + name_end_offset;
                let name = payload[name_start..name_end]
                    .trim()
                    .trim_matches(['\'', '"']);
                let function_body_start = name_end + 1;
                let function_body_end = payload[function_body_start..]
                    .find("</function>")
                    .map(|offset| function_body_start + offset)
                    .unwrap_or(payload.len());
                let body = &payload[function_body_start..function_body_end];
                let mut arguments = serde_json::Map::new();
                let mut parameter_cursor = 0usize;
                while let Some(parameter_offset) = body[parameter_cursor..].find("<parameter=") {
                    let parameter_start = parameter_cursor + parameter_offset + "<parameter=".len();
                    let Some(parameter_name_end_offset) = body[parameter_start..].find('>') else {
                        break;
                    };
                    let parameter_name_end = parameter_start + parameter_name_end_offset;
                    let parameter_name = body[parameter_start..parameter_name_end]
                        .trim()
                        .trim_matches(['\'', '"']);
                    let value_start = parameter_name_end + 1;
                    let Some(value_end_offset) = body[value_start..].find("</parameter>") else {
                        break;
                    };
                    let value_end = value_start + value_end_offset;
                    let value = body[value_start..value_end].trim();
                    if !parameter_name.is_empty() && !value.is_empty() {
                        arguments
                            .insert(parameter_name.to_string(), Value::String(value.to_string()));
                    }
                    parameter_cursor = value_end + "</parameter>".len();
                }


                if !name.is_empty() && !arguments.is_empty() {
                    let id = EMULATED_TOOL_CALL_ID.fetch_add(1, Ordering::Relaxed);
                    calls.push(ModelToolCall {
                        id: format!("jynx-emulated-{id}"),
                        name: name.to_string(),
                        arguments: Value::Object(arguments),
                    });
                }
                cursor = end + "</tool_call>".len();
            }
            if cursor < text.len() {
                visible.push_str(&text[cursor..]);
            }
            (visible, calls)
        }

        let (first_visible, mut calls) =
            parse_tagged(text, "<jynx_tool_call>", "</jynx_tool_call>");
        let (xml_visible, mut xml_calls) = parse_function_xml(&first_visible);
        calls.append(&mut xml_calls);
        let (visible, mut legacy_calls) = parse_tagged(&xml_visible, "<tool_call>", "</tool_call>");
        calls.append(&mut legacy_calls);
        ModelReply {
            input_tokens: None, output_tokens: None, phase: None, finish_reason: None,
            text: visible.trim().to_string(),
            tool_calls: calls,
        }
    }

    fn normalize_tool_markup(mut reply: ModelReply) -> ModelReply {
        let public_text = Self::strip_private_thinking(&reply.text, false);
        let parsed = Self::parse_emulated_tool_calls(&public_text);
        reply.text = Self::clean_dialogue_text(&parsed.text).trim().to_string();
        for call in parsed.tool_calls {
            let duplicate = reply
                .tool_calls
                .iter()
                .any(|existing| existing.name == call.name && existing.arguments == call.arguments);
            if !duplicate {
                reply.tool_calls.push(call);
            }
        }
        reply
    }

    fn clean_dialogue_text(text: &str) -> String {
        let mut fence: Option<&str> = None;
        text.split('\n').filter(|line| {
            let trimmed = line.trim();
            if let Some(marker) = ["```", "~~~"].into_iter().find(|marker| trimmed.starts_with(marker)) {
                if fence == Some(marker) { fence = None; } else if fence.is_none() { fence = Some(marker); }
                return true;
            }


            fence.is_some() || trimmed.is_empty() || !trimmed.chars().all(|ch| matches!(ch, '.' | '…') || ch.is_whitespace())
        }).collect::<Vec<_>>().join("\n")
    }

    fn strip_private_thinking(text:&str,streaming:bool)->String{private_text::public_text(text,streaming)}

    fn visible_stream_text(raw: &str) -> String {
        let public_text = Self::strip_private_thinking(raw, true);
        let mut visible = Self::parse_emulated_tool_calls(&public_text).text;
        for marker in ["<jynx_tool_call", "<tool_call"] {
            if let Some(index) = visible.find(marker) {
                visible.truncate(index);
            }



            for prefix_len in (1..marker.len()).rev() {
                if visible.ends_with(&marker[..prefix_len]) {
                    visible.truncate(visible.len() - prefix_len);
                    break;
                }
            }
        }
        Self::clean_dialogue_text(&visible).trim().to_string()
    }

    fn input_usage(value: &Value) -> Option<usize> {
        value["usage"]["prompt_tokens"].as_u64().or_else(||value["usage"]["input_tokens"].as_u64()).or_else(||value["timings"]["prompt_n"].as_u64()).and_then(|tokens|usize::try_from(tokens).ok())
    }
    fn output_usage(value: &Value) -> Option<usize> {
        value["usage"]["completion_tokens"].as_u64().or_else(||value["usage"]["output_tokens"].as_u64()).or_else(||value["timings"]["predicted_n"].as_u64()).and_then(|tokens|usize::try_from(tokens).ok())
    }

    fn message_phase(value: &Value) -> Option<String> {
        let choice = &value["choices"][0];
        [&choice["delta"], &choice["message"], choice, value].into_iter()
            .find_map(|item| item["phase"].as_str()
                .and_then(|phase| match phase {
                    "commentary" | "partial_answer" | "final_answer" => Some(phase.to_owned()),
                    _ => None,
                }))
    }

    fn parse_message(value: &Value) -> ModelReply {
        let choice = value
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first());
        let finish_reason = choice.and_then(|choice| choice.get("finish_reason")).or_else(||value.get("stop_reason")).and_then(Value::as_str).map(str::to_owned);
        let null = Value::Null;
        let message = choice
            .and_then(|choice| choice.get("message"))
            .or_else(|| value.get("message"))
            .unwrap_or(&null);
        if private_text::private_part(value)||choice.is_some_and(private_text::private_part)||private_text::private_part(message) {
            return Self::normalize_tool_markup(ModelReply { input_tokens: Self::input_usage(value), output_tokens: Self::output_usage(value), phase: Self::message_phase(value), finish_reason: finish_reason.clone(), text: String::new(), tool_calls: Self::parse_tool_calls(message) });
        }
        let mut text = Self::text_from_content(message);
        if text.is_empty() {
            if let Some(choice) = choice {
                text = Self::text_from_content(&choice["text"]);
            }
        }



        if text.is_empty() {
            for key in ["output_text", "response", "text", "content"] {
                text = Self::text_from_content(&value[key]);
                if !text.is_empty() {
                    break;
                }
            }
        }
        if text.is_empty() {
            if let Some(output) = value.get("output").and_then(Value::as_array) {
                text = output
                    .iter()
                    .map(Self::text_from_content)
                    .filter(|part| !part.is_empty())
                    .collect::<Vec<_>>()
                    .join("");
            }
        }
        Self::normalize_tool_markup(ModelReply {
            input_tokens: Self::input_usage(value), output_tokens: Self::output_usage(value), phase: Self::message_phase(value), finish_reason: finish_reason.clone(),
            text,
            tool_calls: Self::parse_tool_calls(message),
        })
    }

    fn append_stream_tool_calls(delta: &Value, calls: &mut BTreeMap<usize, ToolAccumulator>) {
        if let Some(tool_calls) = delta["tool_calls"].as_array() {
            for call in tool_calls {
                let index = call["index"].as_u64().unwrap_or(0) as usize;
                let entry = calls.entry(index).or_default();
                if let Some(id) = call["id"].as_str() {
                    if entry.id.is_empty() && !id.is_empty() {
                        entry.id = id.to_string();
                    }
                }
                let function = &call["function"];
                if let Some(name) = function["name"].as_str() {
                    if entry.name.is_empty() && !name.is_empty() {
                        entry.name = name.to_string();
                    }
                }
                match &function["arguments"] {
                    Value::String(arguments) => entry.arguments.push_str(arguments),
                    Value::Object(_) | Value::Array(_) if entry.arguments.is_empty() => {
                        entry.arguments = function["arguments"].to_string();
                    }
                    _ => {}
                }
            }
        }
    }



    fn preview_tool_arguments(raw: &str) -> Option<Value> {
        if let Ok(value) = serde_json::from_str::<Value>(raw) { return Some(value); }
        let mut rest = raw.trim_start().strip_prefix('{')?;
        loop {
            rest = rest.trim_start();
            let mut keys = serde_json::Deserializer::from_str(rest).into_iter::<String>();
            let key = keys.next()?.ok()?;
            rest = rest[keys.byte_offset()..].trim_start().strip_prefix(':')?.trim_start();
            let mut values = serde_json::Deserializer::from_str(rest).into_iter::<Value>();
            let value = values.next()?.ok()?;
            if matches!(key.as_str(), "path" | "url" | "query" | "command") && value.is_string() {
                let mut fields = serde_json::Map::new();
                fields.insert(key, value);
                return Some(Value::Object(fields));
            }
            rest = rest[values.byte_offset()..].trim_start().strip_prefix(',')?;
        }
    }

    #[cfg(test)]
    fn process_stream_value(value:&Value,text:&mut String,calls:&mut BTreeMap<usize,ToolAccumulator>,events:&UnboundedSender<Value>){
        Self::process_stream_value_with_channel(value,text,calls,&mut None,events);
    }

    fn process_stream_value_with_channel(
        value: &Value,
        text: &mut String,
        calls: &mut BTreeMap<usize, ToolAccumulator>,
        channel:&mut Option<String>,
        events: &UnboundedSender<Value>,
    ) {
        let choice = value
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first());
        let null = Value::Null;
        let delta = choice
            .and_then(|choice| choice.get("delta"))
            .unwrap_or(&null);
        if let Some(value)=delta.get("channel").or_else(||choice.and_then(|choice|choice.get("channel"))).and_then(Value::as_str){*channel=Some(value.to_owned());}
        if private_text::private_part(delta)&&delta.get("channel").is_none(){*channel=Some("reasoning".into());}
        else if matches!(delta["type"].as_str(),Some("text"|"output_text"|"text_delta")){*channel=Some("final".into());}
        let in_private_channel=channel.as_deref().is_some_and(private_text::private_channel);
        let content=if in_private_channel{
            let reasoning=Self::text_from_content(&delta["content"]);
            if !reasoning.is_empty(){let _=events.send(json!({"type":"reasoning_delta","text":reasoning}));}
            String::new()
        }else{Self::text_from_content(delta)};
        if !content.is_empty() {
            let had_tool_markup = text.contains("<tool_call") || text.contains("<jynx_tool_call");
            let previous_visible = Self::visible_stream_text(text);
            text.push_str(&content);
            let visible = Self::visible_stream_text(text);
            let visible_delta = visible
                .strip_prefix(&previous_visible)
                .unwrap_or(&visible)
                .to_string();



            if !visible_delta.is_empty() || visible != previous_visible {
                let _ = events.send(
                    json!({"type":"text_delta","text":visible_delta,"accumulated_text":visible}),
                );
            }
            if !had_tool_markup
                && (text.contains("<tool_call") || text.contains("<jynx_tool_call"))
            {
                let _ = events.send(json!({"type":"status","label":"Preparing tool call"}));
            }
        }

        let reasoning = Self::text_from_content(&delta["reasoning_content"]);
        let reasoning = if reasoning.is_empty() {
            Self::text_from_content(&delta["reasoning"])
        } else {
            reasoning
        };
        if !reasoning.is_empty() {
            let _ = events.send(json!({"type":"reasoning_delta","text":reasoning}));
        }
        let known_calls: Vec<usize> = calls
            .iter()
            .filter(|(_, call)| !call.name.is_empty())
            .map(|(index, _)| *index)
            .collect();
        let previous_previews: std::collections::BTreeMap<usize, Option<Value>> = calls.iter()
            .map(|(index, call)| (*index, Self::preview_tool_arguments(&call.arguments))).collect();
        Self::append_stream_tool_calls(delta, calls);






        for (index, call) in calls.range(0..=0) {
            let arguments = Self::preview_tool_arguments(&call.arguments);



            if matches!(call.name.as_str(), "read_file" | "list_files") && arguments.as_ref()
                .and_then(|args| args["path"].as_str()).is_none_or(|path| path.trim().is_empty()) {
                continue;
            }
            if call.name != "finish_turn" && !call.name.is_empty() && (!known_calls.contains(index) || previous_previews.get(index) != Some(&arguments)) {
                let name = call.name.replace('_', " ");
                let _ = events.send(json!({
                    "type":"status",
                    "label":format!("Preparing {name}"),
                    "tool_kind":&call.name,
                    "tool_arguments":arguments
                }));
            }
        }




        if text.is_empty() && calls.is_empty() {
            if let Some(message) = choice.and_then(|choice| choice.get("message")) {
                let final_text = if in_private_channel&&message.get("channel").is_none(){String::new()}else{Self::text_from_content(message)};
                if !final_text.is_empty() {
                    text.push_str(&final_text);
                    let visible = Self::visible_stream_text(text);
                    if !visible.is_empty() {
                        let _ = events.send(json!({
                            "type":"text_delta",
                            "text":visible,
                            "accumulated_text":visible
                        }));
                    }
                }
                for (index, call) in Self::parse_tool_calls(message).into_iter().enumerate() {
                    calls.insert(
                        index,
                        ToolAccumulator {
                            id: call.id,
                            name: call.name,
                            arguments: call.arguments.to_string(),
                        },
                    );
                }
            }
        }
    }

    fn finish_stream_calls(calls: BTreeMap<usize, ToolAccumulator>) -> Result<Vec<ModelToolCall>> {
        calls
            .into_values()
            .filter(|call| !call.name.is_empty())
            .map(|call| {
                let arguments = if call.arguments.trim().is_empty() {
                    Value::Object(Default::default())
                } else {
                    serde_json::from_str(&call.arguments)
                        .map_err(|error| StreamBodyError(format!("incomplete arguments for {}: {error}", call.name)))?
                };
                Ok(ModelToolCall { id: call.id, name: call.name, arguments })
            })
            .collect()
    }

    async fn chat_once(
        &self,
        messages: &[ChatMessage],
        tools: &Value,
        events: Option<&UnboundedSender<Value>>,
    ) -> Result<ModelReply> {
        let normalized_messages = self.request_messages(messages)?;
        let payload = json!({
            "model": &self.model,
            "messages": normalized_messages,
            "tools": tools,
            "tool_choice": "auto",
            "temperature": 0.2,
            "stream": false
        });
        let value = self
            .send_json_with_body_retry(&payload, events, "model API request failed")
            .await?;
        Ok(Self::parse_message(&value))
    }

    async fn chat_once_text_only(
        &self,
        messages: &[ChatMessage],
        events: Option<&UnboundedSender<Value>>,
    ) -> Result<ModelReply> {
        let normalized_messages = self.request_messages(messages)?;
        let payload = json!({
            "model": &self.model,
            "messages": normalized_messages,
            "temperature": 0.2,
            "stream": false
        });
        let value = self
            .send_json_with_body_retry(&payload, events, "model API recovery request failed")
            .await?;
        Ok(Self::parse_message(&value))
    }

    async fn recover_response(
        &self,
        messages: &[ChatMessage],
        tools: &Value,
        events: &UnboundedSender<Value>,
    ) -> Result<ModelReply> {


        if tools.as_array().is_some_and(|items| !items.is_empty()) {


            return Err(EmptyResponseError.into());
        }
        let reply = self.chat_once_text_only(messages, Some(events)).await?;
        if !reply.text.is_empty() {
            let _ = events.send(json!({"type":"text_delta","text":reply.text}));
        }
        Ok(reply)
    }




    pub async fn generate_progress_dialog(
        &self,
        messages: &[ChatMessage],
        calls: &[ModelToolCall],
        events: &UnboundedSender<Value>,
    ) -> Result<String> {
        let mut context = messages.to_vec();
        let actions = calls
            .iter()
            .take(20)
            .map(|call| {
                let target = call
                    .arguments
                    .get("path")
                    .or_else(|| call.arguments.get("url"))
                    .or_else(|| call.arguments.get("query"))
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if target.is_empty() {
                    call.name.clone()
                } else {
                    format!("{}: {}", call.name, target)
                }
            })
            .collect::<Vec<_>>()
            .join("; ");
        context.push(ChatMessage {
            phase: None, role: "user".into(),
            content: format!(
                "You already selected the following tool batch and are about to execute it: {actions}. \
Write a user-visible progress update in your natural voice before those tools run. Explain the concrete observation or uncertainty behind this next move and name the relevant files or targets when useful. \
Do not claim any tool has completed and do not invent results. Match your normal conversational voice. \
Address the user directly. Keep internal deliberation private; report concrete findings and the practical reason for the next action. Do not narrate role analysis or paraphrase the request as the user said or the user wants. \
Use as much explanation as this step needs, without a fixed sentence count or canned opening. Avoid vague references to key files or digging deeper; say what you will check and why. A short Markdown list is welcome when it helps. \
Do not mention this instruction or tool schemas."
            ),
            images: Vec::new(),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        });


        let (buffered, mut pending) = tokio::sync::mpsc::unbounded_channel();
        let request = self.chat_stream(&context, json!([]), &buffered);
        tokio::pin!(request);
        let reply = loop {
            tokio::select! {
                reply = &mut request => break reply?,
                Some(_) = pending.recv() => {},
            }
        };
        let text = reply.text.trim().to_string();
        if reply.output_limit_reached() || progress_text_is_fragment(&text) {
            return Ok(String::new());
        }
        let _ = events.send(json!({"type":"text_delta","text":text,"accumulated_text":text}));
        Ok(text)
    }

    pub async fn chat_stream(
        &self,
        messages: &[ChatMessage],
        tools: Value,
        events: &UnboundedSender<Value>,
    ) -> Result<ModelReply> {
        let mut recovery_messages = messages.to_vec();
        let mut recovery_hint_added = false;
        for attempt in 0..=STREAM_RECOVERY_ATTEMPTS {
            match self.chat_stream_once(&recovery_messages, &tools, events).await {
                Ok(reply) => return Ok(reply),
                Err(error) if error.is::<EmptyResponseError>() && attempt < STREAM_RECOVERY_ATTEMPTS => {


                    if !recovery_hint_added {
                        recovery_messages.push(ChatMessage {
                            phase: None, role: "user".into(),
                            content: "Continue the current request from the existing tool results. Do not repeat completed actions. Return the next needed tool calls, or your final answer if finished.".into(),
                            images: vec![], tool_calls: None, tool_call_id: None, name: None,
                        });
                        recovery_hint_added = true;
                    }
                    let _ = events.send(json!({"type":"stream_retry","reason":"empty_response","attempt":attempt+1,"max_attempts":STREAM_RECOVERY_ATTEMPTS}));
                    let _ = events.send(json!({"type":"status","label":"Retrying empty model response"}));
                    sleep(Duration::from_millis(500 * (attempt as u64 + 1))).await;
                },
                Err(error)
                    if error.is::<StreamBodyError>() && attempt < STREAM_RECOVERY_ATTEMPTS =>
                {



                    let _ = events.send(json!({
                        "type":"stream_retry",
                        "attempt":attempt + 1,
                        "max_attempts":STREAM_RECOVERY_ATTEMPTS,
                    }));
                    sleep(Duration::from_millis(500 * (attempt as u64 + 1))).await;
                }
                Err(error) => return Err(error),
            }
        }
        unreachable!("stream recovery loop always returns")
    }

    async fn chat_stream_once(
        &self,
        messages: &[ChatMessage],
        tools: &Value,
        events: &UnboundedSender<Value>,
    ) -> Result<ModelReply> {
        let normalized_messages = self.request_messages(messages)?;
        let mut payload = json!({
            "model": &self.model,
            "messages": normalized_messages,
            "temperature": self.temperature.unwrap_or(0.2),
            "stream": true
        });
        if let Some(limit)=self.max_output_tokens { payload["max_tokens"]=json!(limit); }
        if tools.as_array().is_some_and(|items| !items.is_empty()) {
            payload["tools"] = tools.clone();
            payload["tool_choice"] = json!("auto");
            if self.local_compat { payload["parallel_tool_calls"] = json!(true); }
        }
        let mut wants_usage = self.stream_usage_supported.load(Ordering::Relaxed);
        if wants_usage { payload["stream_options"] = json!({"include_usage":true}); }
        let response = match self.send_payload_with_retry(&payload, Some(events), "model API request failed").await {
            Ok(response) => response,
            Err(error) if wants_usage && ["stream_options", "include_usage"].iter().any(|field|error.to_string().contains(field)) => {
                self.stream_usage_supported.store(false, Ordering::Relaxed);
                payload.as_object_mut().unwrap().remove("stream_options");
                wants_usage = false;
                self.send_payload_with_retry(&payload, Some(events), "model API request failed").await?
            }
            Err(error) => return Err(error),
        };

        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !content_type.contains("text/event-stream") && !content_type.contains("stream") {
            let value: Value = response
                .json()
                .await
                .map_err(|error| StreamBodyError(format!("{error:#}")))?;
            let reply = Self::parse_message(&value);
            if reply.output_limit_reached() || !reply.text.trim().is_empty() || !reply.tool_calls.is_empty() {
                if !reply.text.is_empty() {
                    let _ = events.send(json!({"type":"text_delta","text":reply.text}));
                }
                return Ok(reply);
            }

            let _ = events.send(json!({"type":"status","label":"Retrying response"}));
            let fallback = self.chat_once(messages, &tools, Some(events)).await?;
            if !fallback.text.trim().is_empty() || !fallback.tool_calls.is_empty() {
                if !fallback.text.is_empty() {
                    let _ = events.send(json!({"type":"text_delta","text":fallback.text}));
                }
                return Ok(fallback);
            }

            let _ = events.send(json!({"type":"status","label":"Recovering response"}));
            return self.recover_response(messages, tools, events).await;
        }

        let mut stream = response.bytes_stream();


        let mut buffer = Vec::<u8>::new();
        let mut text = String::new();
        let mut calls: BTreeMap<usize, ToolAccumulator> = BTreeMap::new();
        let mut input_tokens = None;
        let mut output_tokens = None;
        let mut stream_channel=None;
        let mut phase = None;
        let mut stream_finished = false;
        let mut finish_reason = None;
        let mut usage_deadline = None;
        'response: loop {
            let wait = usage_deadline.map(|deadline: tokio::time::Instant|deadline.saturating_duration_since(tokio::time::Instant::now())).unwrap_or(MODEL_IDLE_TIMEOUT);
            let next = match tokio::time::timeout(wait, stream.next()).await {
                Ok(next) => next,
                Err(_) if stream_finished => break,
                Err(_) => return Err(StreamBodyError(format!("no response data for {} minutes", MODEL_IDLE_TIMEOUT.as_secs()/60)).into()),
            };
            let Some(chunk) = next else { break };
            let chunk = chunk.map_err(|error| StreamBodyError(format!("{error:#}")))?;
            buffer.extend_from_slice(&chunk);
            while let Some(position) = buffer.iter().position(|byte| *byte == b'\n') {
                let mut line = String::from_utf8(buffer[..position].to_vec())
                    .map_err(|error| StreamBodyError(format!("invalid UTF-8 in model stream: {error}")))?;
                buffer.drain(..=position);
                if line.ends_with('\r') {
                    line.pop();
                }
                let line = line.trim();
                if line.is_empty() || line.starts_with(':') || line.starts_with("event:") {
                    continue;
                }
                let data = line.strip_prefix("data:").map(str::trim).unwrap_or(line);
                if data == "[DONE]" {
                    stream_finished = true;
                    buffer.clear();
                    break 'response;
                }
                if data.is_empty() {
                    continue;
                }
                if let Ok(value) = serde_json::from_str::<Value>(data) {
                    input_tokens = Self::input_usage(&value).or(input_tokens);
                    output_tokens = Self::output_usage(&value).or(output_tokens);
                    phase = Self::message_phase(&value).or(phase);
                    Self::process_stream_value_with_channel(&value, &mut text, &mut calls, &mut stream_channel, events);
                    if stream_finished && input_tokens.is_some() { buffer.clear(); break 'response; }
                    if let Some(reason) = value["choices"][0]["finish_reason"].as_str().filter(|reason| !reason.is_empty()) {
                        finish_reason = Some(reason.to_owned());
                        stream_finished = true;
                        if wants_usage && input_tokens.is_none() {
                            usage_deadline.get_or_insert(tokio::time::Instant::now() + Duration::from_millis(250));
                        } else {
                            buffer.clear();
                            break 'response;
                        }
                    }
                }
            }
        }
        if !buffer.is_empty() {
            let remainder = String::from_utf8(buffer)
                .map_err(|error| StreamBodyError(format!("incomplete UTF-8 in model stream: {error}")))?;
            for line in remainder.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with(':') || line.starts_with("event:") {
                    continue;
                }
                let data = line.strip_prefix("data:").map(str::trim).unwrap_or(line);
                if data == "[DONE]" {
                    stream_finished = true;
                    continue;
                }
                if let Ok(value) = serde_json::from_str::<Value>(data) {
                    input_tokens = Self::input_usage(&value).or(input_tokens);
                    output_tokens = Self::output_usage(&value).or(output_tokens);
                    phase = Self::message_phase(&value).or(phase);
                    Self::process_stream_value_with_channel(&value, &mut text, &mut calls, &mut stream_channel, events);
                    if let Some(reason) = value["choices"][0]["finish_reason"].as_str().filter(|reason| !reason.is_empty()) {
                        finish_reason = Some(reason.to_owned());
                        stream_finished = true;
                    }
                }
            }
        }

        if !stream_finished {
            return Err(StreamBodyError("model stream ended before its completion marker; incomplete tool calls were not executed".into()).into());
        }
        let truncated = matches!(finish_reason.as_deref(), Some("length" | "max_tokens" | "max_output_tokens"));
        let tool_calls = if truncated { Vec::new() } else { Self::finish_stream_calls(calls)? };
        let reply = Self::normalize_tool_markup(ModelReply { input_tokens, output_tokens, phase, finish_reason, text, tool_calls });
        if !reply.output_limit_reached() && reply.text.trim().is_empty() && reply.tool_calls.is_empty() {



            let _ = events.send(json!({"type":"status","label":"Retrying response"}));
            let fallback = self.chat_once(messages, &tools, Some(events)).await?;
            if !fallback.text.trim().is_empty() || !fallback.tool_calls.is_empty() {
                if !fallback.text.is_empty() {
                    let _ = events.send(json!({"type":"text_delta","text":fallback.text}));
                }
                return Ok(fallback);
            }



            let _ = events.send(json!({"type":"status","label":"Recovering response"}));
            return self.recover_response(messages, tools, events).await;
        }
        Ok(reply)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn private_thinking_is_hidden_across_stream_chunks_but_tag_examples_stay_literal() {
        assert_eq!(ModelClient::visible_stream_text("<thi"), "");
        assert_eq!(ModelClient::visible_stream_text("<think>The user said to inspect the project."), "");
        assert_eq!(ModelClient::visible_stream_text("<think>The user said to inspect the project.</think>I found two modules to check."), "I found two modules to check.");
        assert_eq!(ModelClient::visible_stream_text("<analysis>Private planning</analysis>The build failed because a dependency is missing."), "The build failed because a dependency is missing.");
        let example = "The literal tag is `<think>`.\n```xml\n<think>example</think>\n```";
        assert_eq!(ModelClient::strip_private_thinking(example, false), example);
        let reply = ModelClient::parse_message(&json!({"choices":[{"message":{"content":[
            {"type":"reasoning","text":"The user said to change it."},
            {"type":"text","text":"I will update the parser."}
        ]}}]}));
        assert_eq!(reply.text, "I will update the parser.");
        let (events, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        ModelClient::process_stream_value(&json!({"choices":[{"delta":{"channel":"analysis","content":"Private role analysis"}}]}), &mut String::new(), &mut std::collections::BTreeMap::new(), &events);
        assert!(!std::iter::from_fn(||receiver.try_recv().ok()).any(|event|event["type"]=="text_delta"));
        assert!(ModelClient::parse_message(&json!({"choices":[{"message":{"channel":"analysis","content":"Private role analysis"}}]})).text.is_empty());
        let private_call = ModelClient::normalize_tool_markup(ModelReply { input_tokens: None, output_tokens: None, phase: None, finish_reason: None, text:"<think><tool_call>{\"name\":\"write_file\",\"arguments\":{\"path\":\"private.txt\",\"content\":\"plan\"}}</tool_call></think>I will inspect the parser.".into(), tool_calls:vec![] });
        assert!(private_call.tool_calls.is_empty(), "private deliberation is never an executable text tool call");
        assert_eq!(private_call.text, "I will inspect the parser.");
    }
    #[test]
    fn standalone_dot_dialogue_never_creates_a_streamed_message() {
        let (events, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let mut text = String::new();
        let mut calls = std::collections::BTreeMap::new();
        for fragment in [".", "\n", "...", "\n"] {
            ModelClient::process_stream_value(&json!({"choices":[{"delta":{"content":fragment}}]}), &mut text, &mut calls, &events);
        }
        assert!(!std::iter::from_fn(|| receiver.try_recv().ok()).any(|event| event["type"] == "text_delta"));
        ModelClient::process_stream_value(&json!({"choices":[{"delta":{"content":"The search found the relevant file."}}]}), &mut text, &mut calls, &events);
        let event = receiver.try_recv().unwrap();
        assert_eq!(event["type"], "text_delta");
        assert_eq!(event["accumulated_text"], "The search found the relevant file.");
    }

    #[test]
    fn punctuation_cleanup_preserves_sentences_paths_numbers_and_fenced_code() {
        let text = "Found version 3.14 in .gitignore.\n.\n```text\n.\n...\n```\n…\nThe next check is ready.";
        assert_eq!(ModelClient::clean_dialogue_text(text), "Found version 3.14 in .gitignore.\n```text\n.\n...\n```\nThe next check is ready.");
        let reply = ModelClient::parse_message(&json!({"choices":[{"message":{"content":".","tool_calls":[{"id":"call","function":{"name":"list_files","arguments":"{\"path\":\".\"}"}}]}}]}));
        assert!(reply.text.is_empty());
        assert_eq!(reply.tool_calls.len(), 1, "cleaning dialogue must not discard real tool calls");
        assert_eq!(reply.tool_calls[0].arguments["path"], ".");
    }
    #[tokio::test]
    async fn utf8_split_across_http_chunks_preserves_file_names_and_code() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            let (header_end, length) = loop {
                let mut chunk = [0u8; 4096];
                let count = socket.read(&mut chunk).await.unwrap();
                assert!(count > 0);
                bytes.extend_from_slice(&chunk[..count]);
                if let Some(offset) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&bytes[..offset]).to_ascii_lowercase();
                    let length = headers.lines().find_map(|line| line.strip_prefix("content-length: ")).unwrap().parse::<usize>().unwrap();
                    break (offset + 4, length);
                }
            };
            while bytes.len() < header_end + length {
                let mut chunk = [0u8; 4096];
                let count = socket.read(&mut chunk).await.unwrap();
                assert!(count > 0);
                bytes.extend_from_slice(&chunk[..count]);
            }
            let delta = json!({"choices":[{"delta":{"content":"Editing café 🌌.","tool_calls":[{"index":0,"id":"unicode","function":{"name":"write_file","arguments":json!({"path":"café-🌌.txt","content":"こんにちは 🌌"}).to_string()}}]}}]});
            let body = format!("data: {delta}\r\n\r\ndata: [DONE]\r\n\r\n");
            socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).as_bytes()).await.unwrap();
            for byte in body.bytes() {
                socket.write_all(&[byte]).await.unwrap();
                if byte >= 128 { tokio::time::sleep(std::time::Duration::from_millis(2)).await; }
            }
        });
        let client = ModelClient::new(format!("http://{address}/v1"), "fixture", None, true, "none");
        *client.local_reasoning_caps.lock().unwrap()=Some(serde_json::Value::Null);
        let (events, _) = tokio::sync::mpsc::unbounded_channel();
        let reply = tokio::time::timeout(std::time::Duration::from_secs(5), client.chat_stream(&[], json!([]), &events)).await.unwrap().unwrap();
        assert_eq!(reply.text, "Editing café 🌌.");
        assert_eq!(reply.tool_calls[0].arguments["path"], "café-🌌.txt");
        assert_eq!(reply.tool_calls[0].arguments["content"], "こんにちは 🌌");
        server.await.unwrap();
    }

    #[tokio::test]
    async fn empty_step_recovers_tool_calls_with_prior_results_and_native_effort_intact() {
        use jynx_protocol::ChatMessage;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address=listener.local_addr().unwrap();
        let server=tokio::spawn(async move {
            for attempt in 0..3 {
                let (mut socket,_)=listener.accept().await.unwrap();let mut bytes=Vec::new();
                let (end,length)=loop {
                    let mut buf=[0u8;8192];let n=socket.read(&mut buf).await.unwrap();assert!(n>0);bytes.extend_from_slice(&buf[..n]);
                    if let Some(end)=bytes.windows(4).position(|v|v==b"\r\n\r\n") {
                        let head=String::from_utf8_lossy(&bytes[..end]).to_ascii_lowercase();
                        let length=head.lines().find_map(|l|l.strip_prefix("content-length:").map(|v|v.trim().parse::<usize>().unwrap())).unwrap();
                        if bytes.len()>=end+4+length {break (end+4,length);}
                    }
                };
                let payload:serde_json::Value=serde_json::from_slice(&bytes[end..end+length]).unwrap();
                assert_eq!(payload["tools"][0]["function"]["name"],"read_file");
                assert_eq!(payload["reasoning_effort"],"xhigh");
                let messages=payload["messages"].as_array().unwrap();
                assert!(messages.iter().any(|m|m["role"]=="tool"&&m["tool_call_id"]=="already-done"&&m["content"]=="PRIOR_RESULT_RETAINED"));
                let hints=messages.iter().filter(|m|m["content"].as_str().unwrap_or("").starts_with("Continue the current request from the existing tool results.")).count();
                assert_eq!(hints,usize::from(attempt==2),"one transient recovery hint, no accumulation");
                let (kind,body)=if attempt==0 {
                    ("text/event-stream","data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"Private synthetic reasoning\"}}]}\n\ndata: [DONE]\n\n".to_string())
                } else if attempt==1 {
                    ("application/json",json!({"choices":[{"message":{"content":"","reasoning_content":"Private reasoning only"}}]}).to_string())
                } else {
                    ("application/json",json!({"choices":[{"message":{"content":"I will inspect the remaining file.","tool_calls":[{"id":"next","type":"function","function":{"name":"read_file","arguments":"{\"path\":\"next.rs\"}"}}]}}]}).to_string())
                };
                socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            }
        });
        let messages=vec![ChatMessage{phase: None, role:"assistant".into(),content:"Earlier work".into(),images:vec![],tool_calls:Some(json!([{"id":"already-done","type":"function","function":{"name":"read_file","arguments":"{}"}}])),tool_call_id:None,name:None},ChatMessage{phase: None, role:"tool".into(),content:"PRIOR_RESULT_RETAINED".into(),images:vec![],tool_calls:None,tool_call_id:Some("already-done".into()),name:Some("read_file".into())}];
        let client=ModelClient::new(format!("http://{address}/v1"),"fixture",None,false,"xhigh");
        let (events,mut receiver)=tokio::sync::mpsc::unbounded_channel();
        let reply=tokio::time::timeout(std::time::Duration::from_secs(10),client.chat_stream(&messages,json!([{"type":"function","function":{"name":"read_file","parameters":{"type":"object"}}}]),&events)).await.unwrap().unwrap();
        assert_eq!(reply.tool_calls.len(),1);assert_eq!(reply.tool_calls[0].arguments["path"],"next.rs");
        assert!(!reply.text.contains("Private"));assert_eq!(messages.len(),2);
        let observed=std::iter::from_fn(||receiver.try_recv().ok()).collect::<Vec<_>>();
        assert_eq!(observed.iter().filter(|e|e["type"]=="stream_retry"&&e["reason"]=="empty_response").count(),1);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn empty_agent_responses_never_fall_back_to_text_only() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            for attempt in 0..6 {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut bytes = Vec::new();
                let (header_end, length) = loop {
                    let mut chunk = [0u8; 4096];
                    let count = socket.read(&mut chunk).await.unwrap();
                    assert!(count > 0);
                    bytes.extend_from_slice(&chunk[..count]);
                    if let Some(offset) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&bytes[..offset]).to_ascii_lowercase();
                        let length = headers.lines().find_map(|line| line.strip_prefix("content-length: ")).unwrap().parse::<usize>().unwrap();
                        break (offset + 4, length);
                    }
                };
                while bytes.len() < header_end + length {
                    let mut chunk = [0u8; 4096];
                    let count = socket.read(&mut chunk).await.unwrap();
                    assert!(count > 0);
                    bytes.extend_from_slice(&chunk[..count]);
                }
                let payload: serde_json::Value = serde_json::from_slice(&bytes[header_end..header_end + length]).unwrap();
                assert_eq!(payload["tools"][0]["function"]["name"], "edit_file");
                assert_eq!(payload["tool_choice"], "auto");
                let (content_type, body) = if attempt == 0 {
                    ("text/event-stream", "data: [DONE]\n\n")
                } else {
                    ("application/json", "{\"choices\":[{\"message\":{\"content\":\"\"}}]}")
                };
                socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
            }

        });
        let client = ModelClient::new(format!("http://{address}/v1"), "test", None, true, "none");
        *client.local_reasoning_caps.lock().unwrap()=Some(serde_json::Value::Null);
        let (events, _) = tokio::sync::mpsc::unbounded_channel();
        let error = tokio::time::timeout(std::time::Duration::from_secs(5), client.chat_stream(
            &[], json!([{"type":"function","function":{"name":"edit_file","parameters":{"type":"object"}}}]), &events
        )).await.unwrap().unwrap_err();
        assert!(error.to_string().contains("no visible text or tool calls after repeated attempts"), "{error}");
        assert!(!error.to_string().contains("tool-capable"));
        server.await.unwrap();
    }

    use super::{ModelClient, ModelReply, ModelToolCall};
    use serde_json::json;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
        sync::mpsc::unbounded_channel,
        time::{Duration, timeout},
    };

    #[test]
    fn streamed_tool_arguments_announce_preparation_before_execution() {
        let (events, mut receiver) = unbounded_channel();
        let mut text = String::new();
        let mut calls = std::collections::BTreeMap::new();
        ModelClient::process_stream_value(
            &json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"search-1","function":{"name":"web_search","arguments":"{\"query\":"}}]}}]}),
            &mut text,
            &mut calls,
            &events,
        );
        let preview = receiver.try_recv().unwrap();
        assert_eq!(preview["label"], "Preparing web search");
        assert_eq!(preview["tool_kind"], "web_search");
        ModelClient::process_stream_value(
            &json!({"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"quantum computing\"}"}}]}}]}),
            &mut text,
            &mut calls,
            &events,
        );
        let preview = receiver.try_recv().unwrap();
        assert_eq!(preview["tool_arguments"]["query"], "quantum computing");
        assert!(receiver.try_recv().is_err(), "completed arguments should be announced once");
        let finished = ModelClient::finish_stream_calls(calls).unwrap();
        assert_eq!(finished[0].name, "web_search");
        assert_eq!(finished[0].arguments["query"], "quantum computing");
    }

    #[test]
    fn listing_preview_requires_a_target_and_later_command_cannot_replace_it() {
        let (events,mut received)=unbounded_channel();let mut text=String::new();let mut calls=std::collections::BTreeMap::new();
        for fragment in ["{", "\"path\":", "\"src/fixt"] {
            ModelClient::process_stream_value(&json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"list-first","function":{"name":if calls.is_empty(){"list_files"}else{""},"arguments":fragment}}]}}]}),&mut text,&mut calls,&events);
            assert!(received.try_recv().is_err(),"an unnamed Listing row must never appear");
        }
        ModelClient::process_stream_value(&json!({"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"ures\"}"}}]}}]}),&mut text,&mut calls,&events);
        let preview=received.try_recv().unwrap();assert_eq!(preview["tool_kind"],"list_files");assert_eq!(preview["tool_arguments"]["path"],"src/fixtures");
        ModelClient::process_stream_value(&json!({"choices":[{"delta":{"tool_calls":[{"index":1,"id":"command-second","function":{"name":"run_command","arguments":"{\"command\":\"synthetic-check\"}"}}]}}]}),&mut text,&mut calls,&events);
        assert!(received.try_recv().is_err(),"a later command must not steal the first call's preview");
        let result=ModelClient::finish_stream_calls(calls).unwrap();assert_eq!(result[0].id,"list-first");assert_eq!(result[1].id,"command-second");
    }

    #[test]
    fn a_later_list_arriving_first_cannot_masquerade_as_the_first_running_call() {
        let (events,mut received)=unbounded_channel();let mut text=String::new();let mut calls=std::collections::BTreeMap::new();
        ModelClient::process_stream_value(&json!({"choices":[{"delta":{"tool_calls":[{"index":1,"id":"list-second","function":{"name":"list_files","arguments":"{\"path\":\"src\"}"}}]}}]}),&mut text,&mut calls,&events);
        assert!(received.try_recv().is_err(),"do not preview a later index before the first call arrives");
        ModelClient::process_stream_value(&json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"command-first","function":{"name":"run_command","arguments":"{\"command\":\"synthetic-check\"}"}}]}}]}),&mut text,&mut calls,&events);
        assert_eq!(received.try_recv().unwrap()["tool_kind"],"run_command");
        let result=ModelClient::finish_stream_calls(calls).unwrap();assert_eq!(result[0].id,"command-first");assert_eq!(result[1].id,"list-second");
    }

    #[test]
    fn streamed_read_preview_waits_for_a_complete_filename() {
        let (events, mut receiver) = unbounded_channel();
        let mut text = String::new();
        let mut calls = std::collections::BTreeMap::new();
        for fragment in ["{", "\"path\":", "\"src/fixt"] {
            ModelClient::process_stream_value(
                &json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"read-preview","function":{"name":if calls.is_empty() { "read_file" } else { "" },"arguments":fragment}}]}}]}),
                &mut text, &mut calls, &events,
            );
            assert!(receiver.try_recv().is_err(), "no unnamed Reading row while arguments stream");
        }
        ModelClient::process_stream_value(
            &json!({"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"ure.ts\",\"max_lines\":20}"}}]}}]}),
            &mut text, &mut calls, &events,
        );
        let preview = receiver.try_recv().unwrap();
        assert_eq!(preview["tool_arguments"]["path"], "src/fixture.ts");
        assert!(receiver.try_recv().is_err());
        assert_eq!(ModelClient::finish_stream_calls(calls).unwrap()[0].arguments["path"], "src/fixture.ts");
    }

    #[test]
    fn streamed_edit_preview_names_the_file_before_the_edit_body_finishes() {
        let (events, mut receiver) = unbounded_channel();
        let mut text = String::new();
        let mut calls = std::collections::BTreeMap::new();
        for fragment in ["{\"path\":", "\"src/café-🌌.ts\",\"old_text\":\"old", " text", "\",\"new_text\":\"new text\"}"] {
            ModelClient::process_stream_value(
                &json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"edit-preview","function":{"name":if calls.is_empty() { "edit_file" } else { "" },"arguments":fragment}}]}}]}),
                &mut text, &mut calls, &events,
            );
        }
        let first = receiver.try_recv().unwrap();
        assert!(first["tool_arguments"].is_null());
        let target = receiver.try_recv().unwrap();
        assert_eq!(target["tool_arguments"]["path"], "src/café-🌌.ts");
        assert!(target["tool_arguments"].get("old_text").is_none());
        let completed = receiver.try_recv().unwrap();
        assert_eq!(completed["tool_arguments"]["new_text"], "new text");
        assert!(receiver.try_recv().is_err(), "unchanged paths must not cause preview flicker");
        let finished = ModelClient::finish_stream_calls(calls).unwrap();
        assert_eq!(finished[0].arguments["old_text"], "old text");
    }

    #[test]
    fn partial_preview_reads_real_top_level_fields_only() {
        assert!(ModelClient::preview_tool_arguments(r#"{"content":"fake \"path\":\"wrong.ts\""#).is_none());
        assert!(ModelClient::preview_tool_arguments(r#"{"path":"unfinished"#).is_none());
        assert_eq!(ModelClient::preview_tool_arguments(r#"{"mode":"edit","path":"C:\\src\\real.ts","content":"unfinished"#).unwrap()["path"], "C:\\src\\real.ts");
        let (events, _) = unbounded_channel();
        let mut text = String::new();
        let mut calls = std::collections::BTreeMap::new();
        ModelClient::process_stream_value(&json!({"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"name":"edit_file","arguments":"{\"path\":\"real.ts\",\"old_text\":\"unfinished"}}]}}]}), &mut text, &mut calls, &events);
        assert!(ModelClient::finish_stream_calls(calls).is_err(), "preview extraction must never make incomplete arguments executable");
    }

    #[test]
    fn compatibility_tool_markup_announces_preparation_without_leaking_arguments() {
        let (events, mut receiver) = unbounded_channel();
        let mut text = String::new();
        let mut calls = std::collections::BTreeMap::new();
        for content in ["Let me search. <jynx_tool_", "call>{\"name\":\"web_search\",\"arguments\":{"] {
            ModelClient::process_stream_value(
                &json!({"choices":[{"delta":{"content":content}}]}),
                &mut text,
                &mut calls,
                &events,
            );
        }
        let mut preparing = 0;
        while let Ok(event) = receiver.try_recv() {
            if event["type"] == "status" {
                assert_eq!(event["label"], "Preparing tool call");
                preparing += 1;
            } else if event["type"] == "text_delta" {
                assert_eq!(event["accumulated_text"], "Let me search.");
            }
        }
        assert_eq!(preparing, 1);
    }

    #[test]
    fn local_efforts_are_native_and_never_impose_a_thinking_budget(){
        for effort in ["none","minimal","low","medium","high","xhigh","max"]{
            let client=ModelClient::new("http://localhost:8080","local",None,true,effort);
            let payload=client.payload_with_reasoning(json!({"model":"local"}));
            assert_eq!(payload["reasoning_effort"],effort);
            assert_eq!(payload["reasoning_format"],"deepseek");
            assert!(payload.get("reasoning_budget_tokens").is_none());
            assert!(payload.get("max_tokens").is_none());
            assert!(payload.get("chat_template_kwargs").is_none());
        }
        let client=ModelClient::new("http://localhost:8080","local",None,true,"default");
        assert!(client.payload_with_reasoning(json!({})).get("reasoning_effort").is_none());
    }

    #[test]
    fn native_effort_values_keep_their_json_type_and_app_none_omits_override(){
        for value in [json!("Deep-V3"),json!(2),json!(false),json!(null),json!("@jynx/none")]{
            let client=ModelClient::new("http://localhost:8080","fixture",None,true,crate::reasoning::key(&value));
            assert_eq!(client.payload_with_reasoning(json!({}))["reasoning_effort"],value);
        }
        let client=ModelClient::new("http://localhost:8080","fixture",None,true,crate::reasoning::NONE);
        assert!(client.payload_with_reasoning(json!({})).get("reasoning_effort").is_none());
    }

    #[test]
    fn phases_survive_json_stream_metadata_and_assistant_context_roundtrips() {
        for phase in ["commentary", "partial_answer", "final_answer"] {
            let reply = ModelClient::parse_message(&json!({"choices":[{"message":{"content":"Arbitrary synthetic text", "phase":phase},"finish_reason":"stop"}]}));
            assert_eq!(reply.phase.as_deref(), Some(phase));
            assert_eq!(ModelClient::message_phase(&json!({"choices":[{"delta":{"phase":phase}}]})).as_deref(), Some(phase));
        }
        assert!(ModelClient::message_phase(&json!({"choices":[{"delta":{"channel":"final","content":"Public progress text"}}]})).is_none());
        let client = ModelClient::new("http://127.0.0.1:1/v1", "fixture", None, false, "none");
        let messages: Vec<jynx_protocol::ChatMessage> = serde_json::from_value(json!([
            {"role":"assistant","content":"Synthetic update","phase":"commentary"},
            {"role":"user","content":"Synthetic follow-up","phase":"commentary"}
        ])).unwrap();
        let wire = client.request_messages(&messages).unwrap();
        assert_eq!(wire[0]["phase"], "commentary");
        assert!(wire[1].get("phase").is_none());
        let restored: Vec<jynx_protocol::ChatMessage> = serde_json::from_str(&serde_json::to_string(&messages).unwrap()).unwrap();
        assert_eq!(restored[0].phase.as_deref(), Some("commentary"));
    }

    #[tokio::test]
    async fn streaming_usage_after_the_finish_reason_is_captured_without_waiting_for_eof() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (release, wait) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0u8; 8192];
            socket.read(&mut request).await.unwrap();
            socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\ndata: {\"choices\":[{\"delta\":{\"content\":\"Synthetic answer.\"},\"finish_reason\":\"stop\"}]}\n\n").await.unwrap();
            tokio::time::sleep(Duration::from_millis(15)).await;
            socket.write_all(b"data: {\"choices\":[],\"usage\":{\"prompt_tokens\":1200,\"completion_tokens\":30}}\n\n").await.unwrap();
            let _ = wait.await;
        });
        let client = ModelClient::new(format!("http://{address}/v1"), "fixture", None, false, "none");
        let (events, _receiver) = unbounded_channel();
        let result = timeout(Duration::from_secs(3),client.chat_stream(&[],json!([]),&events)).await;
        let _ = release.send(());
        let reply = result.unwrap().unwrap();
        assert_eq!(reply.input_tokens, Some(1200));
        assert_eq!(reply.output_tokens, Some(30));
        assert_eq!(reply.text, "Synthetic answer.");
        server.await.unwrap();
    }

    #[tokio::test]
    async fn progress_dialogue_commits_only_after_generation_finishes() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (release, wait_for_release) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            let (header_end, content_length) = loop {
                let mut chunk = [0u8; 4096];
                let count = socket.read(&mut chunk).await.unwrap();
                assert!(count > 0);
                bytes.extend_from_slice(&chunk[..count]);
                if let Some(offset) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&bytes[..offset]).to_ascii_lowercase();
                    let length = headers.lines().find_map(|line| line.strip_prefix("content-length: ")).unwrap().parse::<usize>().unwrap();
                    break (offset + 4, length);
                }
            };
            while bytes.len() < header_end + content_length {
                let mut chunk = [0u8; 4096];
                let count = socket.read(&mut chunk).await.unwrap();
                assert!(count > 0);
                bytes.extend_from_slice(&chunk[..count]);
            }
            let payload: serde_json::Value = serde_json::from_slice(&bytes[header_end..header_end + content_length]).unwrap();
            assert_eq!(payload["stream"], true);
            assert!(payload.get("tools").is_none());
            assert!(payload.get("tool_choice").is_none());
            let first = "data: {\"choices\":[{\"delta\":{\"content\":\"I will search \"}}]}\n\n";
            let last = "data: {\"choices\":[{\"delta\":{\"content\":\"several sources.\"}}]}\n\ndata: [DONE]\n\n";
            socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{first}", first.len() + last.len()).as_bytes()).await.unwrap();
            wait_for_release.await.unwrap();
            socket.write_all(last.as_bytes()).await.unwrap();
        });
        let client = ModelClient::new(format!("http://{address}/v1"), "test", None, false, "none");
        let (events, mut receiver) = unbounded_channel();
        let generation = tokio::spawn(async move {
            client.generate_progress_dialog(&[], &[ModelToolCall {
                id: "search".into(), name: "web_search".into(), arguments: json!({"query":"quantum computing"}),
            }], &events).await
        });
        assert!(timeout(Duration::from_millis(100), receiver.recv()).await.is_err(), "An unfinished progress request must not leak partial dialogue");
        assert!(!generation.is_finished());
        release.send(()).unwrap();
        let dialog = timeout(Duration::from_secs(5), generation).await.unwrap().unwrap().unwrap();
        assert_eq!(dialog, "I will search several sources.");
        let event = receiver.recv().await.unwrap();
        assert_eq!(event["text"], dialog);
        assert_eq!(event["accumulated_text"], dialog);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn cancelled_progress_dialogue_does_not_leave_a_partial_letter() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (ready, wait_ready) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0u8; 8192];
            socket.read(&mut request).await.unwrap();
            socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\ndata: {\"choices\":[{\"delta\":{\"content\":\"I\"}}]}\n\n").await.unwrap();
            ready.send(()).unwrap();
            std::future::pending::<()>().await;
        });
        let client = ModelClient::new(format!("http://{address}/v1"), "fixture", None, false, "none");
        let (events, mut receiver) = unbounded_channel();
        let generation = tokio::spawn(async move { client.generate_progress_dialog(&[], &[], &events).await });
        timeout(Duration::from_secs(5), wait_ready).await.unwrap().unwrap();
        assert!(timeout(Duration::from_millis(100), receiver.recv()).await.is_err());
        generation.abort();
        assert!(generation.await.unwrap_err().is_cancelled());
        assert!(receiver.try_recv().is_err());
        server.abort();
    }

    #[tokio::test]
    async fn incomplete_progress_replies_do_not_publish_dialogue() {
        for (body, finish) in [("I", "stop"), ("I will check the", "length")] {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let response = json!({"choices":[{"message":{"content":body},"finish_reason":finish}]}).to_string();
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = [0u8; 8192];
                socket.read(&mut request).await.unwrap();
                socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}", response.len()).as_bytes()).await.unwrap();
            });
            let client = ModelClient::new(format!("http://{address}/v1"), "fixture", None, false, "none");
            let (events, mut receiver) = unbounded_channel();
            assert_eq!(client.generate_progress_dialog(&[], &[], &events).await.unwrap(), "");
            assert!(receiver.try_recv().is_err());
            server.await.unwrap();
        }
    }

    #[tokio::test]
    async fn completed_tool_stream_does_not_wait_for_http_connection_close() {
        for finish in ["data: [DONE]\n\n", "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n"] {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let (release, wait_for_release) = tokio::sync::oneshot::channel();
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = [0u8; 4096];
                let _ = socket.read(&mut request).await.unwrap();
                let call = json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"list-1","function":{"name":"list_files","arguments":"{\"path\":\".\"}"}}]}}]});
                socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\ndata: {call}\n\n{finish}").as_bytes()).await.unwrap();


                let _ = wait_for_release.await;
            });
            let client = ModelClient::new(format!("http://{address}/v1"), "test", None, false, "none");
            let (events, _receiver) = unbounded_channel();
            let reply = timeout(Duration::from_secs(3), client.chat_stream(&[], json!([]), &events)).await;
            let _ = release.send(());
            server.await.unwrap();
            let reply = reply.expect("finished tool stream waited for HTTP EOF").unwrap();
            assert_eq!(reply.tool_calls.len(), 1);
            assert_eq!(reply.tool_calls[0].name, "list_files");
            assert_eq!(reply.tool_calls[0].arguments["path"], ".");
        }
    }

    #[test]
    fn cloud_max_uses_highest_advertised_effort_without_sampling_override() {
        let client = ModelClient::new("https://api.example/v1", "test", None, false, "max");
        let mut payload = json!({"model":"test","temperature":0.2});
        client
            .apply_max_capabilities(
                &mut payload,
                &json!({"reasoning":{"supported_efforts":["medium","high","xhigh"]}}),
            )
            .unwrap();
        assert_eq!(payload["reasoning_effort"], "xhigh");
        assert!(payload.get("temperature").is_none());
        assert!(
            client
                .apply_max_capabilities(
                    &mut payload,
                    &json!({"reasoning":{"supported_efforts":["none"]}})
                )
                .is_err()
        );
    }

    #[test]
    fn openrouter_max_uses_native_reasoning_and_requires_provider_support() {
        let client = ModelClient::new("https://openrouter.ai/api/v1", "test", None, false, "max");
        let mut payload = client.payload_with_reasoning(json!({"model":"test"}));
        client
            .apply_max_capabilities(&mut payload, &json!({}))
            .unwrap();
        assert_eq!(payload["reasoning"], json!({"effort":"max","enabled":true}));
        assert_eq!(payload["provider"]["require_parameters"], true);
        assert!(payload.get("reasoning_effort").is_none());
    }

    #[tokio::test]
    async fn max_negotiates_efforts_without_ever_dropping_reasoning() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            for effort in ["max", "xhigh", "high"] {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut bytes = Vec::new();
                let header_end;
                let content_length;
                loop {
                    let mut chunk = [0u8; 4096];
                    let count = socket.read(&mut chunk).await.unwrap();
                    assert!(count > 0);
                    bytes.extend_from_slice(&chunk[..count]);
                    if let Some(offset) = bytes.windows(4).position(|window| window == b"\r\n\r\n")
                    {
                        header_end = offset + 4;
                        let headers =
                            String::from_utf8_lossy(&bytes[..offset]).to_ascii_lowercase();
                        content_length = headers
                            .lines()
                            .find_map(|line| line.strip_prefix("content-length: "))
                            .unwrap()
                            .parse::<usize>()
                            .unwrap();
                        break;
                    }
                }
                while bytes.len() < header_end + content_length {
                    let mut chunk = [0u8; 4096];
                    let count = socket.read(&mut chunk).await.unwrap();
                    assert!(count > 0);
                    bytes.extend_from_slice(&chunk[..count]);
                }
                let payload: serde_json::Value =
                    serde_json::from_slice(&bytes[header_end..header_end + content_length])
                        .unwrap();
                assert_eq!(payload["reasoning_effort"], effort);
                let body = r#"{"error":"unsupported reasoning_effort"}"#;
                let response = format!(
                    "HTTP/1.1 400 Bad Request\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                socket.write_all(response.as_bytes()).await.unwrap();
            }
        });
        let client = ModelClient::new(format!("http://{address}/v1"), "test", None, false, "max");


        *client.reasoning_catalog.lock().unwrap() = Some(serde_json::Value::Null);
        let error = timeout(
            Duration::from_secs(5),
            client.send_payload_with_retry(&json!({"model":"test"}), None, "test"),
        )
        .await
        .unwrap()
        .unwrap_err();
        assert!(error.to_string().contains("Max reasoning is not supported"));
        server.await.unwrap();
    }

    #[tokio::test]
    async fn interrupted_stream_retries_without_accepting_partial_reply() {
        for fault in ["transport", "missing_completion", "malformed_arguments"] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            for attempt in 0..2 {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = [0u8; 4096];
                let _ = socket.read(&mut request).await.unwrap();
                let body = if attempt == 0 {
                    if fault == "malformed_arguments" {
                        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"name\":\"write_file\",\"arguments\":\"{\\\"path\\\":\"}}]}}]}\n\ndata: [DONE]\n\n"
                    } else {
                        "data: {\"choices\":[{\"delta\":{\"content\":\"partial\"}}]}\n\n"
                    }
                } else {
                    "data: {\"choices\":[{\"delta\":{\"content\":\"complete\"}}]}\n\ndata: [DONE]\n\n"
                };
                let declared_length = if attempt == 0 && fault == "transport" {
                    body.len() + 50
                } else {
                    body.len()
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {declared_length}\r\nConnection: close\r\n\r\n{body}"
                );
                socket.write_all(response.as_bytes()).await.unwrap();
            }
        });
        let model = ModelClient::new(format!("http://{address}"), "test", None, true, "low");
        *model.local_reasoning_caps.lock().unwrap()=Some(serde_json::Value::Null);
        let (events, mut receiver) = unbounded_channel();
        let reply = timeout(
            Duration::from_secs(5),
            model.chat_stream(&[], json!([]), &events),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(reply.text, "complete");
        assert!(reply.tool_calls.is_empty(), "an incomplete call must not escape into execution");
        let mut saw_retry = false;
        while let Ok(event) = receiver.try_recv() {
            if event["type"] == "stream_retry" {
                saw_retry = true;
            }
        }
        assert!(saw_retry);
        server.await.unwrap();
        }
    }

    #[test]
    fn structured_completion_reasons_preserve_output_limit_signals() {
        for reason in ["length", "max_tokens", "max_output_tokens"] {
            let reply=ModelClient::parse_message(&json!({"choices":[{"message":{"content":"Partial implementation"},"finish_reason":reason}]}));
            assert!(reply.output_limit_reached());
            assert_eq!(reply.finish_reason.as_deref(),Some(reason));
        }
        let reply=ModelClient::parse_message(&json!({"choices":[{"message":{"content":"Complete."},"finish_reason":"stop"}]}));
        assert!(!reply.output_limit_reached());
    }

    #[test]
    fn nested_xml_tool_markup_is_hidden_and_parsed() {
        let reply = ModelClient::normalize_tool_markup(ModelReply {
            input_tokens: None, output_tokens: None, phase: None, finish_reason: None,
            text: "Checking now.\n<tool_call><function=read_file><parameter=path>src/main.rs</parameter></function></tool_call>".into(),
            tool_calls: Vec::new(),
        });
        assert_eq!(reply.text, "Checking now.");
        assert_eq!(reply.tool_calls.len(), 1);
        assert_eq!(reply.tool_calls[0].name, "read_file");
        assert_eq!(reply.tool_calls[0].arguments["path"], "src/main.rs");
    }

    #[test]
    fn malformed_xml_noise_is_hidden_without_overriding_native_calls() {
        let reply = ModelClient::normalize_tool_markup(ModelReply {
            input_tokens: None, output_tokens: None, phase: None, finish_reason: None,
            text: "Working.\n<tool_call><function=read_file><parameter=path></parameter></function></tool_call>".into(),
            tool_calls: vec![ModelToolCall {
                id: "native-1".into(),
                name: "read_file".into(),
                arguments: json!({"path":"Cargo.toml"}),
            }],
        });
        assert_eq!(reply.text, "Working.");
        assert_eq!(reply.tool_calls.len(), 1);
        assert_eq!(reply.tool_calls[0].id, "native-1");
    }

    #[test]
    fn unfinished_stream_markup_never_becomes_visible_text() {
        assert_eq!(
            ModelClient::visible_stream_text("Working.\n<tool_call><function=read_file>"),
            "Working."
        );
        assert_eq!(
            ModelClient::visible_stream_text("Working.\n<tool_"),
            "Working."
        );
    }

    #[test]
    fn tool_and_args_json_dialect_is_hidden_and_parsed() {
        let reply = ModelClient::normalize_tool_markup(ModelReply {
            input_tokens: None, output_tokens: None, phase: None, finish_reason: None,
            text: "Checking files.\n<tool_call>\n{\"tool\":\"list_files\",\"args\":{\"path\":\"$workspace\"}}\n</tool_call>".into(),
            tool_calls: Vec::new(),
        });
        assert_eq!(reply.text, "Checking files.");
        assert_eq!(reply.tool_calls.len(), 1);
        assert_eq!(reply.tool_calls[0].name, "list_files");
        assert_eq!(reply.tool_calls[0].arguments["path"], "$workspace");
    }

    #[test]
    fn malformed_protocol_payload_is_never_visible_text() {
        let reply = ModelClient::normalize_tool_markup(ModelReply {
            input_tokens: None, output_tokens: None, phase: None, finish_reason: None,
            text: "Checking files.\n<tool_call>{not valid json}</tool_call>".into(),
            tool_calls: Vec::new(),
        });
        assert_eq!(reply.text, "Checking files.");
        assert!(reply.tool_calls.is_empty());
    }
}

#[cfg(test)]
mod native_reasoning_tests{
    use super::*;
    use tokio::io::{AsyncReadExt,AsyncWriteExt};
    use tokio::net::{TcpListener,TcpStream};
    async fn read_request(socket:&mut TcpStream)->(String,Value){
        let mut bytes=Vec::new();let (header_end,length)=loop{
            let mut chunk=[0u8;4096];let count=socket.read(&mut chunk).await.unwrap();assert!(count>0);bytes.extend_from_slice(&chunk[..count]);
            if let Some(offset)=bytes.windows(4).position(|w|w==b"\r\n\r\n"){
                let headers=String::from_utf8_lossy(&bytes[..offset]).to_ascii_lowercase();
                let length=headers.lines().find_map(|line|line.strip_prefix("content-length: ")).map(|n|n.parse::<usize>().unwrap()).unwrap_or(0);break(offset+4,length);
            }
        };
        while bytes.len()<header_end+length{let mut chunk=[0u8;4096];let count=socket.read(&mut chunk).await.unwrap();assert!(count>0);bytes.extend_from_slice(&chunk[..count]);}
        let line=String::from_utf8_lossy(&bytes[..header_end]).lines().next().unwrap().to_owned();
        let body=if length>0{serde_json::from_slice(&bytes[header_end..header_end+length]).unwrap()}else{Value::Null};(line,body)
    }
    async fn respond(socket:&mut TcpStream,status:&str,body:Value){let body=body.to_string();socket.write_all(format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();}
    #[tokio::test]
    async fn reported_native_efforts_are_sent_without_prompt_or_budget_changes(){
        let listener=TcpListener::bind("127.0.0.1:0").await.unwrap();let address=listener.local_addr().unwrap();
        let server=tokio::spawn(async move{
            for expected in ["low","medium","xhigh","xhigh","xhigh"]{
                let(mut socket,_)=listener.accept().await.unwrap();let(line,_)=read_request(&mut socket).await;assert!(line.starts_with("GET /props "));
                respond(&mut socket,"200 OK",json!({"chat_template_caps":{"reasoning_effort_levels":["low","medium","xhigh"]}})).await;
                let(mut socket,_)=listener.accept().await.unwrap();let(line,body)=read_request(&mut socket).await;assert!(line.starts_with("POST /v1/chat/completions "));
                assert_eq!(body["reasoning_effort"],expected);assert_eq!(body["messages"][0]["content"],"Synthetic fixed instructions");
                assert!(body.get("reasoning_budget_tokens").is_none());assert!(body.get("chat_template_kwargs").is_none());
                respond(&mut socket,"200 OK",json!({"ok":true})).await;
            }
        });
        for effort in ["low","medium","high","xhigh","max"]{
            let client=ModelClient::new(format!("http://{address}/v1"),"fixture",None,true,effort);
            tokio::time::timeout(Duration::from_secs(5),client.send_payload_with_retry(&json!({"model":"fixture","messages":[{"role":"system","content":"Synthetic fixed instructions"}]}),None,"fixture")).await.unwrap().unwrap();
        }
        server.await.unwrap();
    }
    #[tokio::test]
    async fn rejected_toggle_uses_provider_default_and_publishes_actual_efforts(){
        let listener=TcpListener::bind("127.0.0.1:0").await.unwrap();let address=listener.local_addr().unwrap();
        let server=tokio::spawn(async move{
            let(mut socket,_)=listener.accept().await.unwrap();let(line,_)=read_request(&mut socket).await;assert!(line.starts_with("GET /props "));respond(&mut socket,"404 Not Found",json!({})).await;
            for step in 0..3{
                let(mut socket,_)=listener.accept().await.unwrap();let(_,body)=read_request(&mut socket).await;
                assert_eq!(body["messages"][0]["content"],"Synthetic instructions");
                if step==0{
                    assert_eq!(body["reasoning_effort"],"on");
                    respond(&mut socket,"400 Bad Request",json!({"error":{"message":"Invalid 'reasoning_effort' value: 'on'. Supported values: none, minimal, low, medium, high, xhigh."}})).await;
                }else{
                    assert!(body.get("reasoning_effort").is_none());
                    respond(&mut socket,"200 OK",json!({"ok":true})).await;
                }
            }
        });
        let client=ModelClient::new(format!("http://{address}/v1"),"fixture",None,true,"on");
        let(events,mut observed)=tokio::sync::mpsc::unbounded_channel();
        for _ in 0..2{tokio::time::timeout(Duration::from_secs(5),client.send_payload_with_retry(&json!({"model":"fixture","messages":[{"role":"system","content":"Synthetic instructions"}]}),Some(&events),"fixture")).await.unwrap().unwrap();}
        assert!(std::iter::from_fn(||observed.try_recv().ok()).any(|event|event["type"]=="reasoning_capabilities"&&event["selected"]=="default"&&event["efforts"].as_array().unwrap().iter().any(|value|value=="xhigh")));
        server.await.unwrap();
    }
    #[tokio::test]
    async fn jinja_500_rejection_retries_the_advertised_effort_and_caches_it(){
        let listener=TcpListener::bind("127.0.0.1:0").await.unwrap();let address=listener.local_addr().unwrap();
        let server=tokio::spawn(async move{
            let(mut socket,_)=listener.accept().await.unwrap();let(line,_)=read_request(&mut socket).await;assert!(line.starts_with("GET /props "));respond(&mut socket,"404 Not Found",json!({})).await;
            for step in 0..3{
                let(mut socket,_)=listener.accept().await.unwrap();let(_,body)=read_request(&mut socket).await;
                assert_eq!(body["reasoning_effort"],if step==0{"high"}else{"xhigh"});assert!(body.get("reasoning_budget_tokens").is_none());
                if step==0{respond(&mut socket,"500 Internal Server Error",json!({"error":{"code":500,"message":"Jinja Exception: Unexpected reasoning effort high. Supported types are xhigh (default), medium, and low."}})).await;}
                else{respond(&mut socket,"200 OK",json!({"ok":true})).await;}
            }
        });
        let client=ModelClient::new(format!("http://{address}/v1"),"fixture",None,true,"high");
        let(events,mut observed)=tokio::sync::mpsc::unbounded_channel();
        for _ in 0..2{tokio::time::timeout(Duration::from_secs(5),client.send_payload_with_retry(&json!({"model":"fixture"}),Some(&events),"fixture")).await.unwrap().unwrap();}
        assert!(std::iter::from_fn(||observed.try_recv().ok()).any(|event|event["type"]=="reasoning_capabilities"&&event["selected"]=="xhigh"));
        assert!(!ModelClient::reasoning_parameter_rejected(reqwest::StatusCode::INTERNAL_SERVER_ERROR,"GPU out of memory",&json!({"reasoning_effort":"high"})));
        server.await.unwrap();
    }
}

#[cfg(test)]
mod private_channel_tests{
    use super::*;
    use tokio::io::{AsyncReadExt,AsyncWriteExt};
    #[test]
    fn private_typed_response_parts_and_channel_wrappers_never_become_final_text(){
        for kind in ["reasoning_content","reasoning_summary_text","redacted_thinking","summary_text","analysis_text"]{
            let reply=ModelClient::parse_message(&json!({"output":[{"type":kind,"text":"PRIVATE"},{"type":"message","content":[{"type":"output_text","text":"Public answer"}]}]}));
            assert_eq!(reply.text,"Public answer");
        }
        assert!(ModelClient::parse_message(&json!({"channel":"thinking","response":"PRIVATE"})).text.is_empty());
        assert!(ModelClient::parse_message(&json!({"choices":[{"channel":"analysis","message":{"content":"PRIVATE"}}]})).text.is_empty());
    }
    #[tokio::test]
    async fn private_stream_channel_persists_until_final_and_does_not_reach_the_final_reply(){
        let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let address=listener.local_addr().unwrap();
        let server=tokio::spawn(async move{
            let(mut socket,_)=listener.accept().await.unwrap();let mut bytes=[0u8;4096];socket.read(&mut bytes).await.unwrap();
            let chunks=[json!({"choices":[{"delta":{"channel":"analysis","content":"PRIVATE_A"}}]}),json!({"choices":[{"delta":{"content":"PRIVATE_B"}}]}),json!({"choices":[{"delta":{"channel":"final","content":"Public "}}]}),json!({"choices":[{"delta":{"content":"answer."}}]})];
            let mut body=chunks.iter().map(|chunk|format!("data: {chunk}\n\n")).collect::<String>();body.push_str("data: [DONE]\n\n");
            socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
        });
        let client=ModelClient::new(format!("http://{address}/v1"),"fixture",None,false,"default");let(events,mut received)=tokio::sync::mpsc::unbounded_channel();
        let reply=tokio::time::timeout(Duration::from_secs(5),client.chat_stream(&[],json!([]),&events)).await.unwrap().unwrap();
        assert_eq!(reply.text,"Public answer.");
        let mut saw_private_activity=false;
        while let Ok(event)=received.try_recv(){
            if event["type"]=="text_delta"{assert!(!event.to_string().contains("PRIVATE"));}
            if event["type"]=="reasoning_delta"{saw_private_activity=true;}
        }
        assert!(saw_private_activity);server.await.unwrap();
    }
}
