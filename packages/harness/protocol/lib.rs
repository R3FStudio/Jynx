use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcRequest {
    pub id: Value,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcResponse {
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
    pub role: String,
    pub content: String,
    #[serde(default, skip_serializing)]
    pub images: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelProviderConfig {
    pub endpoint: String,
    pub model: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub label: Option<String>,



    #[serde(default)]
    pub local_compat: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSettings {
    #[serde(default = "default_true")]
    pub web_search_enabled: bool,
    #[serde(default = "default_true")]
    pub browser_tool_enabled: bool,
    #[serde(default)]
    pub auto_open_tool_details: bool,
    #[serde(default = "default_true")]
    pub show_reasoning_summaries: bool,
    #[serde(default = "default_permission_mode")]
    pub permission_mode: String,
    #[serde(default = "default_reasoning_effort")]
    pub reasoning_effort: String,


    #[serde(default)]
    pub host_instructions: String,

    #[serde(default)]
    pub user_feedback: String,


    #[serde(default)]
    pub host_browser_enabled: bool,

    #[serde(default)]
    pub host_terminal_enabled: bool,


    #[serde(default)]
    pub compaction_presentation: bool,

    #[serde(default)]
    pub final_answer_presentation: bool,
    #[serde(default)]
    pub temperature: Option<f64>,
    #[serde(default)]
    pub max_output_tokens: Option<u32>,
    #[serde(default)]
    pub auto_compact_enabled: Option<bool>,
    #[serde(default)]
    pub compact_at_ratio: Option<f32>,
    #[serde(default)]
    pub max_tool_rounds: Option<usize>,
    #[serde(default)]
    pub disabled_tools: Vec<String>,
    #[serde(default)]
    pub native_tool_calling: Option<bool>,
    #[serde(default)]
    pub mcp_enabled: Option<bool>,
    #[serde(default)]
    pub web_search_max_results: Option<u64>,
    #[serde(default)]
    pub web_fetch_max_chars: Option<u64>,
    #[serde(default)]
    pub mcp_timeout_ms: Option<u64>,
    #[serde(default)]
    pub mcp_max_output_chars: Option<usize>,
    #[serde(default)]
    pub host_editor_tools: bool,
}

impl Default for AgentSettings {
    fn default() -> Self {
        Self {
            web_search_enabled: true,
            browser_tool_enabled: true,
            auto_open_tool_details: false,
            show_reasoning_summaries: true,
            permission_mode: ":ask".into(),
            reasoning_effort: "medium".into(),
            host_instructions: String::new(),
            user_feedback: String::new(),
            host_browser_enabled: false,
            host_terminal_enabled: false,
            compaction_presentation: false,
            final_answer_presentation: false,
            temperature: None,
            max_output_tokens: None,
            auto_compact_enabled: None,
            compact_at_ratio: None,
            max_tool_rounds: None,
            disabled_tools: Vec::new(),
            native_tool_calling: None,
            mcp_enabled: None,
            web_search_max_results: None,
            web_fetch_max_chars: None,
            mcp_timeout_ms: None,
            mcp_max_output_chars: None,
            host_editor_tools: false,
        }
    }
}

fn default_true() -> bool {
    true
}
fn default_permission_mode() -> String {
    ":ask".into()
}
fn default_reasoning_effort() -> String {
    "medium".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRequest {
    #[serde(default)]
    pub request_id: String,
    pub model_path: String,
    pub runtime: String,
    pub context_size: usize,
    pub cwd: String,
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub provider: Option<ModelProviderConfig>,
    #[serde(default)]
    pub settings: AgentSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolEvent {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub status: String,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub arguments: Option<Value>,
    #[serde(default)]
    pub output: Option<String>,
    #[serde(default)]
    pub diff: Option<String>,
    #[serde(default)]
    pub batch_id: Option<String>,
    #[serde(default)]
    pub batch_index: Option<usize>,
    #[serde(default)]
    pub batch_size: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResponse {
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub context_messages: Option<Vec<ChatMessage>>,
    #[serde(default)]
    pub awaiting_user_input: bool,
    pub text: String,
    pub tools: Vec<ToolEvent>,
    pub compacted: bool,
    #[serde(default)]
    pub context_used: usize,
    #[serde(default)]
    pub context_estimated: bool,
    #[serde(default)]
    pub context_limit: usize,
}
