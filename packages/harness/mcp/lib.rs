

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::HashMap, path::PathBuf, process::Stdio};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::Mutex,
    time::{Duration, timeout},
};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default = "enabled_by_default")]
    pub enabled: bool,
}

fn enabled_by_default() -> bool {
    true
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct Config {
    #[serde(default, rename = "mcpServers")]
    pub servers: HashMap<String, ServerConfig>,
}

pub fn config_path() -> PathBuf {
    if let Some(path) = std::env::var_os("JYNX_MCP_CONFIG") {
        return PathBuf::from(path);
    }
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" });
    let home = home.map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    let current = home.join(".jynx").join("mcp.json");
    let legacy = home.join(".daemonstudio").join("mcp.json");
    if current.exists() || !legacy.exists() { current } else { legacy }
}

pub async fn read_config() -> Result<Config> {
    let path = config_path();
    let bytes = match tokio::fs::read(&path).await {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Config::default()),
        Err(error) => return Err(error).with_context(|| format!("Reading {}", path.display())),
    };
    if bytes.len() > 256_000 {
        return Err(anyhow!("MCP configuration exceeds 256 KB"));
    }
    let config: Config =
        serde_json::from_slice(&bytes).with_context(|| format!("Parsing {}", path.display()))?;
    if config.servers.len() > 32 {
        return Err(anyhow!("MCP configuration has too many servers"));
    }
    Ok(config)
}

pub async fn ensure_config() -> Result<PathBuf> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    if !path.exists() {
        tokio::fs::write(&path, b"{\n  \"mcpServers\": {}\n}\n").await?;
    }
    Ok(path)
}

pub async fn add_server(name: &str, command: &str, args: Vec<String>) -> Result<()> {
    if name.is_empty()
        || name.len() > 64
        || !name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'))
    {
        return Err(anyhow!(
            "MCP server name must use letters, numbers, hyphens, or underscores"
        ));
    }
    if command.trim().is_empty()
        || command.len() > 1024
        || args.len() > 32
        || args.iter().any(|arg| arg.len() > 2048)
    {
        return Err(anyhow!("MCP server command or arguments are invalid"));
    }
    let path = ensure_config().await?;
    let mut root: Value = serde_json::from_slice(&tokio::fs::read(&path).await?)?;
    let servers = root
        .as_object_mut()
        .ok_or_else(|| anyhow!("MCP configuration must be a JSON object"))?
        .entry("mcpServers")
        .or_insert_with(|| json!({}));
    let servers = servers
        .as_object_mut()
        .ok_or_else(|| anyhow!("mcpServers must be a JSON object"))?;
    if servers.contains_key(name) {
        return Err(anyhow!("An MCP server with that name already exists"));
    }
    if servers.len() >= 32 {
        return Err(anyhow!("MCP configuration has too many servers"));
    }
    servers.insert(
        name.to_string(),
        json!({"command":command,"args":args,"env":{},"enabled":true}),
    );
    tokio::fs::write(path, format!("{}\n", serde_json::to_string_pretty(&root)?)).await?;
    Ok(())
}

struct Connection {
    child: Child,
    stdin: ChildStdin,
    lines: tokio::io::Lines<BufReader<ChildStdout>>,
    next_id: u64,
}

impl Connection {
    async fn start(config: &ServerConfig) -> Result<Self> {
        if config.command.trim().is_empty() {
            return Err(anyhow!("MCP server command is empty"));
        }
        let mut command = Command::new(&config.command);
        command
            .args(&config.args)
            .envs(&config.env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        #[cfg(windows)]
        {
            command.creation_flags(0x08000000);
        }
        let mut child = command
            .spawn()
            .with_context(|| format!("Starting MCP server {}", config.command))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("MCP stdin unavailable"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("MCP stdout unavailable"))?;
        let mut connection = Self {
            child,
            stdin,
            lines: BufReader::new(stdout).lines(),
            next_id: 1,
        };
        connection
            .request(
                "initialize",
                json!({
                    "protocolVersion":"2025-06-18",
                    "capabilities":{},
                    "clientInfo":{"name":"jynx-harness","version":env!("CARGO_PKG_VERSION")}
                }),
            )
            .await?;
        connection
            .send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
            .await?;
        Ok(connection)
    }

    async fn send(&mut self, value: Value) -> Result<()> {
        let mut bytes = serde_json::to_vec(&value)?;
        bytes.push(b'\n');
        self.stdin.write_all(&bytes).await?;
        self.stdin.flush().await?;
        Ok(())
    }

    async fn request(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        self.send(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))
            .await?;
        timeout(Duration::from_secs(12), async {
            loop {
                let line = self
                    .lines
                    .next_line()
                    .await?
                    .ok_or_else(|| anyhow!("MCP server closed stdout"))?;
                if line.len() > 1_000_000 {
                    return Err(anyhow!("MCP response is too large"));
                }
                let response: Value = match serde_json::from_str(&line) {
                    Ok(value) => value,
                    Err(_) => continue,
                };
                if response["id"].as_u64() != Some(id) {
                    continue;
                }
                if !response["error"].is_null() {
                    return Err(anyhow!("MCP {method}: {}", response["error"]));
                }
                return Ok(response["result"].clone());
            }
        })
        .await
        .map_err(|_| anyhow!("MCP {method} timed out"))?
    }

