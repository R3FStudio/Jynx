#[path="terminal_graphics.rs"]
mod terminal_graphics;
use anyhow::{Context, Result, anyhow};
use jynx_agent::{Agent, ApprovalGate};
use jynx_mcp::{
    add_server as add_mcp_server, ensure_config as ensure_mcp_config,
    read_config as read_mcp_config,
};
use jynx_protocol::{AgentSettings, ChatMessage};
use jynx_storage::{ChatStore, RuntimeStateStore, default_state_root};
use serde_json::Value;
use std::io::{self as stdio, IsTerminal, Write};
use std::{collections::VecDeque, process::Stdio, sync::Arc};
use tokio::sync::mpsc::unbounded_channel;
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::{Child, Command},
    sync::Mutex,
    time::{Duration, sleep},
};

#[derive(Debug, Clone)]
struct ChatOptions {
    endpoint: String,
    model: String,
    api_key: Option<String>,
    cwd: String,
    context_size: usize,
    local_compat: bool,
    settings: AgentSettings,
    prompt: Option<String>,
    llama_binary: Option<String>,
    runtime_name: Option<String>,
    gguf: Option<String>,
    gpu_layers: String,
    llama_extra: Vec<String>,
    runtime_verbosity: u8,
}

struct CliRuntime {
    child: Child,
    key: String,
    logs: Arc<Mutex<VecDeque<String>>>,
}

