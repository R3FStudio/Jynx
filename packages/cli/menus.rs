use super::{ChatOptions, preferences, ui};
use anyhow::{Context, Result};
use std::{
    io::Read,
    path::{Path, PathBuf},
};

pub fn permissions(options: &mut ChatOptions) -> Result<()> {
    let values = [":ask", ":workspace", ":danger-full-access"];
    let items = vec![
        "Ask for approval   Confirm file changes and commands".into(),
        "Workspace access   Allow work within this project".into(),
        "Full access        Allow tools without confirmation".into(),
    ];
    let selected = values
        .iter()
        .position(|v| *v == options.settings.permission_mode)
        .unwrap_or(0);
    if let Some(index) = ui::choose("Permissions", &items, selected)? {
        if index == 2
            && ui::choose(
                "Full access allows tools to change files and run commands without asking",
                &[
                    "Keep current permissions".into(),
                    "Enable full access".into(),
                ],
                0,
            )? != Some(1)
        {
            return Ok(());
        }
        options.settings.permission_mode = values[index].into();
    }
    Ok(())
}

pub fn context(options: &mut ChatOptions) -> Result<()> {
    loop {
        let value = ui::input(
            "Context window in tokens (blank keeps current)",
            &options.context_size.to_string(),
        )?;
        if value.is_empty() {
            return Ok(());
        }
        if let Ok(n) = value.replace([',', '_'], "").parse::<usize>() {
            if (512..=16_777_216).contains(&n) {
                options.context_size = n;
                return Ok(());
            }
        }
        println!(
            "Enter between 512 and 16,777,216 tokens. The model and available memory determine the usable maximum."
        );
    }
}

pub async fn settings(options: &mut ChatOptions) -> Result<()> {
    loop {
        let items = vec![
            format!("Model and endpoint   {}", options.model),
            format!("Context window       {} tokens", options.context_size),
            format!("Permissions          {}", options.settings.permission_mode),
            format!("Reasoning            {}", options.settings.reasoning_effort),
            format!(
                "Model loading        {} / GPU layers {}",
                options.runtime_name.as_deref().unwrap_or("automatic"),
                options.gpu_layers
            ),
            format!(
                "Web search           {}",
                if options.settings.web_search_enabled {
                    "enabled"
                } else {
                    "disabled"
                }
            ),
            format!(
                "Read websites        {}",
                if options.settings.browser_tool_enabled {
                    "enabled"
                } else {
                    "disabled"
                }
            ),
            "MCP servers          Configure tool integrations".into(),
            format!(
                "Tool output          {}",
                if options.settings.auto_open_tool_details {
                    "expanded"
                } else {
                    "concise / inspect with /tools"
                }
            ),
            "Save and return to chat".into(),
        ];
        match ui::choose("Jynx settings / arrows to move / type to filter", &items, 0)? {
            Some(0) => models(options)?,
            Some(1) => context(options)?,
            Some(2) => permissions(options)?,
            Some(3) => {
                let effort = ui::input(
                    "Native model reasoning value (default uses the model default)",
                    &options.settings.reasoning_effort,
                )?;
                if !effort.is_empty() {
                    options.settings.reasoning_effort = effort;
                }
            }
            Some(4) => loading(options)?,
            Some(5) => options.settings.web_search_enabled = !options.settings.web_search_enabled,
            Some(6) => {
                options.settings.browser_tool_enabled = !options.settings.browser_tool_enabled
            }
            Some(7) => super::mcp_menu(false).await?,
            Some(8) => {
                options.settings.auto_open_tool_details = !options.settings.auto_open_tool_details
            }
            _ => break,
        }
    }
    Ok(())
}

