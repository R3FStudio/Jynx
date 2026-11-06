#[path = "history.rs"]
mod history;
#[path = "discovery.rs"]
mod discovery;
#[path = "menus.rs"]
mod menus;
#[path = "preferences.rs"]
mod preferences;
#[path = "terminal_graphics.rs"]
mod terminal_graphics;
use crate::terminal_ui as ui;
use anyhow::{Context, Result, anyhow};
use jynx_agent::{Agent, ApprovalGate};
use jynx_mcp::{
    add_server as add_mcp_server, ensure_config as ensure_mcp_config,
    read_config as read_mcp_config,
};
use jynx_protocol::{AgentSettings, ChatMessage, ToolEvent};
use jynx_storage::{ChatStore, RuntimeStateStore, default_state_root, default_history_root};
use serde_json::Value;
use std::io::{self as stdio, Write};
use std::{collections::VecDeque, process::Stdio, sync::Arc};
use tokio::sync::mpsc::unbounded_channel;
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::{Child, Command},
    sync::Mutex,
    time::{Duration, sleep},
};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(default)]
struct ChatOptions {
    endpoint: String,
    model: String,
    #[serde(skip)]
    api_key: Option<String>,
    #[serde(skip)]
    cwd: String,
    context_size: usize,
    local_compat: bool,
    settings: AgentSettings,
    #[serde(skip)]
    prompt: Option<String>,
    llama_binary: Option<String>,
    runtime_name: Option<String>,
    gguf: Option<String>,
    gpu_layers: String,
    llama_extra: Vec<String>,
    runtime_verbosity: u8,
    flash_attention: String,
    cache_k: String,
    cache_v: String,
    threads: u32,
    batch_size: u32,
    ubatch_size: u32,
}

struct CliRuntime {
    child: Child,
    key: String,
    logs: Arc<Mutex<VecDeque<String>>>,
}

