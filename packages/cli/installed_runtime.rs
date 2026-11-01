use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::path::{Component, Path, PathBuf};
pub fn resolve(executable: &Path, requested: Option<&str>) -> Result<Option<PathBuf>> {
    let exe = executable.canonicalize()?;
    let Some(root) = exe.parent().and_then(Path::parent) else {
        return Ok(None);
    };
    let manifest_path = root.join("runtime-manifest.json");
    let text = match std::fs::read_to_string(&manifest_path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let manifest: Value =
        serde_json::from_str(&text).context("Invalid installed runtime manifest")?;
    let name = requested.unwrap_or_else(|| {
        manifest
            .get("defaultRuntime")
            .and_then(Value::as_str)
            .unwrap_or("cpu")
    });
    let runtimes = manifest
        .get("runtimes")
        .and_then(Value::as_array)
        .context("Installed runtime list is missing")?;
    let entry = runtimes
        .iter()
        .find(|entry| entry.get("id").and_then(Value::as_str) == Some(name))
        .with_context(|| {
            format!("Runtime {name} is not installed; choose a backend in runtime-manifest.json")
        })?;
    let relative = entry
        .get("server")
        .and_then(Value::as_str)
        .context("Installed runtime path is missing")?;
    let relative = Path::new(relative);
    if relative.is_absolute()
        || relative
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        bail!("Invalid installed runtime path");
    }
    let server = root
        .join(relative)
        .canonicalize()
        .context("Installed llama-server is missing")?;
    if !server.starts_with(root) || !server.is_file() {
        bail!("Installed runtime must stay inside the installation directory");
    }
    Ok(Some(server))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selects_installed_runtime_and_rejects_escapes() {
        let root =
            std::env::temp_dir().join(format!("jynx-installed-runtime-{}", std::process::id()));
        std::fs::create_dir_all(root.join("bin")).unwrap();
        let exe = root.join("bin/jynx-cli");
        std::fs::write(&exe, b"synthetic").unwrap();
        assert!(resolve(&exe, None).unwrap().is_none());
        std::fs::create_dir_all(root.join("runtimes/cpu")).unwrap();
        std::fs::write(root.join("runtimes/cpu/llama-server"), b"synthetic").unwrap();
        let manifest = root.join("runtime-manifest.json");
        std::fs::write(&manifest,r#"{"defaultRuntime":"cpu","runtimes":[{"id":"cpu","server":"runtimes/cpu/llama-server"}]}"#).unwrap();
        assert!(
            resolve(&exe, None)
                .unwrap()
                .unwrap()
                .ends_with("llama-server")
        );
        assert!(resolve(&exe, Some("missing")).is_err());
        std::fs::write(
            &manifest,
            r#"{"runtimes":[{"id":"cpu","server":"../outside"}]}"#,
        )
        .unwrap();
        assert!(resolve(&exe, None).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
