use anyhow::Result;
use jynx_agent::{Agent, AgentControl, ApprovalGate};
use std::{collections::HashMap, sync::Arc};
use jynx_mcp::{
    add_server as add_mcp_server, ensure_config as ensure_mcp_config,
    read_config as read_mcp_config,
};
use jynx_protocol::{AgentRequest, RpcError, RpcRequest, RpcResponse};
use jynx_storage::{ChatStore, RuntimeStateStore, default_state_root};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

pub async fn serve_stdio() -> Result<()> {
    let stdin = tokio::io::stdin();
    let mut lines = BufReader::new(stdin).lines();
    let (out_tx, mut out_rx) = unbounded_channel::<Value>();
    let writer = tokio::spawn(async move {
        let mut stdout = tokio::io::stdout();
        while let Some(value) = out_rx.recv().await {
            stdout
                .write_all(serde_json::to_string(&value)?.as_bytes())
                .await?;
            stdout.write_all(b"\n").await?;
            stdout.flush().await?;
        }
        Result::<()>::Ok(())
    });




    let approval_gate = ApprovalGate::default();
    let controls = Arc::new(tokio::sync::Mutex::new(HashMap::<String, AgentControl>::new()));

    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        let request = match serde_json::from_str::<RpcRequest>(&line) {
            Ok(request) => request,
            Err(error) => {
                let response = RpcResponse {
                    id: Value::Null,
                    result: None,
                    error: Some(RpcError {
                        code: -32700,
                        message: error.to_string(),
                    }),
                };
                let _ = out_tx.send(serde_json::to_value(response)?);
                continue;
            }
        };

        if request.method == "agent/run" {
            let tx = out_tx.clone();
            let gate = approval_gate.clone();
            let request_id = request.params["request_id"].as_str().unwrap_or_default().to_string();
            let control = AgentControl::default();
            controls.lock().await.insert(request_id.clone(), control.clone());
            let active_controls = Arc::clone(&controls);
            tokio::spawn(async move {
                if let Err(error) = serve_agent(request, tx.clone(), gate, control).await {
                    let _ = tx.send(
                        json!({"method":"server/error","params":{"message":error.to_string()}}),
                    );
                }
                active_controls.lock().await.remove(&request_id);
            });
            continue;
        }

        if request.method == "agent/control" {
            let controls = controls.lock().await;
            let control = controls.get(request.params["request_id"].as_str().unwrap_or_default());
            if let Some(control) = control {
                if let Some(id)=request.params["host_tool_id"].as_str() { control.resolve_editor_tool(id,request.params["output"].clone()); }
                if let Some(paused) = request.params["paused"].as_bool() {
                    control.set_paused(paused, request.params["restart_compaction"].as_bool().unwrap_or(false));
                }
                if let Some(id) = request.params["compaction_visible"].as_str() {
                    control.acknowledge_compaction(id.to_string(), request.params["attempt"].as_u64().unwrap_or(1));
                }
            }
            let _ = out_tx.send(serde_json::to_value(RpcResponse { id: request.id, result: Some(json!({"ok":control.is_some()})), error: None })?);
            continue;
        }

        if request.method == "approval/respond" {
            let approval_id = request.params["approval_id"].as_str().unwrap_or_default();
            let approved = request.params["approved"].as_bool().unwrap_or(false);
            let ok = if approval_id.is_empty() {
                false
            } else {
                approval_gate.resolve(approval_id, approved).await
            };
            let response = RpcResponse {
                id: request.id,
                result: Some(json!({"ok":ok})),
                error: None,
            };
            let _ = out_tx.send(serde_json::to_value(response)?);
            continue;
        }

        let response = handle(request).await;
        let _ = out_tx.send(serde_json::to_value(response)?);
    }

    drop(out_tx);
    writer.await??;
    Ok(())
}

async fn serve_agent(
    request: RpcRequest,
    out_tx: UnboundedSender<Value>,
    approval_gate: ApprovalGate,
    control: AgentControl,
) -> Result<()> {
    let id = request.id.clone();
    let parsed = serde_json::from_value::<AgentRequest>(request.params);
    let input = match parsed {
        Ok(input) => input,
        Err(error) => {
            let response = RpcResponse {
                id,
                result: None,
                error: Some(RpcError {
                    code: -32602,
                    message: error.to_string(),
                }),
            };
            let _ = out_tx.send(serde_json::to_value(response)?);
            return Ok(());
        }
    };
    let request_id = if input.request_id.trim().is_empty() {
        id.to_string()
    } else {
        input.request_id.clone()
    };
    let (tx, mut rx) = unbounded_channel::<Value>();
    let task = tokio::spawn(run_agent(input, tx, approval_gate, control));
    tokio::pin!(task);
    loop {
        tokio::select! {
            event = rx.recv() => {
                if let Some(event) = event {
                    let notification = json!({"method":"agent/event","params":{"request_id":request_id,"event":event}});
                    let _ = out_tx.send(notification);
                }
            }
            result = &mut task => {
                while let Ok(event) = rx.try_recv() {
                    let notification = json!({"method":"agent/event","params":{"request_id":request_id,"event":event}});
                    let _ = out_tx.send(notification);
                }
                let response = match result {
                    Ok(Ok(value)) => RpcResponse { id, result: Some(value), error: None },
                    Ok(Err(error)) => RpcResponse { id, result: None, error: Some(RpcError { code: -32000, message: error.to_string() }) },
                    Err(error) => RpcResponse { id, result: None, error: Some(RpcError { code: -32001, message: error.to_string() }) },
                };
                let _ = out_tx.send(serde_json::to_value(response)?);
                break;
            }
        }
    }
    Ok(())
}