pub async fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if matches!(
        args.first().map(String::as_str),
        Some("--version" | "-V" | "version")
    ) {
        println!("Jynx CLI {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if (args.is_empty() || args.first().map(String::as_str) == Some("chat"))
        && crate::updater::startup().await
    {
        return Ok(());
    }
    if args.first().map(String::as_str) == Some("serve") {
        return jynx_server::serve_stdio().await;
    }
    if args.is_empty() {
        return chat_command(&[]).await;
    }
    if matches!(
        args.first().map(String::as_str),
        Some("settings" | "models")
    ) {
        let mut options = parse_chat_options(&[])?;
        if args[0] == "models" {
            menus::models(&mut options)?;
        } else {
            menus::settings(&mut options).await?;
        }
        preferences::save(&options)?;
        println!(
            "Settings saved to {}",
            preferences::home().join("config.toml").display()
        );
        return Ok(());
    }
    let state_root = default_state_root();
    let store = ChatStore::new(default_history_root());
    store.import_legacy(&state_root).await?;
    store.import_known_legacy(std::env::current_dir()?).await?;
    let runtime_store = RuntimeStateStore::new(&state_root);
    match args.first().map(String::as_str) {
        Some("chat") => return chat_command(&args[1..]).await,
        Some("threads") => match args.get(1).map(String::as_str) {
            Some("list") => print_json(
                store
                    .list(args.iter().any(|value| value == "--archived"), None)
                    .await?,
            ),
            Some("show") => print_json(
                store
                    .read(
                        args.get(2)
                            .ok_or_else(|| anyhow!("usage: jynx-cli threads show <id>"))?,
                    )
                    .await?,
            ),
            Some("turns") => print_json(
                store
                    .turns(
                        args.get(2)
                            .ok_or_else(|| anyhow!("usage: jynx-cli threads turns <id>"))?,
                    )
                    .await?,
            ),
            Some("delete") => print_json(
                store
                    .delete(
                        args.get(2)
                            .ok_or_else(|| anyhow!("usage: jynx-cli threads delete <id>"))?,
                    )
                    .await?,
            ),
            Some("archive") => print_json(
                store
                    .set_archived(
                        args.get(2)
                            .ok_or_else(|| anyhow!("usage: jynx-cli threads archive <id>"))?,
                        true,
                    )
                    .await?,
            ),
            Some("unarchive") => print_json(
                store
                    .set_archived(
                        args.get(2)
                            .ok_or_else(|| anyhow!("usage: jynx-cli threads unarchive <id>"))?,
                        false,
                    )
                    .await?,
            ),
            Some("rename") => {
                let id = args
                    .get(2)
                    .ok_or_else(|| anyhow!("usage: jynx-cli threads rename <id> <name>"))?;
                let name = args.get(3..).unwrap_or_default().join(" ");
                if name.trim().is_empty() {
                    return Err(anyhow!("thread name cannot be empty"));
                }
                print_json(store.rename(id, &name).await?);
            }
            _ => {
                return Err(anyhow!(
                    "usage: jynx-cli threads <list|show|turns|delete|archive|unarchive|rename>"
                ));
            }
        },
        Some("history") if args.get(1).map(String::as_str) == Some("export") => {
            print_json(store.list(false, None).await?);
        }
        Some("state") => match args.get(1).map(String::as_str) {
            Some("show") => print_json(runtime_store.read().await?),
            Some("clear") => print_json(runtime_store.clear().await?),
            _ => return Err(anyhow!("usage: jynx-cli state <show|clear>")),
        },
        Some("help") | Some("--help") | Some("-h") => print_help(),
        _ => return Err(anyhow!("unknown command; run jynx-cli help")),
    }
    Ok(())
}

fn print_help() {
    println!("Jynx CLI {}", env!("CARGO_PKG_VERSION"));
    println!("jynx-cli --version             Print the installed version");
    println!("jynx-cli                       Start an interactive chat");
    println!("jynx-cli chat [options] [prompt]");
    println!("jynx-cli settings              Open the settings menu");
    println!("jynx-cli models                Choose a local model or cloud route");
    println!("jynx-cli serve                 Start the JSON-RPC server for host applications");
    println!(
        "  --endpoint <url>        OpenAI-compatible endpoint (default: http://127.0.0.1:8080)"
    );
    println!("  --model <name>          Model name/id (default: local-model)");
    println!(
        "  --api-key <key>         Use a cloud/API-key route; JYNX_MODEL_API_KEY is also supported"
    );
    println!("  --cwd <path>            Workspace exposed to file/code tools");
    println!("  --context <tokens>      Context budget (default: 32768)");
    println!("  --llama-server <path>  Launch this llama.cpp bundle's server executable");
    println!(
        "  --gguf <path>          Load a GGUF with the installed or explicitly selected llama.cpp runtime"
    );
    println!("  --runtime <name>       Select an installed backend (default: cpu)");
    println!("  --gpu-layers <n|all>  llama.cpp GPU offload (default: all)");
    println!("  --llama-arg <value>   Pass one additional llama-server argument; repeat as needed");
    println!("  --runtime-verbosity <0-5>  llama.cpp log detail (default: 3)");
    println!("  --flash-attn <auto|on|off>  Flash attention mode");
    println!("  --cache-type-k <type> --cache-type-v <type>  KV cache types (default: f16)");
    println!("  --threads <n> --batch-size <n> --ubatch-size <n>  Runtime tuning");
    println!("  --local                 Enable strict llama.cpp local compatibility (default)");
    println!("  --cloud                 Preserve cloud provider message ordering");
    println!("  --permission <mode>     :ask, :workspace, or :danger-full-access");
    println!("  --reasoning <effort>    Model-advertised native value, or default");
    println!("  --no-web                Disable web_search/web_fetch tools");
    println!("  --no-fetch              Keep web_search but disable web_fetch");
    println!(
        "If no prompt is supplied, chat starts an interactive REPL. Piped output includes expanded tool results."
    );
    println!("F2 opens actions, F3 models, F4 permissions. Type / for a searchable command menu.");
    println!("Settings are stored in ~/.jynx/config.toml (or JYNX_HOME/config.toml).");
    println!();
    println!("jynx-cli threads list [--archived]");
    println!("jynx-cli threads show <id>");
    println!("jynx-cli threads turns <id>");
    println!("jynx-cli threads rename <id> <name>");
    println!("jynx-cli threads archive <id>");
    println!("jynx-cli threads unarchive <id>");
    println!("jynx-cli threads delete <id>");
    println!("jynx-cli history export");
    println!("jynx-cli state show");
    println!("jynx-cli state clear");
}

fn parse_chat_options(args: &[String]) -> Result<ChatOptions> {
    parse_chat_options_from(args, preferences::load()?)
}

fn parse_chat_options_from(args: &[String], saved: ChatOptions) -> Result<ChatOptions> {
    let mut endpoint = std::env::var("JYNX_MODEL_ENDPOINT").unwrap_or(saved.endpoint);
    let mut model = std::env::var("JYNX_MODEL_NAME").unwrap_or(saved.model);
    let mut api_key = std::env::var("JYNX_MODEL_API_KEY")
        .ok()
        .filter(|v| !v.trim().is_empty());
    let mut cwd = std::env::current_dir()?.to_string_lossy().into_owned();
    let mut context_size = saved.context_size;
    let mut local_compat = saved.local_compat;
    let mut settings = saved.settings;
    let mut llama_binary = saved.llama_binary;
    let mut runtime_name = saved.runtime_name;
    let mut gguf = saved.gguf;
    let mut gpu_layers = saved.gpu_layers;
    let mut llama_extra = saved.llama_extra;
    let mut runtime_verbosity = saved.runtime_verbosity;
    let mut flash_attention = saved.flash_attention;
    let mut cache_k = saved.cache_k;
    let mut cache_v = saved.cache_v;
    let mut threads = saved.threads;
    let mut batch_size = saved.batch_size;
    let mut ubatch_size = saved.ubatch_size;
    if std::env::var_os("JYNX_MODEL_ENDPOINT").is_some() || api_key.is_some() {
        gguf = None;
        llama_binary = None;
        local_compat = api_key.is_none();
    }
    let mut prompt_parts = Vec::new();
    let mut index = 0usize;
    while index < args.len() {
        let arg = &args[index];
        let take_value = |index: &mut usize, label: &str| -> Result<String> {
            *index += 1;
            args.get(*index)
                .cloned()
                .ok_or_else(|| anyhow!("{label} requires a value"))
        };
        match arg.as_str() {
            "--endpoint" => {
                endpoint = take_value(&mut index, "--endpoint")?;
                gguf = None;
                llama_binary = None;
            }
            "--model" => {
                model = take_value(&mut index, "--model")?;
                gguf = None;
                llama_binary = None;
            }
            "--api-key" => {
                gguf = None;
                llama_binary = None;
                api_key = Some(take_value(&mut index, "--api-key")?);
                local_compat = false;
            }
            "--cwd" => cwd = take_value(&mut index, "--cwd")?,
            "--context" => {
                context_size = take_value(&mut index, "--context")?
                    .parse::<usize>()
                    .map_err(|_| anyhow!("--context must be a positive integer"))?;
                if context_size == 0 {
                    return Err(anyhow!("--context must be greater than zero"));
                }
            }
            "--flash-attn" => flash_attention = take_value(&mut index, "--flash-attn")?,
            "--cache-type-k" => cache_k = take_value(&mut index, "--cache-type-k")?,
            "--cache-type-v" => cache_v = take_value(&mut index, "--cache-type-v")?,
            "--threads" => {
                threads = take_value(&mut index, "--threads")?
                    .parse()
                    .context("Threads must be a nonnegative integer")?
            }
            "--batch-size" => {
                batch_size = take_value(&mut index, "--batch-size")?
                    .parse()
                    .context("Batch size must be a positive integer")?
            }
            "--ubatch-size" => {
                ubatch_size = take_value(&mut index, "--ubatch-size")?
                    .parse()
                    .context("Micro batch size must be a positive integer")?
            }
            "--runtime" => {
                runtime_name = Some(take_value(&mut index, "--runtime")?);
                llama_binary = None;
            }
            "--llama-server" => llama_binary = Some(take_value(&mut index, "--llama-server")?),
            "--gguf" => {
                gguf = Some(take_value(&mut index, "--gguf")?);
                local_compat = true;
                api_key = None;
            }
            "--gpu-layers" => gpu_layers = take_value(&mut index, "--gpu-layers")?,
            "--llama-arg" => llama_extra.push(take_value(&mut index, "--llama-arg")?),
            "--runtime-verbosity" => {
                runtime_verbosity = take_value(&mut index, "--runtime-verbosity")?
                    .parse()
                    .map_err(|_| anyhow!("verbosity must be 0–5"))?
            }
            "--permission" => {
                let mode = take_value(&mut index, "--permission")?;
                if !matches!(mode.as_str(), ":ask" | ":workspace" | ":danger-full-access") {
                    return Err(anyhow!(
                        "--permission must be :ask, :workspace, or :danger-full-access"
                    ));
                }
                settings.permission_mode = mode;
            }
            "--reasoning" => settings.reasoning_effort = take_value(&mut index, "--reasoning")?,
            "--local" => local_compat = true,
            "--cloud" => {
                local_compat = false;
                gguf = None;
                llama_binary = None;
            }
            "--no-web" => {
                settings.web_search_enabled = false;
                settings.browser_tool_enabled = false;
            }
            "--no-fetch" => settings.browser_tool_enabled = false,
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            value if value.starts_with('-') => return Err(anyhow!("unknown chat option: {value}")),
            value => prompt_parts.push(value.to_string()),
        }
        index += 1;
    }
    if api_key.is_none() && !local_compat {
        let name = provider_key_env(&endpoint);
        api_key = std::env::var(name)
            .ok()
            .filter(|key| !key.trim().is_empty());
    }
    settings.reasoning_effort = settings.reasoning_effort.trim().to_owned();
    if settings.reasoning_effort.is_empty() {
        return Err(anyhow!(
            "--reasoning requires a native effort value or default"
        ));
    }
    if runtime_verbosity > 5 {
        return Err(anyhow!("runtime verbosity must be 0–5"));
    }
    if gpu_layers != "all" && gpu_layers.parse::<u32>().is_err() {
        return Err(anyhow!("GPU layers must be all or a nonnegative integer"));
    }
    let prompt = if prompt_parts.is_empty() {
        None
    } else {
        Some(prompt_parts.join(" "))
    };
    if !["f16", "bf16", "f32"].contains(&cache_k.as_str())
        || !["f16", "bf16", "f32"].contains(&cache_v.as_str())
    {
        flash_attention = "on".into();
    }
    let options = ChatOptions {
        endpoint,
        model,
        api_key,
        cwd,
        context_size,
        local_compat,
        settings,
        prompt,
        llama_binary,
        runtime_name,
        gguf,
        gpu_layers,
        llama_extra,
        runtime_verbosity,
        flash_attention,
        cache_k,
        cache_v,
        threads,
        batch_size,
        ubatch_size,
    };
    preferences::validate(&options)?;
    Ok(options)
}

fn provider_key_env(endpoint: &str) -> &'static str {
    match reqwest::Url::parse(endpoint)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .as_deref()
    {
        Some("api.openai.com") => "OPENAI_API_KEY",
        Some("api.anthropic.com") => "ANTHROPIC_API_KEY",
        _ => "JYNX_MODEL_API_KEY",
    }
}