pub fn tool_results(tools: &[jynx_protocol::ToolEvent]) -> Result<()> {
    if tools.is_empty() {
        println!("No tool results in this session yet.");
        return Ok(());
    }
    let items = tools
        .iter()
        .map(|tool| format!("{} / {}", tool.label, tool.status))
        .collect::<Vec<_>>();
    while let Some(index) = ui::choose("Tool results / Esc to return", &items, 0)? {
        let tool = &tools[index];
        if let Some(arguments) = &tool.arguments {
            super::print_expanded_value("arguments", arguments);
        }
        if let Some(output) = &tool.output {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(output) {
                super::print_expanded_value("output", &value);
            } else {
                super::print_indented("output", output);
            }
        }
        if let Some(diff) = &tool.diff {
            super::print_indented("diff", diff);
        }
        let _ = ui::input("Press Enter to return to results", "")?;
    }
    Ok(())
}

#[derive(Debug)]
pub struct LocalModel {
    pub path: PathBuf,
    pub bytes: u64,
}

fn model_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    if !name.ends_with(".gguf")
        || [
            "mmproj",
            "projector",
            "imatrix",
            "mtp",
            "draft",
            "tokenizer",
        ]
        .iter()
        .any(|part| {
            name.starts_with(part)
                || name.contains(&format!("-{part}-"))
                || name.contains(&format!("_{part}_"))
        })
    {
        return false;
    }
    if let Some((_, split)) = name.rsplit_once("-of-") {
        if split.ends_with(".gguf") && split[..split.len() - 5].chars().all(|c| c.is_ascii_digit())
        {
            let before = name.rsplit_once("-of-").unwrap().0;
            return before.ends_with("-00001");
        }
    }
    true
}

pub fn scan_models(root: &Path) -> Result<Vec<LocalModel>> {
    fn visit(root: &Path, depth: usize, models: &mut Vec<LocalModel>) -> Result<()> {
        if depth > 12 {
            return Ok(());
        }
        let entries = match std::fs::read_dir(root) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e).with_context(|| format!("Cannot scan {}", root.display())),
        };
        for entry in entries {
            let entry = entry?;
            let kind = entry.file_type()?;
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() {
                visit(&entry.path(), depth + 1, models)?;
            } else if kind.is_file() && model_name(&entry.file_name().to_string_lossy()) {
                let mut header = [0; 8];
                if let Ok(mut file) = std::fs::File::open(entry.path()) {
                    if file.read_exact(&mut header).is_ok()
                        && &header[..4] == b"GGUF"
                        && (2..=4).contains(&u32::from_le_bytes(header[4..].try_into().unwrap()))
                    {
                        models.push(LocalModel {
                            path: entry.path(),
                            bytes: entry.metadata()?.len(),
                        });
                    }
                }
            }
        }
        Ok(())
    }
    let mut models = Vec::new();
    visit(root, 0, &mut models)?;
    models.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(models)
}

pub fn models(options: &mut ChatOptions) -> Result<()> {
    let root = preferences::home().join("models");
    let models = scan_models(&root)?;
    let mut items = models
        .iter()
        .map(|m| {
            format!(
                "{}   {:.2} GB{}",
                m.path.strip_prefix(&root).unwrap_or(&m.path).display(),
                m.bytes as f64 / 1_000_000_000.0,
                if options.gguf.as_deref() == m.path.to_str() {
                    " / selected"
                } else {
                    ""
                }
            )
        })
        .collect::<Vec<_>>();
    items.push("Open a GGUF file by path".into());
    items.push("Cloud / custom endpoint and API key".into());
    let selected = models
        .iter()
        .position(|m| options.gguf.as_deref() == m.path.to_str())
        .unwrap_or(0);
    let Some(index) = ui::choose(&format!("Models / {}", root.display()), &items, selected)? else {
        return Ok(());
    };
    if index == models.len() + 1 {
        return cloud(options);
    }
    let path = if index == models.len() {
        let value = ui::input("GGUF path", "")?;
        if value.is_empty() {
            return Ok(());
        }
        PathBuf::from(value.trim_matches('"'))
    } else {
        models[index].path.clone()
    };
    let path = path.canonicalize().context("Model file does not exist")?;
    anyhow::ensure!(
        path.is_file()
            && path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("gguf")),
        "Choose an existing GGUF file"
    );
    options.gguf = Some(path.to_string_lossy().into_owned());
    options.model = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    options.local_compat = true;
    options.api_key = None;
    println!(
        "Selected {}. The runtime loads it when you send a message.",
        options.model
    );
    loading(options)
}