pub async fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if matches!(args.first().map(String::as_str), Some("--version" | "-V" | "version")) {
        println!("Jynx CLI {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if (args.is_empty() || args.first().map(String::as_str) == Some("chat")) && crate::updater::startup().await {
        return Ok(());
    }
    if args.first().map(String::as_str) == Some("serve") {
        return jynx_server::serve_stdio().await;
    }
    if args.is_empty() {
        return chat_command(&[]).await;
    }
    let state_root = default_state_root();
    let store = ChatStore::new(&state_root);
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
    println!("  --gguf <path>          Load a GGUF with the installed or explicitly selected llama.cpp runtime");
    println!("  --runtime <name>       Select an installed backend (default: cpu)");
    println!("  --gpu-layers <n|all>  llama.cpp GPU offload (default: all)");
    println!("  --llama-arg <value>   Pass one additional llama-server argument; repeat as needed");
    println!("  --runtime-verbosity <0-5>  llama.cpp log detail (default: 3)");
    println!("  --local                 Enable strict llama.cpp local compatibility (default)");
    println!("  --cloud                 Preserve cloud provider message ordering");
    println!("  --permission <mode>     :ask, :workspace, or :danger-full-access");
    println!("  --reasoning <effort>    Model-advertised native value, or default");
    println!("  --no-web                Disable web_search/web_fetch tools");
    println!("  --no-fetch              Keep web_search but disable web_fetch");
    println!(
        "If no prompt is supplied, chat starts an interactive REPL. Tool calls are always expanded."
    );
    println!("In the interactive chat, use /settings for model, llama.cpp, and MCP menus.");
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
    let mut endpoint =
        std::env::var("JYNX_MODEL_ENDPOINT").unwrap_or_else(|_| "http://127.0.0.1:8080".into());
    let mut model = std::env::var("JYNX_MODEL_NAME").unwrap_or_else(|_| "local-model".into());
    let mut api_key = std::env::var("JYNX_MODEL_API_KEY")
        .ok()
        .filter(|value| !value.trim().is_empty());
    let mut cwd = std::env::current_dir()?.to_string_lossy().into_owned();
    let mut context_size = 32768usize;
    let mut local_compat = true;
    let mut settings = AgentSettings::default();
    let mut llama_binary = None;
    let mut runtime_name = None;
    let mut gguf = None;
    let mut gpu_layers = "all".to_string();
    let mut llama_extra = Vec::new();
    let mut runtime_verbosity = 3u8;
    settings.permission_mode = ":ask".into();
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
            "--endpoint" => endpoint = take_value(&mut index, "--endpoint")?,
            "--model" => model = take_value(&mut index, "--model")?,
            "--api-key" => {
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
            "--runtime" => runtime_name = Some(take_value(&mut index, "--runtime")?),
            "--llama-server" => llama_binary = Some(take_value(&mut index, "--llama-server")?),
            "--gguf" => gguf = Some(take_value(&mut index, "--gguf")?),
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
            "--cloud" => local_compat = false,
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
    settings.reasoning_effort=settings.reasoning_effort.trim().to_owned();
    if settings.reasoning_effort.is_empty(){return Err(anyhow!("--reasoning requires a native effort value or default"));}
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
    Ok(ChatOptions {
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
    })
}

async fn chat_command(args: &[String]) -> Result<()> {
    let mut options = parse_chat_options(args)?;
    let mut cli_runtime: Option<CliRuntime> = None;
    let mut history: Vec<ChatMessage> = Vec::new();
    if let Some(prompt) = options.prompt.clone() {
        ensure_cli_runtime(&mut options, &mut cli_runtime).await?;
        history.push(user_message(prompt));
        let response = run_chat_turn(&options, history.clone()).await?;
        if !response.text.trim().is_empty() {
            println!();
        }
        return Ok(());
    }

    let route_label =
        if options.local_compat {
            "local"
        } else {
            "cloud"
        };
    println!("\n╭─ Jynx CLI ─────────────────────────────────────");
    println!("│ model      {} ({})", options.model, route_label);
    println!("│ workspace  {}", options.cwd);
    if options.llama_binary.is_some() {
        println!("│ runtime    managed llama.cpp");
    }
    println!("╰─ /settings for menus · /help for commands · /exit to quit ─────────────\n");
    loop {
        print!("› ");
        stdio::stdout().flush()?;
        let mut line = String::new();
        if stdio::stdin().read_line(&mut line)? == 0 {
            break;
        }
        let prompt = line.trim();
        if prompt.is_empty() {
            continue;
        }
        if matches!(prompt, "/exit" | "/quit") {
            break;
        }
        if prompt == "/help" {
            println!("/help    Show CLI commands");
            println!("/status  Show the active model, endpoint, workspace, and permission mode");
            println!("/clear   Start a fresh conversation without changing the model");
            println!("/compact Compact the current model context without clearing the conversation");
            println!("/settings  Configure model, endpoint, context, permissions, and MCP servers");
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
            let result=jynx_context::compact_on_request(&history,options.context_size);
            if result.compacted {history=result.messages;println!("Conversation compacted.\n");} else {println!("Compaction not needed.\n");}
            continue;
        }
        if prompt == "/clear" {
            history.clear();
            println!("Conversation cleared.\n");
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
        if matches!(prompt, "/settings" | "/config") {
            settings_menu(&mut options).await?;
            continue;
        }
        history.push(user_message(prompt.to_string()));
        println!("\nJynx ›");
        if let Err(error) = ensure_cli_runtime(&mut options, &mut cli_runtime).await {
            eprintln!("\nruntime error: {error}\n");
            history.pop();
            continue;
        }
        match run_chat_turn(&options, history.clone()).await {
            Ok(response) => {
                if let Some(context)=response.context { history=context; }
                else if !response.text.trim().is_empty() {
                    history.push(ChatMessage {
                        role: "assistant".into(),
                        content: response.text,
                        images: Vec::new(),
                        tool_calls: None,
                        tool_call_id: None,
                        name: None,
                    });
                }
                println!();
            }
            Err(error) => {
                eprintln!("\nerror: {error}\n");
            }
        }
    }
    Ok(())
}

fn user_message(content: String) -> ChatMessage {
    ChatMessage {
        role: "user".into(),
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

async fn settings_menu(options: &mut ChatOptions) -> Result<()> {
    let alternate = stdio::stdout().is_terminal();
    if alternate {
        print!("\x1b[?1049h");
    }
    loop {
        if alternate {
            print!("\x1b[2J\x1b[H");
        }
        println!("╭─ Jynx CLI / Settings ──────────────────────────");
        println!("│ 1  Model         {}", options.model);
        println!("│ 2  Endpoint      {}", options.endpoint);
        println!("│ 3  Context       {} tokens", options.context_size);
        println!("│ 4  Reasoning     {}", options.settings.reasoning_effort);
        println!("│ 5  Permission    {}", options.settings.permission_mode);
        println!(
            "│ 6  Compatibility {}",
            if options.local_compat {
                "local llama.cpp"
            } else {
                "cloud"
            }
        );
        println!(
            "│ 7  Web tools     {}",
            if options.settings.web_search_enabled {
                "on"
            } else {
                "off"
            }
        );
        println!("│ 8  MCP servers   open configuration details");
        println!("│ 9  llama.cpp     bundle, GGUF, GPU layers, and logs");
        println!("╰─ 0  Back to chat ──────────────────────────────────");
        match menu_input("Select › ")?.as_str() {
            "0" | "" | "back" => break,
            "1" => {
                let value = menu_input("Model name › ")?;
                if !value.is_empty() {
                    options.model = value;
                }
            }
            "2" => {
                let value = menu_input("OpenAI-compatible endpoint URL › ")?;
                if !value.is_empty() {
                    options.endpoint = value;
                    options.llama_binary = None;
                }
            }
            "3" => {
                let value = menu_input("Context tokens › ")?;
                if !value.is_empty() {
                    match value.parse::<usize>() {
                        Ok(n) if n >= 512 => options.context_size = n,
                        _ => {
                            println!("Enter at least 512 tokens.");
                            let _ = menu_input("Press Enter…")?;
                        }
                    }
                }
            }
            "4" => {
                let value=menu_input("Native effort value (or default) > ")?;
                if !value.trim().is_empty(){options.settings.reasoning_effort=value;}
            }
            "5" => {
                let value = menu_input("Permission (:ask/:workspace/:danger-full-access) › ")?;
                if matches!(
                    value.as_str(),
                    ":ask" | ":workspace" | ":danger-full-access"
                ) {
                    options.settings.permission_mode = value;
                }
            }
            "6" => options.local_compat = !options.local_compat,
            "7" => {
                options.settings.web_search_enabled = !options.settings.web_search_enabled;
                options.settings.browser_tool_enabled = options.settings.web_search_enabled;
            }
            "8" => mcp_menu(alternate).await?,
            "9" => loop {
                if alternate {
                    print!("\x1b[2J\x1b[H");
                }
                println!("╭─ llama.cpp ────────────────────────────────────────");
                println!(
                    "│ 1  Server executable  {}",
                    options.llama_binary.as_deref().unwrap_or("Not set")
                );
                println!(
                    "│ 2  GGUF model         {}",
                    options.gguf.as_deref().unwrap_or("Not set")
                );
                println!("│ 3  GPU layers         {}", options.gpu_layers);
                println!(
                    "│ 4  Log detail         {} (0–5)",
                    options.runtime_verbosity
                );
                println!(
                    "│ 5  Extra args         {}",
                    if options.llama_extra.is_empty() {
                        "None".to_string()
                    } else {
                        options.llama_extra.join(" ")
                    }
                );
                println!("│ 6  Clear extra args");
                println!("╰─ 0  Back ──────────────────────────────────────────");
                match menu_input("Select › ")?.as_str() {
                    "0" | "" => break,
                    "1" => {
                        let value = menu_input("Path to llama-server executable › ")?;
                        if !value.is_empty() {
                            options.llama_binary = Some(value);
                        }
                    }
                    "2" => {
                        let value = menu_input("Path to GGUF model › ")?;
                        if !value.is_empty() {
                            options.gguf = Some(value);
                        }
                    }
                    "3" => {
                        let value = menu_input("GPU layers (all or number) › ")?;
                        if value == "all" || value.parse::<u32>().is_ok() {
                            options.gpu_layers = value;
                        }
                    }
                    "4" => {
                        let value = menu_input("Log verbosity (0–5) › ")?;
                        if let Ok(level) = value.parse::<u8>() {
                            if level <= 5 {
                                options.runtime_verbosity = level;
                            }
                        }
                    }
                    "5" => {
                        let value = menu_input("One argument (repeat to add more) › ")?;
                        if !value.is_empty() {
                            options.llama_extra.push(value);
                        }
                    }
                    "6" => options.llama_extra.clear(),
                    _ => {}
                }
            },
            _ => {}
        }
    }
    if alternate {
        print!("\x1b[?1049l");
        stdio::stdout().flush()?;
    }
    Ok(())
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
        match menu_input("Select › ")?.as_str() {
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

async fn ensure_cli_runtime(
    options: &mut ChatOptions,
    runtime: &mut Option<CliRuntime>,
) -> Result<()> {
    if options.llama_binary.is_none() && options.gguf.is_some() {
        let bundled = crate::installed_runtime::resolve(&std::env::current_exe()?, options.runtime_name.as_deref())?;
        if bundled.is_some() && options.runtime_name.as_deref().unwrap_or("cpu") == "cpu" {
            options.gpu_layers = "0".to_owned();
        }
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
    let key = format!(
        "{}|{}|{}|{}|{}|{:?}",
        binary.display(),
        gguf.display(),
        options.context_size,
        options.gpu_layers,
        options.runtime_verbosity,
        options.llama_extra
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
            &options.gpu_layers,
            "--log-verbosity",
            &options.runtime_verbosity.to_string(),
        ])
        .args(&options.llama_extra)
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

struct CliTurnResult {text:String,context:Option<Vec<ChatMessage>>}

async fn run_chat_turn(options: &ChatOptions, messages: Vec<ChatMessage>) -> Result<CliTurnResult> {
    let approval_gate = ApprovalGate::default();
    let mut settings=options.settings.clone();settings.final_answer_presentation=true;
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
    let mut text_open = CliTextState::default();
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
                    return Ok(CliTurnResult{text:response.tools.iter().filter(|tool|tool.kind=="ask_user").filter_map(|tool|tool.output.as_deref()).collect::<Vec<_>>().join("\n"),context:response.context_messages});
                }
                return Ok(CliTurnResult{text:response.text,context:response.context_messages});
            }
        }
    }
}

fn prompt_cli_approval(label: &str) -> Result<bool> {
    print!("? Approve {label}? [y/N] ");
    stdio::stdout().flush()?;
    let mut answer = String::new();
    if stdio::stdin().read_line(&mut answer)? == 0 {
        return Ok(false);
    }
    Ok(matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

#[derive(Default)]
struct CliTextState {
    open: bool,
    pending: String,
}

fn print_cli_event(event: &Value, text_open: &mut CliTextState) -> Result<()> {
    match event["type"].as_str().unwrap_or_default() {
        "final_answer" => {
            let formatted=terminal_graphics::format_response(event["text"].as_str().unwrap_or_default());
            return print_cli_event(&serde_json::json!({"type":"text_delta","text":formatted}),text_open);
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
                    if stdio::stdout().is_terminal() {
                        std::thread::sleep(std::time::Duration::from_millis(12));
                    }
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
                } else if event["status"].as_str()==Some("paused") {
                    "Compaction paused"
                } else {
                    "Conversation compacted"
                }
            );
        }
        "tool_started" => {
            finish_text_line(text_open);
            if event["kind"]=="ask_user" { return Ok(()); }
            let kind = event["kind"].as_str().unwrap_or("tool");
            let label = event["label"].as_str().unwrap_or(kind);
            println!("{} {}  ▾", cli_tool_icon(kind), label);
            if let Some(arguments) = event.get("arguments") {
                print_expanded_value("arguments", arguments);
            }
        }
        "tool_progress" => {
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
            if event["kind"]=="ask_user" && event["status"]=="completed" {
                if let Some(output)=event["output"].as_str().and_then(|value|serde_json::from_str::<Value>(value).ok()) {
                    if let Some(questions)=output["questions"].as_array() {
                        for question in questions {
                            println!("? {}",question["question"].as_str().unwrap_or_default());
                            if let Some(options)=question["options"].as_array() {for (index,option) in options.iter().enumerate(){println!("  {}. {}",index+1,option.as_str().unwrap_or_default());}}
                        }
                        println!("Answer in the next message of this conversation.");
                    }
                }
                return Ok(());
            }
            let kind = event["kind"].as_str().unwrap_or("tool");
            let label = event["label"].as_str().unwrap_or(kind);
            let failed = event["status"].as_str() == Some("failed");
            println!("{} {}", if failed { "✕" } else { "✓" }, label);
            if let Some(output) = event["output"]
                .as_str()
                .filter(|value| !value.trim().is_empty())
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