async fn chat_command(args: &[String]) -> Result<()> {
    let mut options = parse_chat_options(args)?;
    let discovery = discovery::Discovery::new(&preferences::home()).ok();
    let mut cli_runtime: Option<CliRuntime> = None;
    let mut history: Vec<ChatMessage> = Vec::new();
    let mut recent_tools: Vec<ToolEvent> = Vec::new();
    let mut chat_history = history::History::new(&options.cwd, &options.model);
    if let Some(prompt) = options.prompt.clone() {
        ensure_cli_runtime(&mut options, &mut cli_runtime).await?;
        publish_discovery(&discovery, &options, &cli_runtime);
        history.push(user_message(prompt.clone()));
        let response = run_chat_turn(&options, history.clone()).await?;
        let context = response.context.clone().unwrap_or_else(|| {
            let mut value = history.clone();
            value.push(ChatMessage { phase: None, role:"assistant".into(), content:response.text.clone(), images:Vec::new(), tool_calls:None, tool_call_id:None, name:None }); value
        });
        chat_history.save_turn(&prompt, &response.text, &response.tools, &context, false).await?;
        if !response.text.trim().is_empty() {
            println!();
        }
        return Ok(());
    }

    ui::banner(
        &options.model,
        &options.cwd,
        options.context_size,
        &options.settings.permission_mode,
    );
    let mut input_history = Vec::new();
    loop {
        publish_discovery(&discovery, &options, &cli_runtime);
        let Some(line) = ui::prompt(&input_history)? else {
            break;
        };
        let prompt = line.trim();
        if prompt.is_empty() {
            continue;
        }
        input_history.push(prompt.to_owned());
        if matches!(prompt, "/exit" | "/quit") {
            break;
        }
        if prompt == "/help" {
            println!(
                "F2 actions / F3 models / F4 permissions / / opens the searchable action menu"
            );
            println!("/model   Local models and cloud endpoints");
            println!("/permissions  Approval mode / /context  Context window");
            println!("/help    Show CLI commands");
            println!("/status  Show the active model, endpoint, workspace, and permission mode");
            println!("/history Open shared Jynx conversations, including archived chats");
            println!("/clear   Start a fresh conversation without changing the model");
            println!(
                "/compact Compact the current model context without clearing the conversation"
            );
            println!("/settings  Configure model, endpoint, context, permissions, and MCP servers");
            println!("/tools   Inspect the last completed turn's tool results");
            println!("/logs    Show recent llama.cpp runtime output");
            println!("/exit    Close Jynx CLI\n");
            continue;
        }
        if prompt == "/status" {
            println!("Model: {} · Endpoint: {}", options.model, options.endpoint);
            println!(
                "Workspace: {} · Permission: {}",
                options.cwd, options.settings.permission_mode
            );
            println!("Conversation: {} messages\n", history.len());
            continue;
        }
        if prompt == "/compact" {
            println!("Compacting conversation...");
            let result = jynx_context::compact_on_request(&history, options.context_size);
            if result.compacted {
                history = result.messages;
                println!("Conversation compacted.\n");
            } else {
                println!("Compaction not needed.\n");
            }
            continue;
        }
        if prompt == "/history" {
            match history::History::select(&options.cwd).await {
                Ok(Some((saved, messages))) => { chat_history = saved; history = messages; println!("Conversation opened.\n"); }
                Ok(None) => {},
                Err(error) => eprintln!("Could not open history: {error}"),
            }
            continue;
        }
        if prompt == "/clear" {
            history.clear();
            chat_history = history::History::new(&options.cwd, &options.model);
            println!("Conversation cleared.\n");
            continue;
        }
        if prompt == "/tools" {
            menus::tool_results(&recent_tools)?;
            continue;
        }
        if prompt == "/logs" {
            if let Some(runtime) = &cli_runtime {
                for line in runtime.logs.lock().await.iter() {
                    println!("{line}");
                }
            } else {
                println!("No managed llama.cpp runtime has been started.");
            }
            continue;
        }
        if matches!(
            prompt,
            "/settings" | "/config" | "/model" | "/permissions" | "/context"
        ) {
            let previous = options.clone();
            let result = match prompt {
                "/model" => menus::models(&mut options),
                "/permissions" => menus::permissions(&mut options),
                "/context" => menus::context(&mut options),
                _ => menus::settings(&mut options).await,
            };
            if let Err(error) = result.and_then(|_| preferences::save(&options)) {
                options = previous;
                eprintln!("Settings were not saved: {error:#}");
            } else {
                println!(
                    "Settings saved to {}\n",
                    preferences::home().join("config.toml").display()
                );
            }
            continue;
        }
        history.push(user_message(prompt.to_string()));
        println!("\nJynx ›");
        if let Err(error) = ensure_cli_runtime(&mut options, &mut cli_runtime).await {
            eprintln!("\nruntime error: {error}\n");
            history.pop();
            continue;
        }
        publish_discovery(&discovery, &options, &cli_runtime);
        match run_chat_turn(&options, history.clone()).await {
            Ok(response) => {
                let answer = response.text.clone();
                recent_tools = response.tools;
                if let Some(context) = response.context {
                    history = context;
                } else if !response.text.trim().is_empty() {
                    history.push(ChatMessage {
                        phase: None, role: "assistant".into(),
                        content: response.text,
                        images: Vec::new(),
                        tool_calls: None,
                        tool_call_id: None,
                        name: None,
                    });
                }
                if let Err(error) = chat_history.save_turn(prompt, &answer, &recent_tools, &history, false).await {
                    eprintln!("Could not save conversation: {error}");
                }
                println!();
            }
            Err(error) => {
                if let Err(save_error) = chat_history.save_turn(prompt, "", &[], &history, true).await { eprintln!("Could not save conversation: {save_error}"); }
                eprintln!("\nerror: {error}\n");
            }
        }
    }
    Ok(())
}

