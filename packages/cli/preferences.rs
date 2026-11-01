use super::ChatOptions;
use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};

impl Default for ChatOptions {
    fn default() -> Self {
        Self {
            endpoint: "http://127.0.0.1:8080".into(),
            model: "local-model".into(),
            api_key: None,
            cwd: String::new(),
            context_size: 32768,
            local_compat: true,
            settings: Default::default(),
            prompt: None,
            llama_binary: None,
            runtime_name: None,
            gguf: None,
            gpu_layers: "all".into(),
            llama_extra: Vec::new(),
            runtime_verbosity: 3,
            flash_attention: "auto".into(),
            cache_k: "f16".into(),
            cache_v: "f16".into(),
            threads: 0,
            batch_size: 512,
            ubatch_size: 128,
        }
    }
}

pub fn home() -> PathBuf {
    std::env::var_os("JYNX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".jynx")
        })
}

pub fn load() -> Result<ChatOptions> {
    load_from(&home().join("config.toml"))
}

fn load_from(path: &Path) -> Result<ChatOptions> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(ChatOptions::default()),
        Err(e) => return Err(e).with_context(|| format!("Reading {}", path.display())),
    };
    let options: ChatOptions =
        toml::from_str(&text).with_context(|| format!("Invalid settings in {}", path.display()))?;
    validate(&options)?;
    Ok(options)
}

pub fn validate(options: &ChatOptions) -> Result<()> {
    if options.context_size < 512 {
        bail!("Context must be at least 512 tokens");
    }
    if !matches!(
        options.settings.permission_mode.as_str(),
        ":ask" | ":workspace" | ":danger-full-access"
    ) {
        bail!("Invalid permission mode");
    }
    if !matches!(options.flash_attention.as_str(), "auto" | "on" | "off") {
        bail!("Invalid flash attention mode");
    }
    for cache in [&options.cache_k, &options.cache_v] {
        if !matches!(
            cache.as_str(),
            "f16" | "bf16" | "f32" | "q8_0" | "q4_0" | "q4_1" | "q5_0" | "q5_1" | "iq4_nl"
        ) {
            bail!("Invalid KV cache type: {cache}");
        }
    }
    if options.batch_size == 0
        || options.ubatch_size == 0
        || options.ubatch_size > options.batch_size
    {
        bail!("Batch sizes must be positive; micro batch must not exceed batch size");
    }
    if options.runtime_verbosity > 5 {
        bail!("Log detail must be 0-5");
    }
    Ok(())
}

pub fn save(options: &ChatOptions) -> Result<()> {
    save_to(&home().join("config.toml"), options)
}

fn save_to(path: &Path, options: &ChatOptions) -> Result<()> {
    validate(options)?;
    let mut saved = options.clone();
    if saved.gguf.is_some() {
        saved.endpoint = "http://127.0.0.1:8080".into();
    }
    std::fs::create_dir_all(path.parent().context("Settings directory is missing")?)?;
    let text = toml::to_string_pretty(&saved)?;
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    std::fs::write(&temporary, text)?;
    std::fs::rename(&temporary, path).with_context(|| format!("Saving {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn settings_round_trip_excludes_secrets_workspace_and_prompt() {
        let dir = std::env::temp_dir().join(format!("jynx-preferences-{}", std::process::id()));
        let path = dir.join("config.toml");
        let mut options = ChatOptions::default();
        options.context_size = 65536;
        options.api_key = Some("synthetic-secret".into());
        options.cwd = "synthetic-workspace".into();
        options.prompt = Some("synthetic-prompt".into());
        save_to(&path, &options).unwrap();
        save_to(&path, &options).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("synthetic-"));
        let loaded = load_from(&path).unwrap();
        assert_eq!(loaded.context_size, 65536);
        assert_eq!(loaded.settings.permission_mode, ":ask");
        assert!(loaded.api_key.is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn partial_config_uses_defaults_and_rejects_invalid_loading_settings() {
        let loaded: ChatOptions = toml::from_str("context_size = 64000").unwrap();
        assert_eq!(loaded.cache_k, "f16");
        assert_eq!(loaded.context_size, 64000);
        let mut bad = loaded;
        bad.ubatch_size = 1024;
        assert!(validate(&bad).is_err());
    }
}
