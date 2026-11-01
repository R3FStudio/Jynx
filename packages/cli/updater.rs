use anyhow::{Context, Result, bail};
use flate2::read::GzDecoder;
use semver::Version;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{self, IsTerminal, Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};
const API: &str = "https://api.github.com/repos/R3FStudio/Jynx/releases/latest";
const DOWNLOADS: &str = "https://github.com/R3FStudio/Jynx/releases/download/";
const MAX_BINARY: usize = 128 * 1024 * 1024;
#[derive(Debug)]
struct Update {
    version: Version,
    archive: String,
    checksum: String,
}
fn platform() -> &'static str {
    match std::env::consts::OS {
        "windows" => "windows",
        "macos" => "macos",
        _ => "linux",
    }
}
fn architecture() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        value => value,
    }
}
fn select_release(value: &Value, current: &str, os: &str, arch: &str) -> Option<Update> {
    if value.get("draft")?.as_bool()? || value.get("prerelease")?.as_bool()? {
        return None;
    }
    let tag = value.get("tag_name")?.as_str()?;
    let version = Version::parse(tag.strip_prefix('v').unwrap_or(tag)).ok()?;
    if !version.pre.is_empty() || version <= Version::parse(current).ok()? {
        return None;
    }
    let name = format!("Jynx-CLI-Only-v{version}-{os}-{arch}.tar.gz");
    let assets = value.get("assets")?.as_array()?;
    let url = |name: &str| -> Option<String> {
        let asset = assets
            .iter()
            .find(|a| a.get("name").and_then(Value::as_str) == Some(name))?;
        let url = asset.get("browser_download_url")?.as_str()?;
        (url == format!("{DOWNLOADS}{tag}/{name}")).then(|| url.to_owned())
    };
    Some(Update {
        version,
        archive: url(&name)?,
        checksum: url(&format!("{name}.sha256"))?,
    })
}
async fn download(client: &reqwest::Client, url: &str, limit: usize) -> Result<Vec<u8>> {
    let mut response = client.get(url).send().await?.error_for_status()?;
    if response.content_length().is_some_and(|n| n > limit as u64) {
        bail!("Release asset exceeds size limit");
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if bytes.len().saturating_add(chunk.len()) > limit {
            bail!("Release asset exceeds size limit");
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
fn verify_checksum(bytes: &[u8], checksum: &[u8]) -> Result<()> {
    let text = std::str::from_utf8(checksum)?;
    let digest = text
        .split_whitespace()
        .next()
        .context("Missing release checksum")?;
    if digest.len() != 64 || !digest.chars().all(|c| c.is_ascii_hexdigit()) {
        bail!("Invalid release checksum");
    }
    if !format!("{:x}", Sha256::digest(bytes)).eq_ignore_ascii_case(digest) {
        bail!("Release checksum mismatch; installed CLI was not changed");
    }
    Ok(())
}
fn extract_binary(bytes: &[u8], executable: &str) -> Result<Vec<u8>> {
    let mut archive =
        tar::Archive::new(GzDecoder::new(bytes).take((MAX_BINARY + 1024 * 1024) as u64));
    let mut binary = None;
    for item in archive.entries()? {
        let mut entry = item?;
        let path = entry.path()?.into_owned();
        if !entry.header().entry_type().is_file() {
            bail!("Unexpected release archive entry");
        }
        if path == Path::new("LICENSE") || path == Path::new("COPYRIGHT") {
            continue;
        }
        if path != Path::new(executable) || binary.is_some() {
            bail!("Unexpected release archive path");
        }
        if entry.size() > MAX_BINARY as u64 {
            bail!("Executable exceeds size limit");
        }
        let mut data = Vec::new();
        entry
            .by_ref()
            .take(MAX_BINARY as u64 + 1)
            .read_to_end(&mut data)?;
        if data.is_empty() || data.len() > MAX_BINARY {
            bail!("Invalid release executable");
        }
        binary = Some(data);
    }
    binary.context("Executable missing from release archive")
}
struct Stage(PathBuf);
impl Drop for Stage {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn create_stage(parent: &Path) -> Result<Stage> {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let path = parent.join(format!(".jynx-update-{}-{nonce}", std::process::id()));
    fs::create_dir(&path).context(
        "Cannot write beside the installed CLI; update through your installation manager",
    )?;
    Ok(Stage(path))
}
#[cfg(not(windows))]
fn install(stage: Stage, staged: &Path, current: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(staged)?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(staged, permissions)?;
    fs::rename(staged, current).context("Cannot replace installed CLI")?;
    drop(stage);
    println!("Update installed. Run jynx-cli again to use the new version.");
    Ok(())
}
#[cfg(windows)]
fn windows_script(directory: &Path, staged: &Path, current: &Path, pid: u32) -> String {
    let quote = |p: &Path| format!("'{}'", p.to_string_lossy().replace('\'', "''"));
    let backup = directory.join("previous.exe");
    let script = directory.join("install.ps1");
    format!(
        r#"$ErrorActionPreference='Stop'
Wait-Process -Id {pid} -ErrorAction SilentlyContinue
$updated=$false
for($attempt=0;$attempt -lt 60;$attempt++){{
 try{{Move-Item -LiteralPath {current} -Destination {backup};$updated=$true;break}}catch{{Start-Sleep -Milliseconds 500}}
}}
if(-not $updated){{exit 1}}
try{{Move-Item -LiteralPath {staged} -Destination {current}}}catch{{Move-Item -LiteralPath {backup} -Destination {current};exit 1}}
Remove-Item -LiteralPath {backup}
Remove-Item -LiteralPath {script}
Remove-Item -LiteralPath {stage}
"#,
        current = quote(current),
        backup = quote(&backup),
        staged = quote(staged),
        script = quote(&script),
        stage = quote(directory)
    )
}
#[cfg(windows)]
fn install(stage: Stage, staged: &Path, current: &Path) -> Result<()> {
    use std::os::windows::process::CommandExt;
    let script = stage.0.join("install.ps1");
    fs::write(
        &script,
        windows_script(&stage.0, staged, current, std::process::id()),
    )?;
    std::process::Command::new("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(&script)
        .creation_flags(0x08000000)
        .spawn()
        .context("Cannot start update installer")?;
    std::mem::forget(stage);
    println!(
        "Update verified. Finishing installation as this process exits; then run jynx-cli again."
    );
    Ok(())
}
async fn update(client: &reqwest::Client, release: &Update) -> Result<()> {
    let current = std::env::current_exe()?.canonicalize()?;
    let stage = create_stage(
        current
            .parent()
            .context("Installed CLI has no parent directory")?,
    )?;
    println!("Downloading Jynx CLI {}...", release.version);
    let bytes = download(client, &release.archive, MAX_BINARY).await?;
    let checksum = download(client, &release.checksum, 4096).await?;
    verify_checksum(&bytes, &checksum)?;
    let executable = if cfg!(windows) {
        "jynx-cli.exe"
    } else {
        "jynx-cli"
    };
    let binary = extract_binary(&bytes, executable)?;
    let staged = stage.0.join(executable);
    fs::write(&staged, binary)?;
    install(stage, &staged, &current)
}

pub async fn startup() -> bool {
    if !io::stdin().is_terminal()
        || !io::stdout().is_terminal()
        || std::env::var_os("JYNX_NO_UPDATE_CHECK").is_some()
    {
        return false;
    }
    let Ok(client) = reqwest::Client::builder()
        .user_agent(concat!("jynx-cli/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(120))
        .connect_timeout(Duration::from_secs(3))
        .build()
    else {
        return false;
    };
    let release = tokio::time::timeout(Duration::from_secs(3), async {
        let bytes = download(&client, API, 2 * 1024 * 1024).await?;
        let value: Value = serde_json::from_slice(&bytes)?;
        Ok::<_, anyhow::Error>(select_release(
            &value,
            env!("CARGO_PKG_VERSION"),
            platform(),
            architecture(),
        ))
    })
    .await;
    let Ok(Ok(Some(release))) = release else {
        return false;
    };
    println!(
        "\nJynx CLI update available\n  {} > {}\n\n[U] Update   [C] Continue with this version (default)",
        env!("CARGO_PKG_VERSION"),
        release.version
    );
    print!("> ");
    let _ = io::stdout().flush();
    let mut answer = String::new();
    if io::stdin().read_line(&mut answer).is_err()
        || !matches!(answer.trim().to_ascii_lowercase().as_str(), "u" | "update")
    {
        return false;
    }
    match update(&client, &release).await {
        Ok(()) => true,
        Err(error) => {
            eprintln!("Update failed: {error:#}\nContinuing with the installed version.");
            false
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn release(version: &str) -> Value {
        let name = format!("Jynx-CLI-Only-v{version}-windows-x64.tar.gz");
        serde_json::json!({"draft":false,"prerelease":false,"tag_name":format!("v{version}"),"assets":[
            {"name":name,"browser_download_url":format!("{DOWNLOADS}v{version}/{name}")},
            {"name":format!("{name}.sha256"),"browser_download_url":format!("{DOWNLOADS}v{version}/{name}.sha256")}]})
    }
    #[test]
    fn selects_newer_matching_release() {
        assert_eq!(
            select_release(&release("0.0.2"), "0.0.1", "windows", "x64")
                .unwrap()
                .version
                .to_string(),
            "0.0.2"
        );
    }
    #[test]
    fn rejects_old_wrong_arch_and_prereleases() {
        assert!(select_release(&release("0.0.1"), "0.0.1", "windows", "x64").is_none());
        assert!(select_release(&release("0.0.2"), "0.0.1", "linux", "arm64").is_none());
        let mut r = release("0.0.2");
        r["prerelease"] = true.into();
        assert!(select_release(&r, "0.0.1", "windows", "x64").is_none());
    }
    #[test]
    fn rejects_legacy_archive_names() {
        let mut r = release("0.0.2");
        for asset in r["assets"].as_array_mut().unwrap() {
            let old_name = asset["name"].as_str().unwrap().replace("Jynx-CLI-Only-v", "jynx-cli-");
            asset["browser_download_url"] = format!("{DOWNLOADS}v0.0.2/{old_name}").into();
            asset["name"] = old_name.into();
        }
        assert!(select_release(&r, "0.0.1", "windows", "x64").is_none());
    }
    #[test]
    fn requires_checksum_and_official_asset_destination() {
        let mut r = release("0.0.2");
        r["assets"][0]["browser_download_url"] = "https://example.invalid/cli".into();
        assert!(select_release(&r, "0.0.1", "windows", "x64").is_none());
        r = release("0.0.2");
        r["assets"].as_array_mut().unwrap().pop();
        assert!(select_release(&r, "0.0.1", "windows", "x64").is_none());
    }
    #[test]
    fn rejects_corrupt_download() {
        let digest = format!("{:x}  binary.tar.gz", Sha256::digest(b"verified"));
        assert!(verify_checksum(b"verified", digest.as_bytes()).is_ok());
        assert!(verify_checksum(b"changed", digest.as_bytes()).is_err());
        assert!(verify_checksum(b"verified", b"not-a-digest").is_err());
    }
    fn archive(name: &str) -> Vec<u8> {
        let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        let mut builder = tar::Builder::new(encoder);
        let mut header = tar::Header::new_gnu();
        header.set_size(6);
        header.set_mode(0o755);
        header.set_cksum();
        builder
            .append_data(&mut header, name, &b"binary"[..])
            .unwrap();
        builder.into_inner().unwrap().finish().unwrap()
    }
    #[test]
    fn extracts_only_expected_executable() {
        assert_eq!(
            extract_binary(&archive("jynx-cli.exe"), "jynx-cli.exe").unwrap(),
            b"binary"
        );
        assert!(extract_binary(&archive("other.exe"), "jynx-cli.exe").is_err());
        assert!(extract_binary(&archive("nested/jynx-cli.exe"), "jynx-cli.exe").is_err());
    }
    #[cfg(windows)]
    #[test]
    fn windows_installer_replaces_files_and_rolls_back_failed_moves() {
        use std::os::windows::process::CommandExt;
        for missing in [false, true] {
            let parent = create_stage(&std::env::temp_dir()).unwrap();

            let folder = parent.0.join("team's CLI");
            fs::create_dir(&folder).unwrap();
            let current = folder.join("jynx-cli.exe");
            fs::write(&current, b"old").unwrap();
            let stage = create_stage(&folder).unwrap();
            let staged = stage.0.join("jynx-cli.exe");
            if !missing {
                fs::write(&staged, b"new").unwrap();
            }
            let script = stage.0.join("install.ps1");
            fs::write(
                &script,
                windows_script(&stage.0, &staged, &current, 2147483647),
            )
            .unwrap();
            let status = std::process::Command::new("powershell.exe")
                .args([
                    "-NoLogo",
                    "-NoProfile",
                    "-NonInteractive",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-File",
                ])
                .arg(&script)
                .creation_flags(0x08000000)
                .status()
                .unwrap();
            assert_eq!(status.success(), !missing);
            assert_eq!(
                fs::read(&current).unwrap(),
                if missing { b"old" } else { b"new" }
            );
        }
    }
    #[cfg(not(windows))]
    #[test]
    fn unix_installer_replaces_executable_atomically() {
        let parent = create_stage(&std::env::temp_dir()).unwrap();
        let current = parent.0.join("jynx-cli");
        fs::write(&current, b"old").unwrap();
        let stage = create_stage(&parent.0).unwrap();
        let staged = stage.0.join("jynx-cli");
        fs::write(&staged, b"new").unwrap();
        install(stage, &staged, &current).unwrap();
        assert_eq!(fs::read(&current).unwrap(), b"new");
    }
}