fn user_message(content: String) -> ChatMessage {
    ChatMessage {
        phase: None, role: "user".into(),
        content,
        images: Vec::new(),
        tool_calls: None,
        tool_call_id: None,
        name: None,
    }
}

fn menu_input(prompt: &str) -> Result<String> {
    print!("{prompt}");
    stdio::stdout().flush()?;
    let mut answer = String::new();
    stdio::stdin().read_line(&mut answer)?;
    Ok(answer.trim().to_string())
}

async fn mcp_menu(alternate: bool) -> Result<()> {
    let path = ensure_mcp_config().await?;
    loop {
        if alternate {
            print!("\x1b[2J\x1b[H");
        }
        println!("╭─ MCP servers ──────────────────────────────────────");
        println!("│ Configuration: {}", path.display());
        match read_mcp_config().await {
            Ok(config) if config.servers.is_empty() => println!("│ No local servers configured"),
            Ok(config) => {
                let mut servers: Vec<_> = config.servers.into_iter().collect();
                servers.sort_by(|left, right| left.0.cmp(&right.0));
                for (name, server) in servers {
                    println!(
                        "│ {}  {} · {}",
                        if server.enabled { "●" } else { "○" },
                        name,
                        server.command
                    );
                }
            }
            Err(error) => println!("│ Configuration error: {error}"),
        }
        println!("│ 1  Add local stdio server");
        println!("│ Advanced: edit mcp.json to set environment variables or disable servers.");
        println!("╰─ 0  Back ──────────────────────────────────────────");
        match ui::choose(
            "MCP servers",
            &["Add a local stdio server".into(), "Back".into()],
            0,
        )?
        .map(|i| if i == 0 { "1" } else { "0" })
        .unwrap_or("0")
        {
            "0" | "" => break,
            "1" => {
                println!(
                    "Only add trusted programs. MCP servers run when the agent begins a turn."
                );
                let name = menu_input("Server name › ")?;
                let command = menu_input("Executable or command › ")?;
                let raw_args = menu_input("Arguments as JSON array (blank for none) › ")?;
                let args: Result<Vec<String>> = if raw_args.is_empty() {
                    Ok(Vec::new())
                } else {
                    serde_json::from_str(&raw_args).map_err(Into::into)
                };
                let result = match args {
                    Ok(args) => add_mcp_server(&name, &command, args).await,
                    Err(error) => Err(error),
                };
                println!(
                    "{}",
                    match result {
                        Ok(()) => "Server added. Tools are discovered on the next turn.".to_owned(),
                        Err(error) => format!("Could not add server: {error}"),
                    }
                );
                let _ = menu_input("Press Enter…")?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn publish_discovery(discovery: &Option<discovery::Discovery>, options: &ChatOptions, runtime: &Option<CliRuntime>) {
    if let Some(discovery) = discovery {
        let ready = options.api_key.is_none() && (options.gguf.is_none() || runtime.is_some());
        let endpoint = if ready { Some(options.endpoint.as_str()) } else { None };
        let _ = discovery.update(endpoint, &options.model, options.context_size);
    }
}

async fn ensure_cli_runtime(
    options: &mut ChatOptions,
    runtime: &mut Option<CliRuntime>,
) -> Result<()> {
    if options.llama_binary.is_none() && options.gguf.is_some() {
        let bundled = crate::installed_runtime::resolve(
            &std::env::current_exe()?,
            options.runtime_name.as_deref(),
        )?;
        options.llama_binary = bundled.map(|path| path.to_string_lossy().into_owned());
    }
    let (Some(binary), Some(gguf)) = (&options.llama_binary, &options.gguf) else {
        if options.llama_binary.is_some() || options.gguf.is_some() {
            return Err(anyhow!(
                "Set both llama-server and a GGUF model in /settings → llama.cpp"
            ));
        }
        if let Some(previous) = runtime.as_mut() {
            previous.child.start_kill()?;
        }
        *runtime = None;
        return Ok(());
    };
    let binary = std::fs::canonicalize(binary)
        .with_context(|| format!("llama-server executable not found: {binary}"))?;
    let gguf =
        std::fs::canonicalize(gguf).with_context(|| format!("GGUF model not found: {gguf}"))?;
    let gpu_layers = if binary
        .parent()
        .and_then(|parent| parent.file_name())
        .is_some_and(|name| name == "cpu")
    {
        "0"
    } else {
        &options.gpu_layers
    };
    let key = format!(
        "{}|{}|{}|{}|{}|{:?}",
        binary.display(),
        gguf.display(),
        options.context_size,
        gpu_layers,
        options.runtime_verbosity,
        runtime_arguments(options)
    );
    if let Some(current) = runtime.as_mut() {
        if current.key == key && current.child.try_wait()?.is_none() {
            return Ok(());
        }
        current.child.start_kill()?;
    }
    *runtime = None;
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    drop(listener);
    let directory = binary
        .parent()
        .ok_or_else(|| anyhow!("llama-server has no parent directory"))?;
    let mut command = Command::new(&binary);
    command
        .current_dir(directory)
        .args([
            "-m",
            gguf.to_string_lossy().as_ref(),
            "-c",
            &options.context_size.to_string(),
            "--host",
            "127.0.0.1",
            "--port",
            &port.to_string(),
            "--jinja",
            "-ngl",
            gpu_layers,
            "--log-verbosity",
            &options.runtime_verbosity.to_string(),
        ])
        .args(runtime_arguments(options))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    {
        command.creation_flags(0x08000000);
    }
    let existing = std::env::var_os("PATH").unwrap_or_default();
    let new_path = std::env::join_paths(
        std::iter::once(directory.to_path_buf()).chain(std::env::split_paths(&existing)),
    )?;
    command.env("PATH", new_path);
    let mut child = command
        .spawn()
        .with_context(|| format!("starting {}", binary.display()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow!("runtime stderr unavailable"))?;
    let logs = Arc::new(Mutex::new(VecDeque::<String>::new()));
    let log_sink = logs.clone();
    tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let mut queue = log_sink.lock().await;
            queue.push_back(line);
            if queue.len() > 250 {
                queue.pop_front();
            }
        }
    });
    println!(
        "⌁ Loading {} with llama.cpp…",
        gguf.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("model")
    );
    let endpoint = format!("http://127.0.0.1:{port}/v1");
    let client = reqwest::Client::new();
    let started = std::time::Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            let tail = logs
                .lock()
                .await
                .iter()
                .rev()
                .take(8)
                .cloned()
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join("\n");
            return Err(anyhow!("llama.cpp exited ({status}). {tail}"));
        }
        if client
            .get(format!("http://127.0.0.1:{port}/health"))
            .timeout(Duration::from_secs(1))
            .send()
            .await
            .is_ok_and(|response| response.status().is_success())
        {
            break;
        }
        if started.elapsed() > Duration::from_secs(180) {
            return Err(anyhow!("llama.cpp did not become ready within 180 seconds"));
        }
        sleep(Duration::from_millis(350)).await;
    }
    println!("✓ Local model ready at {endpoint}");
    options.endpoint = endpoint;
    options.model = gguf
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("local-model")
        .to_owned();
    options.local_compat = true;
    *runtime = Some(CliRuntime { child, key, logs });
    Ok(())
}