fn cloud(options: &mut ChatOptions) -> Result<()> {
    let providers = [
        ("OpenAI", "https://api.openai.com/v1", "OPENAI_API_KEY"),
        (
            "Anthropic (OpenAI-compatible API)",
            "https://api.anthropic.com/v1",
            "ANTHROPIC_API_KEY",
        ),
        (
            "Custom OpenAI-compatible endpoint",
            "",
            "JYNX_MODEL_API_KEY",
        ),
    ];
    let Some(index) = ui::choose(
        "Cloud provider",
        &providers.iter().map(|p| p.0.to_owned()).collect::<Vec<_>>(),
        0,
    )?
    else {
        return Ok(());
    };
    let (_, base, key_name) = providers[index];
    let endpoint = if base.is_empty() {
        ui::input("Endpoint URL", &options.endpoint)?
    } else {
        base.into()
    };
    let url = reqwest::Url::parse(&endpoint).context("Enter a valid endpoint URL")?;
    anyhow::ensure!(
        matches!(url.scheme(), "https" | "http")
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none(),
        "Use an HTTP(S) endpoint without embedded credentials"
    );
    let model = ui::input(
        "Model ID",
        if options.local_compat {
            ""
        } else {
            &options.model
        },
    )?;
    if model.is_empty() {
        return Ok(());
    }
    println!(
        "API keys are kept for this session only. Set {key_name} to reuse a key without saving it in config.toml."
    );
    let key = ui::secret("API key (blank uses the provider environment variable)")?;
    options.api_key = if key.is_empty() {
        std::env::var(key_name)
            .ok()
            .filter(|k| !k.trim().is_empty())
    } else {
        Some(key)
    };
    options.endpoint = endpoint;
    options.model = model;
    options.local_compat = false;
    options.gguf = None;
    options.llama_binary = None;
    Ok(())
}

fn number(label: &str, current: u32, min: u32, max: u32) -> Result<u32> {
    loop {
        let text = ui::input(label, &current.to_string())?;
        if text.is_empty() {
            return Ok(current);
        }
        if let Ok(value) = text.parse::<u32>() {
            if (min..=max).contains(&value) {
                return Ok(value);
            }
        }
        println!("Enter a number from {min} to {max}.");
    }
}