async fn handle(request: RpcRequest) -> RpcResponse {
    let result = match request.method.as_str() {
        "server/diagnostics" => Ok(json!({
            "name":"jynx-harness",
            "protocol_version":6,
            "transport":"stdio-jsonl",
            "features":[
                "local-models","api-models","provider-routing-v2","streaming-v1","typed-tools-v1",
                "free-web-search-v1","sandbox","prompt-injection-boundaries","compaction","workspace-permissions",
                "thread-store-v1","runtime-state-v1","approvals-v1","vision-input-v1","mcp-stdio-tools-v1","compaction-control-v1","compaction-paint-v1","questions-v1","manual-compaction-v1","message-feedback-v1","native-reasoning-v1","dynamic-reasoning-v1","context-snapshots-v1","thread-summaries-v1","verified-local-compaction-v1","jynx-branding-v1"
            ]
        })),
        "mcp/config" => async {
            let path = ensure_mcp_config().await?;
            let config = read_mcp_config().await?;
            let mut servers: Vec<_> = config.servers.iter().map(|(name, value)| json!({"name":name,"enabled":value.enabled,"command":value.command})).collect();
            servers.sort_by_key(|entry| entry["name"].as_str().unwrap_or_default().to_owned());
            Ok(json!({"path":path,"servers":servers}))
        }.await,
        "mcp/config/add" => async {
            let name = request.params["name"].as_str().ok_or_else(|| anyhow::anyhow!("MCP name is required"))?;
            let command = request.params["command"].as_str().ok_or_else(|| anyhow::anyhow!("MCP executable is required"))?;
            let args = request.params["args"].as_array().ok_or_else(|| anyhow::anyhow!("MCP args must be an array"))?
                .iter().map(|value| value.as_str().map(str::to_owned).ok_or_else(|| anyhow::anyhow!("MCP args must be strings"))).collect::<Result<Vec<_>>>()?;
            add_mcp_server(name, command, args).await?;
            let path = ensure_mcp_config().await?;
            let config = read_mcp_config().await?;
            let mut servers: Vec<_> = config.servers.iter().map(|(name, value)| json!({"name":name,"enabled":value.enabled,"command":value.command})).collect();
            servers.sort_by_key(|entry| entry["name"].as_str().unwrap_or_default().to_owned());
            Ok(json!({"path":path,"servers":servers}))
        }.await,
        "context/compact" => async {
            let messages:Vec<jynx_protocol::ChatMessage>=serde_json::from_value(request.params["messages"].clone())?;
            let limit=request.params["context_size"].as_u64().unwrap_or(20000).max(1) as usize;
            let before=jynx_context::estimate_tokens(&messages);
            let result=tokio::task::spawn_blocking(move ||jynx_context::compact_on_request(&messages,limit)).await?;
            Ok(json!({"messages":result.messages,"compacted":result.compacted,"before":before,"used":jynx_context::estimate_tokens(&result.messages),"limit":limit}))
        }.await,
        "thread/import" => async {
            let source = request.params["source"].as_str().ok_or_else(|| anyhow::anyhow!("missing source"))?;
            store().import_legacy(source).await
        }.await,
        "thread/list" => state_list(request.params).await,
        "thread/save" => state_save(request.params).await,
        "thread/feedback" => state_feedback(request.params).await,
        "thread/read" | "thread/resume" => state_read(request.params).await,
        "thread/turns/list" => state_turns(request.params).await,
        "thread/archive" => state_archive(request.params, true).await,
        "thread/unarchive" => state_archive(request.params, false).await,
        "thread/name/set" => state_rename(request.params).await,
        "thread/fork" => state_fork(request.params).await,
        "thread/delete" => state_delete(request.params).await,
        "runtime/state/get" => runtime_state_get().await,
        "runtime/state/update" => runtime_state_update(request.params).await,
        "runtime/state/clear" => runtime_state_clear().await,
        _ => Err(anyhow::anyhow!("unknown method: {}", request.method)),
    };
    match result {
        Ok(value) => RpcResponse {
            id: request.id,
            result: Some(value),
            error: None,
        },
        Err(error) => RpcResponse {
            id: request.id,
            result: None,
            error: Some(RpcError {
                code: -32000,
                message: error.to_string(),
            }),
        },
    }
}