fn runtime_arguments(options: &ChatOptions) -> Vec<String> {
    let mut args = vec![
        "--flash-attn".into(),
        options.flash_attention.clone(),
        "--cache-type-k".into(),
        options.cache_k.clone(),
        "--cache-type-v".into(),
        options.cache_v.clone(),
        "--batch-size".into(),
        options.batch_size.to_string(),
        "--ubatch-size".into(),
        options.ubatch_size.to_string(),
    ];
    if options.threads > 0 {
        args.extend(["--threads".into(), options.threads.to_string()]);
    }
    args.extend(options.llama_extra.clone());
    args
}

struct CliTurnResult {
    text: String,
    context: Option<Vec<ChatMessage>>,
    tools: Vec<ToolEvent>,
}

async fn run_chat_turn(options: &ChatOptions, messages: Vec<ChatMessage>) -> Result<CliTurnResult> {
    let approval_gate = ApprovalGate::default();
    let mut settings = options.settings.clone();
    settings.final_answer_presentation = true;
    let agent = Agent::new(
        options.endpoint.clone(),
        options.model.clone(),
        options.api_key.clone(),
        options.cwd.clone(),
        options.context_size,
        settings,
        options.local_compat,
        approval_gate.clone(),
    )?;
    let (tx, mut rx) = unbounded_channel::<Value>();
    let mut task = tokio::spawn(async move { agent.run(messages, tx).await });
    let mut text_open = CliTextState {
        expanded: options.settings.auto_open_tool_details || !ui::interactive(),
        ..Default::default()
    };
    loop {
        tokio::select! {
            event = rx.recv() => {
                if let Some(event) = event {
                    if event["type"].as_str() == Some("approval_required") {
                        finish_text_line(&mut text_open);
                        let approval_id = event["approval_id"].as_str().unwrap_or_default();
                        let label = event["label"].as_str().unwrap_or("tool call");
                        let approved = prompt_cli_approval(label)?;
                        let _ = approval_gate.resolve(approval_id, approved).await;
                    } else {
                        print_cli_event(&event, &mut text_open)?;
                    }
                }
            }
            result = &mut task => {
                while let Ok(event) = rx.try_recv() {
                    print_cli_event(&event, &mut text_open)?;
                }
                let response = result.map_err(|error| anyhow!("agent task failed: {error}"))??;
                if text_open.open || !text_open.pending.is_empty() {
                    finish_text_line(&mut text_open);
                } else if !response.text.trim().is_empty() {
                    println!("{}", response.text.trim_end());
                }
                if response.awaiting_user_input {
                    return Ok(CliTurnResult{text:response.tools.iter().filter(|tool|tool.kind=="ask_user").filter_map(|tool|tool.output.as_deref()).collect::<Vec<_>>().join("\n"),context:response.context_messages,tools:response.tools});
                }
                return Ok(CliTurnResult{text:response.text,context:response.context_messages,tools:response.tools});
            }
        }
    }
}

