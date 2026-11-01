use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    env,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ApprovalMode {


    Ask,


    Workspace,


    FullAccess,
}

#[derive(Debug, Clone)]
pub struct WorkspaceSandbox {
    root: PathBuf,
    mode: ApprovalMode,
}

impl WorkspaceSandbox {
    pub fn new(root: impl AsRef<Path>, mode: ApprovalMode) -> Result<Self> {
        let root = root.as_ref().canonicalize()?;
        Ok(Self { root, mode })
    }

    pub fn resolve(&self, candidate: impl AsRef<Path>, _write: bool) -> Result<PathBuf> {
        let candidate = candidate.as_ref();



        let raw = candidate.to_string_lossy().trim().replace('\\', "/");
        let lower = raw.to_ascii_lowercase();
        let workspace_relative = raw
            .strip_prefix("$workspace/")
            .or_else(|| raw.strip_prefix("${workspace}/"))



            .or_else(|| lower.strip_prefix("/workspace/").map(|_| &raw[11..]));
        let joined = if matches!(lower.as_str(), "$workspace" | "${workspace}" | "/workspace") {
            self.root.clone()
        } else if let Some(relative) = workspace_relative {
            self.root.join(relative)
        } else if candidate.is_absolute() {
            candidate.to_path_buf()
        } else {
            self.root.join(candidate)
        };
        let normalized = normalize_without_existing(&joined);


        Ok(normalized)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn mode(&self) -> ApprovalMode {
        self.mode
    }
    pub fn requires_approval(&self) -> bool {
        self.mode == ApprovalMode::Ask
    }
    pub fn is_full_access(&self) -> bool {
        self.mode == ApprovalMode::FullAccess
    }

    pub fn requires_approval_for_tool(&self, name: &str, args: &Value, path: Option<&str>) -> bool {
        if name == "ask_user" { return false; }
        match self.mode {
            ApprovalMode::Ask => true,
            ApprovalMode::FullAccess => false,
            ApprovalMode::Workspace => {
                if name == "run_skill_script" {
                    return true;
                }
                if name == "read_skill_file" {
                    return false;
                }
                if matches!(name, "run_command" | "run_in_terminal_panel") {
                    return command_needs_manual_approval(
                        args["command"].as_str().unwrap_or_default(),
                    );
                }
                if matches!(name, "web_search" | "web_fetch" | "open_browser") {
                    return false;
                }
                let candidate = path.or_else(|| args["path"].as_str()).unwrap_or(".");
                match self.resolve(candidate, matches!(name, "write_file" | "edit_file")) {
                    Ok(resolved) => self.is_sensitive_path(&resolved),
                    Err(_) => true,
                }
            }
        }
    }

    pub fn is_sensitive_path(&self, path: &Path) -> bool {




        if path.starts_with(&self.root) {
            return false;
        }
        let normalized = path.to_string_lossy().replace('\\', "/").to_lowercase();
        let sensitive_fragments = [
            "/.ssh/",
            "/.gnupg/",
            "/.aws/",
            "/.azure/",
            "/.kube/",
            "/.docker/",
            "/.config/",
            "/.local/share/keyrings/",
            "/appdata/",
            "/library/keychains/",
            "/windows/system32/",
            "/windows/syswow64/",
            "/programdata/",
            "/etc/",
            "/users/",
            "/home/",
            "/root/",
            "/system/",
            "/private/var/db/",
            "/private/etc/",
            "/credentials/",
            "/secrets/",
            "/passwords/",
            "/keychains/",
        ];
        if sensitive_fragments
            .iter()
            .any(|needle| normalized.contains(needle))
        {
            return true;
        }

        for key in ["USERPROFILE", "HOME"] {
            if let Ok(home) = env::var(key) {
                let home = home.replace('\\', "/").trim_end_matches('/').to_lowercase();
                if !home.is_empty() && normalized.starts_with(&home) {
                    return true;
                }
            }
        }





        let clearly_non_sensitive = [
            "/projects/",
            "/project/",
            "/repos/",
            "/repo/",
            "/source/",
            "/src/",
            "/code/",
            "/workspace/",
            "/workspaces/",
            "/development/",
            "/dev/",
            "/build/",
            "/dist/",
            "/tmp/",
            "/temp/",
        ];
        !clearly_non_sensitive
            .iter()
            .any(|needle| normalized.contains(needle))
    }
}

fn normalize_without_existing(path: &Path) -> PathBuf {
    let mut output = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                output.pop();
            }
            std::path::Component::CurDir => {}
            other => output.push(other.as_os_str()),
        }
    }
    output
}

pub fn command_requires_confirmation(command: &str) -> bool {
    let lower = command.to_lowercase();
    [
        "rm -rf",
        "del /s",
        "format ",
        "git push --force",
        "git reset --hard",
        "drop table",
        "shutdown",
        "reboot",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn command_needs_manual_approval(command: &str) -> bool {
    let lower = command.trim().to_lowercase();
    if lower.is_empty() || command_requires_confirmation(&lower) {
        return true;
    }


    let risky = [
        "powershell",
        "pwsh",
        "cmd /c",
        "sh -c",
        "bash -c",
        "zsh -c",
        "curl ",
        "wget ",
        "invoke-webrequest",
        "iwr ",
        "scp ",
        "ssh ",
        "rclone ",
        "robocopy ",
        "xcopy ",
        "reg ",
        "reg.exe",
        "schtasks",
        "sudo ",
        "runas ",
        "chmod ",
        "chown ",
    ];
    if risky.iter().any(|needle| lower.contains(needle)) {
        return true;
    }


    if lower.contains(":\\")
        || lower.contains(":/")
        || lower.contains(" /home/")
        || lower.contains(" /users/")
        || lower.contains(" /etc/")
        || lower.contains(" /root/")
    {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn virtual_workspace_aliases_resolve_to_the_selected_root() {
        let sandbox = WorkspaceSandbox::new(".", ApprovalMode::Workspace).unwrap();
        assert_eq!(
            sandbox.resolve("$workspace", false).unwrap(),
            sandbox.root()
        );
        assert_eq!(
            sandbox.resolve("/workspace", false).unwrap(),
            sandbox.root()
        );
        assert_eq!(
            sandbox.resolve("/workspace/src/main.rs", false).unwrap(),
            normalize_without_existing(&sandbox.root().join("src/main.rs"))
        );
    }
}