async fn run_agent(
    input: AgentRequest,
    events: tokio::sync::mpsc::UnboundedSender<Value>,
    approval_gate: ApprovalGate,
    control: AgentControl,
) -> Result<Value> {
    let provider = input.provider.clone();
    let endpoint = provider
        .as_ref()
        .map(|value| value.endpoint.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            std::env::var("JYNX_MODEL_ENDPOINT")
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
        .unwrap_or_else(|| "http://127.0.0.1:8080".into());
    let model = provider
        .as_ref()
        .map(|value| value.model.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            std::env::var("JYNX_MODEL_NAME")
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
        .unwrap_or_else(|| {
            if input.model_path.trim().is_empty() {
                "local".into()
            } else {
                input.model_path.clone()
            }
        });
    let local_compat = provider
        .as_ref()
        .map(|value| value.local_compat)
        .unwrap_or(true);
    let api_key = provider
        .and_then(|value| value.api_key)
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            std::env::var("JYNX_MODEL_API_KEY")
                .ok()
                .filter(|value| !value.trim().is_empty())
        });
    let agent = Agent::new(
        endpoint,
        model,
        api_key,
        input.cwd,
        input.context_size,
        input.settings,
        local_compat,
        approval_gate,
    )?.with_control(control);
    Ok(serde_json::to_value(
        agent.run(input.messages, events).await?,
    )?)
}

fn store() -> ChatStore {
    ChatStore::new(jynx_storage::default_history_root())
}
fn thread_id(params: &Value) -> Result<&str> {
    params["threadId"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing threadId"))
}

async fn state_list(params: Value) -> Result<Value> {
    store().import_known_legacy(std::env::current_dir()?).await?;
    let archived=params["archived"].as_bool().unwrap_or(false);
    let search=params["searchTerm"].as_str();
    if params["summaryOnly"].as_bool().unwrap_or(false) { store().list_summaries(archived,search).await }
    else { store().list(archived,search).await }
}
async fn state_save(params: Value) -> Result<Value> {
    let thread = params
        .get("thread")
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("missing thread"))?;
    store().save(thread).await
}
async fn state_feedback(params:Value)->Result<Value>{
    let id=thread_id(&params)?;
    let message_id=params["messageId"].as_str().ok_or_else(||anyhow::anyhow!("missing messageId"))?;
    if !params["feedback"].is_null()&&!params["feedback"].is_string(){return Err(anyhow::anyhow!("invalid message rating"));}
    store().set_message_feedback(id,message_id,params["feedback"].as_str(),params["updatedAt"].as_u64().unwrap_or(0)).await
}
async fn state_read(params: Value) -> Result<Value> {
    store().read(thread_id(&params)?).await
}
async fn state_turns(params: Value) -> Result<Value> {
    store().turns(thread_id(&params)?).await
}
async fn state_archive(params: Value, archived: bool) -> Result<Value> {
    store().set_archived(thread_id(&params)?, archived).await
}
async fn state_rename(params: Value) -> Result<Value> {
    let id = thread_id(&params)?.to_string();
    let name = params["name"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing name"))?
        .to_string();
    store().rename(&id, &name).await
}
async fn state_delete(params: Value) -> Result<Value> {
    store().delete(thread_id(&params)?).await
}
async fn state_fork(params: Value) -> Result<Value> {
    store().fork(thread_id(&params)?).await
}

fn runtime_store() -> RuntimeStateStore {
    RuntimeStateStore::new(default_state_root())
}
async fn runtime_state_get() -> Result<Value> {
    let state = runtime_store().read().await?;
    Ok(json!({"state":state}))
}
async fn runtime_state_update(params: Value) -> Result<Value> {
    let patch = params.get("patch").cloned().unwrap_or_else(|| json!({}));
    runtime_store().update(patch).await
}
async fn runtime_state_clear() -> Result<Value> {
    runtime_store().clear().await
}

#[cfg(test)] mod context_rpc_tests {
 use super::*;
 #[tokio::test] async fn manual_compaction_uses_supplied_messages_without_loading_chats(){
  let message=|role:&str,content:String|json!({"role":role,"content":content});
  let response=handle(RpcRequest{id:json!("compact-fixture"),method:"context/compact".into(),params:json!({"context_size":8000,"messages":[message("user","Synthetic old question".into()),message("assistant","Synthetic older content ".repeat(2000)),message("user","Remember the current request".into())]})}).await;
  assert!(response.error.is_none());let result=response.result.unwrap();assert_eq!(result["compacted"],true);assert!(result["used"].as_u64().unwrap()<result["before"].as_u64().unwrap());assert!(result["messages"].to_string().contains("Remember the current request"));
 }
 #[tokio::test] async fn short_context_reports_noop_instead_of_fake_success(){
  let response=handle(RpcRequest{id:json!("short-fixture"),method:"context/compact".into(),params:json!({"messages":[{"role":"user","content":"Hello"}]})}).await;assert!(response.error.is_none());assert_eq!(response.result.unwrap()["compacted"],false);
 }
}