fn prompt_cli_approval(label: &str) -> Result<bool> {
    if ui::interactive() {
        return Ok(ui::choose(
            &format!("Approve {label}?"),
            &["Decline".into(), "Approve this operation".into()],
            0,
        )? == Some(1));
    }
    print!("? Approve {label}? [y/N] ");
    stdio::stdout().flush()?;
    let mut answer = String::new();
    stdio::stdin().read_line(&mut answer)?;
    Ok(matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

#[derive(Default)]
struct CliTextState {
    open: bool,
    pending: String,
    expanded: bool,
}

fn print_cli_event(event: &Value, text_open: &mut CliTextState) -> Result<()> {
    match event["type"].as_str().unwrap_or_default() {
        "final_answer" => {
            let formatted =
                terminal_graphics::format_response(event["text"].as_str().unwrap_or_default());
            return print_cli_event(
                &serde_json::json!({"type":"text_delta","text":formatted}),
                text_open,
            );
        }
        "text_delta" => {
            let text = event["text"].as_str().unwrap_or_default();
            if !text.is_empty() {
                text_open.pending.push_str(text);
                while let Some(end) = text_open
                    .pending
                    .char_indices()
                    .find_map(|(index, ch)| ch.is_whitespace().then_some(index + ch.len_utf8()))
                {
                    let word = text_open.pending.drain(..end).collect::<String>();
                    print!("{word}");
                    stdio::stdout().flush()?;
                    text_open.open = true;
                }
                if text_open.pending.len() > 1024 {
                    print!("{}", text_open.pending);
                    text_open.pending.clear();
                    stdio::stdout().flush()?;
                    text_open.open = true;
                }
            }
        }
        "connection_status" => {
            finish_text_line(text_open);
            let state = event["state"].as_str().unwrap_or_default();
            if state != "connected" {
                println!("⌁ {}", event["message"].as_str().unwrap_or("Connecting..."));
            }
        }

        "reasoning_delta" | "status" => {}
        "context_compaction" => {
            finish_text_line(text_open);
            println!(
                "◇ {}",
                if event["status"].as_str() == Some("started") {
                    "Compacting conversation…"
                } else if event["status"].as_str() == Some("paused") {
                    "Compaction paused"
                } else {
                    "Conversation compacted"
                }
            );
        }
        "tool_started" => {
            finish_text_line(text_open);
            if event["kind"] == "ask_user" {
                return Ok(());
            }
            let kind = event["kind"].as_str().unwrap_or("tool");
            let label = event["label"].as_str().unwrap_or(kind);
            ui::tool_heading(cli_tool_icon(kind), label, false);
            if let Some(arguments) = event.get("arguments").filter(|_| text_open.expanded) {
                print_expanded_value("arguments", arguments);
            }
        }
        "tool_progress" => {
            if !text_open.expanded {
                return Ok(());
            }
            finish_text_line(text_open);
            if let Some(output) = event["output"]
                .as_str()
                .filter(|value| !value.trim().is_empty())
            {
                print_indented("progress", output);
            }
        }
        "tool_completed" => {
            finish_text_line(text_open);
            if event["kind"] == "ask_user" && event["status"] == "completed" {
                if let Some(output) = event["output"]
                    .as_str()
                    .and_then(|value| serde_json::from_str::<Value>(value).ok())
                {
                    if let Some(questions) = output["questions"].as_array() {
                        for question in questions {
                            println!("? {}", question["question"].as_str().unwrap_or_default());
                            if let Some(options) = question["options"].as_array() {
                                for (index, option) in options.iter().enumerate() {
                                    println!(
                                        "  {}. {}",
                                        index + 1,
                                        option.as_str().unwrap_or_default()
                                    );
                                }
                            }
                        }
                        println!("Answer in the next message of this conversation.");
                    }
                }
                return Ok(());
            }
            let kind = event["kind"].as_str().unwrap_or("tool");
            let label = event["label"].as_str().unwrap_or(kind);
            let failed = event["status"].as_str() == Some("failed");
            ui::tool_heading(if failed { "!" } else { "+" }, label, failed);
            if let Some(output) = event["output"]
                .as_str()
                .filter(|value| !value.trim().is_empty() && (text_open.expanded || failed))
            {
                if let Ok(value) = serde_json::from_str::<Value>(output) {
                    print_expanded_value("output", &value);
                } else {
                    print_indented("output", output);
                }
            }
            println!();
        }
        _ => {}
    }
    Ok(())
}

fn cli_tool_icon(kind: &str) -> &'static str {
    match kind {
        "read_file" => "▱",
        "write_file" => "◇",
        "edit_file" => "◇",
        "list_files" => "□",
        "search_files" => "⌕",
        "run_command" => ">_",
        "run_in_terminal_panel" => ">_",
        "open_browser" => "◉",
        "web_search" => "◎",
        "web_fetch" => "◉",
        _ => "◆",
    }
}

fn finish_text_line(text_open: &mut CliTextState) {
    if !text_open.pending.is_empty() {
        print!("{}", text_open.pending);
        text_open.pending.clear();
        text_open.open = true;
    }
    if text_open.open {
        println!("\n");
        text_open.open = false;
    }
}

fn print_expanded_value(label: &str, value: &Value) {
    let pretty = serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string());
    print_indented(label, &pretty);
}