pub fn loading(options: &mut ChatOptions) -> Result<()> {
    loop {
        let items = vec![
            format!("Context window      {} tokens", options.context_size),
            format!(
                "Runtime backend     {}",
                options
                    .runtime_name
                    .as_deref()
                    .unwrap_or("installation default")
            ),
            format!("GPU layers          {}", options.gpu_layers),
            format!("Flash attention     {}", options.flash_attention),
            format!("Key cache type      {}", options.cache_k),
            format!("Value cache type    {}", options.cache_v),
            format!("CPU threads         {} (0 = automatic)", options.threads),
            format!("Batch size          {}", options.batch_size),
            format!("Micro batch size    {}", options.ubatch_size),
            format!(
                "Server executable   {}",
                options
                    .llama_binary
                    .as_deref()
                    .unwrap_or("bundled llama.cpp")
            ),
            format!("Log detail          {}", options.runtime_verbosity),
            format!("Advanced arguments  {}", options.llama_extra.len()),
            "Clear advanced arguments".into(),
            "Done".into(),
        ];
        match ui::choose(
            "Model loading / changes apply on the next message",
            &items,
            0,
        )? {
            Some(0) => context(options)?,
            Some(1) => {
                let choices = crate::installed_runtime::available(&std::env::current_exe()?)?;
                if choices.is_empty() {
                    println!(
                        "No installed runtime manifest found. Select a llama-server executable below."
                    );
                } else if let Some(i) = ui::choose("Installed backends", &choices, 0)? {
                    options.runtime_name = Some(choices[i].clone());
                    options.llama_binary = None;
                }
            }
            Some(2) => {
                let value = ui::input(
                    "GPU layers (all or a nonnegative integer)",
                    &options.gpu_layers,
                )?;
                if value == "all" || value.parse::<u32>().is_ok() {
                    options.gpu_layers = value;
                }
            }
            Some(3) => {
                let choices = vec!["auto".into(), "on".into(), "off".into()];
                if let Some(i) = ui::choose("Flash attention", &choices, 0)? {
                    if i == 2 && (quantized(&options.cache_k) || quantized(&options.cache_v)) {
                        println!(
                            "Quantized KV cache requires flash attention. Choose a floating point cache first."
                        );
                    } else {
                        options.flash_attention = choices[i].clone();
                    }
                }
            }
            Some(index @ (4 | 5)) => {
                let types = [
                    "f16", "bf16", "f32", "q8_0", "q4_0", "q4_1", "q5_0", "q5_1", "iq4_nl",
                ]
                .map(str::to_owned)
                .to_vec();
                if let Some(i) = ui::choose("KV cache type (f16 is the default)", &types, 0)? {
                    if quantized(&types[i]) {
                        options.flash_attention = "on".into();
                    }
                    if index == 4 {
                        options.cache_k = types[i].clone();
                    } else {
                        options.cache_v = types[i].clone();
                    }
                }
            }
            Some(6) => options.threads = number("CPU threads", options.threads, 0, 4096)?,
            Some(7) => {
                options.batch_size = number("Batch size", options.batch_size, 1, 1_048_576)?;
                options.ubatch_size = options.ubatch_size.min(options.batch_size);
            }
            Some(8) => {
                options.ubatch_size = number(
                    "Micro batch size",
                    options.ubatch_size,
                    1,
                    options.batch_size,
                )?
            }
            Some(9) => {
                let path = ui::input(
                    "llama-server executable (blank uses bundled runtime)",
                    options.llama_binary.as_deref().unwrap_or(""),
                )?;
                if path.is_empty() {
                    options.llama_binary = None;
                } else {
                    let path = PathBuf::from(path.trim_matches('"'))
                        .canonicalize()
                        .context("Server executable does not exist")?;
                    anyhow::ensure!(path.is_file(), "Choose a server executable");
                    options.llama_binary = Some(path.to_string_lossy().into_owned());
                }
            }
            Some(10) => {
                options.runtime_verbosity =
                    number("Log detail", options.runtime_verbosity as u32, 0, 5)? as u8
            }
            Some(11) => {
                let value = ui::input("One llama-server argument (repeat to add more)", "")?;
                if !value.is_empty() {
                    options.llama_extra.push(value);
                }
            }
            Some(12) => options.llama_extra.clear(),
            _ => break,
        }
    }
    Ok(())
}

fn quantized(value: &str) -> bool {
    !matches!(value, "f16" | "bf16" | "f32")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn discovers_models_without_companions_deleted_files_or_duplicate_shards() {
        let root = std::env::temp_dir().join(format!("jynx-model-picker-{}", std::process::id()));
        std::fs::create_dir_all(root.join("nested")).unwrap();
        for name in [
            "chat-Q4.gguf",
            "mmproj-chat.gguf",
            "imatrix.gguf",
            "model-00001-of-00002.gguf",
            "model-00002-of-00002.gguf",
            "mtp-draft.gguf",
        ] {
            std::fs::write(root.join("nested").join(name), b"GGUF\x03\0\0\0synthetic").unwrap();
        }
        std::fs::write(root.join("broken.gguf"), b"not a model").unwrap();
        let found = scan_models(&root).unwrap();
        assert_eq!(found.len(), 2);
        std::fs::remove_file(root.join("nested/chat-Q4.gguf")).unwrap();
        assert_eq!(scan_models(&root).unwrap().len(), 1);
        std::fs::remove_dir_all(root).unwrap();
    }
}