    async fn tools(&mut self) -> Result<Vec<Value>> {
        let mut result = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..10 {
            let params = cursor
                .as_ref()
                .map_or_else(|| json!({}), |value| json!({"cursor":value}));
            let page = self.request("tools/list", params).await?;
            if let Some(tools) = page["tools"].as_array() {
                result.extend(tools.iter().take(128).cloned());
            }
            cursor = page["nextCursor"].as_str().map(str::to_owned);
            if cursor.is_none() || result.len() >= 128 {
                break;
            }
        }
        result.truncate(128);
        Ok(result)
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

pub struct Catalog {
    servers: HashMap<String, ServerConfig>,
    connections: HashMap<String, Mutex<Connection>>,
    tools: HashMap<String, (String, String)>,
    schemas: Vec<Value>,
    pub warnings: Vec<String>,
}

impl Catalog {
    pub fn empty() -> Self {
        Self {
            servers: HashMap::new(),
            connections: HashMap::new(),
            tools: HashMap::new(),
            schemas: Vec::new(),
            warnings: Vec::new(),
        }
    }

    pub async fn discover() -> Result<Self> {
        let config = read_config().await?;
        let mut catalog = Self {
            servers: config.servers,
            connections: HashMap::new(),
            tools: HashMap::new(),
            schemas: Vec::new(),
            warnings: Vec::new(),
        };
        let mut names: Vec<String> = catalog.servers.keys().cloned().collect();
        names.sort();
        for name in names {
            let Some(server) = catalog.servers.get(&name) else {
                continue;
            };
            if !server.enabled {
                continue;
            }
            match Connection::start(server).await {
                Ok(mut connection) => match connection.tools().await {
                    Ok(tools) => {
                        for tool in tools {
                            let Some(tool_name) = tool["name"].as_str() else {
                                continue;
                            };
                            let exported =
                                format!("mcp__{}__{}", safe_name(&name), safe_name(tool_name));
                            if catalog.tools.contains_key(&exported) {
                                continue;
                            }
                            catalog
                                .tools
                                .insert(exported.clone(), (name.clone(), tool_name.to_owned()));
                            catalog.schemas.push(json!({
                            "type":"function",
                            "function":{
                                "name":exported,
                                "description":format!("MCP server {name}: {}", tool["description"].as_str().unwrap_or(tool_name)),
                                "parameters":tool.get("inputSchema").filter(|value| value.is_object()).cloned().unwrap_or_else(|| json!({"type":"object","properties":{}}))
                            }
                            }));
                        }
                        catalog
                            .connections
                            .insert(name.clone(), Mutex::new(connection));
                    }
                    Err(error) => catalog.warnings.push(format!("{name}: {error}")),
                },
                Err(error) => catalog.warnings.push(format!("{name}: {error}")),
            }
        }
        Ok(catalog)
    }

    pub fn schemas(&self) -> &[Value] {
        &self.schemas
    }
    pub fn contains(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    pub async fn call(&self, exported: &str, arguments: Value) -> Result<String> {
        let (server_name, tool_name) = self
            .tools
            .get(exported)
            .ok_or_else(|| anyhow!("Unknown MCP tool"))?;
        let connection = self
            .connections
            .get(server_name)
            .ok_or_else(|| anyhow!("MCP server is no longer connected"))?;
        let mut connection = connection.lock().await;
        let result = connection
            .request(
                "tools/call",
                json!({"name":tool_name,"arguments":arguments}),
            )
            .await?;
        Ok(serde_json::to_string(&result)?)
    }
}

fn safe_name(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exported_names_are_safe() {
        assert_eq!(safe_name("my-server.tool"), "my_server_tool");
    }
    #[test]
    fn starter_config_parses() {
        assert!(
            serde_json::from_str::<Config>("{\"mcpServers\":{}}")
                .unwrap()
                .servers
                .is_empty()
        );
    }
}
