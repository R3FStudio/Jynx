pub fn available(executable: &std::path::Path) -> anyhow::Result<Vec<String>> {
    let exe = executable.canonicalize()?;
    let Some(root) = exe.parent().and_then(std::path::Path::parent) else {
        return Ok(Vec::new());
    };
    let path = root.join("runtime-manifest.json");
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return source_available(executable),
        Err(e) => return Err(e.into()),
    };
    let manifest: serde_json::Value = serde_json::from_str(&text)?;
    let mut names = Vec::new();
    if let Some(entries) = manifest["runtimes"].as_array() {
        for entry in entries {
            if let Some(name) = entry["id"].as_str() {
                if resolve(executable, Some(name)).is_ok() {
                    names.push(name.to_owned());
                }
            }
        }
    }
    names.sort();
    names.dedup();
    Ok(names)
}
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
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return source_resolve(executable, requested);
        }
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
fn source_root(executable: &Path) -> Option<PathBuf> {
    let platform = if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    };
    let architecture = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else if cfg!(target_arch = "x86_64") {
        "x64"
    } else {
        std::env::consts::ARCH
    };
    executable
        .ancestors()
        .skip(1)
        .map(|parent| {
            parent
                .join("vendor/llama.cpp/binaries")
                .join(platform)
                .join(architecture)
        })
        .find(|path| path.is_dir())
}

fn source_available(executable: &Path) -> Result<Vec<String>> {
    let Some(root) = source_root(executable) else {
        return Ok(Vec::new());
    };
    let mut names = Vec::new();
    for entry in std::fs::read_dir(&root)? {
        let entry = entry?;
        if entry.file_type()?.is_dir()
            && entry
                .path()
                .join(if cfg!(windows) {
                    "llama-server.exe"
                } else {
                    "llama-server"
                })
                .is_file()
        {
            names.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    names.sort();
    Ok(names)
}

fn source_resolve(executable: &Path, requested: Option<&str>) -> Result<Option<PathBuf>> {
    let Some(root) = source_root(executable) else {
        return Ok(None);
    };
    let name = requested.unwrap_or("cpu");
    if Path::new(name).components().count() != 1
        || !matches!(
            Path::new(name).components().next(),
            Some(Component::Normal(_))
        )
    {
        bail!("Invalid runtime name");
    }
    let server = root.join(name).join(if cfg!(windows) {
        "llama-server.exe"
    } else {
        "llama-server"
    });
    if !server.is_file() {
        bail!(
            "Runtime {name} is not installed. Select another backend or a llama-server executable."
        );
    }
    let server = server.canonicalize()?;
    if !server.starts_with(root.canonicalize()?) {
        bail!("Runtime must stay in its binaries directory");
    }
    Ok(Some(server))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_builds_discover_downloaded_backends_and_reject_path_escapes() {
        let root = std::env::temp_dir().join(format!("jynx-source-runtime-{}", std::process::id()));
        let platform = if cfg!(windows) {
            "windows"
        } else if cfg!(target_os = "macos") {
            "macos"
        } else {
            "linux"
        };
        let arch = if cfg!(target_arch = "aarch64") {
            "arm64"
        } else if cfg!(target_arch = "x86_64") {
            "x64"
        } else {
            std::env::consts::ARCH
        };
        let runtime = root
            .join("vendor/llama.cpp/binaries")
            .join(platform)
            .join(arch)
            .join("cpu");
        std::fs::create_dir_all(&runtime).unwrap();
        let server = runtime.join(if cfg!(windows) {
            "llama-server.exe"
        } else {
            "llama-server"
        });
        std::fs::write(server, b"synthetic").unwrap();
        let exe = root.join("bin/jynx-cli");
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(&exe, b"synthetic").unwrap();
        assert_eq!(available(&exe).unwrap(), vec!["cpu"]);
        assert!(resolve(&exe, None).unwrap().is_some());
        assert!(resolve(&exe, Some("../escape")).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
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
        assert_eq!(available(&exe).unwrap(), vec!["cpu"]);
        std::fs::write(
            &manifest,
            r#"{"runtimes":[{"id":"cpu","server":"../outside"}]}"#,
        )
        .unwrap();
        assert!(resolve(&exe, None).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