fn print_indented(label: &str, text: &str) {
    println!("    {label}:");
    let max_chars = 24_000usize;
    let mut visible = text.to_string();
    let truncated = visible.chars().count() > max_chars;
    if truncated {
        visible = visible.chars().take(max_chars).collect::<String>();
    }
    for line in visible.lines() {
        println!("      {line}");
    }
    if truncated {
        println!("      … output truncated after {max_chars} characters");
    }
}

fn print_json(value: Value) {
    println!(
        "{}",
        serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".into())
    );
}

#[cfg(test)]
mod cli_options_tests {
    use super::*;
    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| v.to_string()).collect()
    }
    #[test]
    fn provider_credentials_match_exact_hosts() {
        assert_eq!(
            provider_key_env("https://api.openai.com/v1"),
            "OPENAI_API_KEY"
        );
        assert_eq!(
            provider_key_env("https://api.anthropic.com/v1"),
            "ANTHROPIC_API_KEY"
        );
        assert_eq!(
            provider_key_env("https://api.openai.com.example.invalid/v1"),
            "JYNX_MODEL_API_KEY"
        );
    }
    #[test]
    fn explicit_cloud_route_cannot_start_saved_local_model() {
        let mut saved = ChatOptions::default();
        saved.gguf = Some("synthetic.gguf".into());
        saved.llama_binary = Some("synthetic-server".into());
        let options = parse_chat_options_from(
            &args(&[
                "--endpoint",
                "https://example.invalid/v1",
                "--cloud",
                "--model",
                "remote",
            ]),
            saved,
        )
        .unwrap();
        assert!(options.gguf.is_none());
        assert!(options.llama_binary.is_none());
        assert!(!options.local_compat);
    }
    #[test]
    fn native_runtime_controls_reach_llama_arguments_and_enable_flash_for_quantized_cache() {
        let options = parse_chat_options_from(
            &args(&[
                "--gguf",
                "synthetic.gguf",
                "--context",
                "64000",
                "--cache-type-v",
                "q4_0",
                "--threads",
                "6",
                "--batch-size",
                "256",
                "--ubatch-size",
                "64",
            ]),
            ChatOptions::default(),
        )
        .unwrap();
        let arguments = runtime_arguments(&options);
        for pair in [
            ["--flash-attn", "on"],
            ["--cache-type-v", "q4_0"],
            ["--threads", "6"],
            ["--batch-size", "256"],
            ["--ubatch-size", "64"],
        ] {
            assert!(arguments.windows(2).any(|window| window == pair));
        }
        assert_eq!(options.context_size, 64000);
        assert!(options.local_compat);
        assert!(
            parse_chat_options_from(
                &args(&["--ubatch-size", "1024", "--batch-size", "256"]),
                ChatOptions::default()
            )
            .is_err()
        );
    }
}
