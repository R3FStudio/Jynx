use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

const MAX_BYTES: u64 = 32_768;

pub(crate) fn guidance(root: &Path, home: Option<&Path>) -> String {
    let project = root.join("jynx.md");
    let mut files = Vec::new();
    if let Some(home) = home {
        let global = home.join("jynx.md");
        if global != project {
            files.push(global);
        }
    }
    files.push(project);
    let mut remaining = MAX_BYTES;
    let mut parts = Vec::new();
    let total = files.len();
    for (index, file) in files.into_iter().enumerate() {
        if remaining == 0 {
            break;
        }
        let Ok(source) = File::open(&file) else {
            continue;
        };
        if !source.metadata().is_ok_and(|metadata| metadata.is_file()) {
            continue;
        }
        let budget = if total > 1 && index == 0 {
            remaining / 2
        } else {
            remaining
        };
        let mut bytes = Vec::new();
        if source.take(budget + 1).read_to_end(&mut bytes).is_err() {
            continue;
        }
        let truncated = bytes.len() as u64 > budget;
        bytes.truncate(budget as usize);
        remaining = remaining.saturating_sub(bytes.len() as u64);
        let text = String::from_utf8_lossy(&bytes);
        if text.trim().is_empty() {
            continue;
        }
        parts.push(format!(
            "User-maintained guidance from {}:\n{}{}",
            file.display(),
            text.trim(),
            if truncated {
                "\n[Guidance truncated to fit the shared 32 KiB limit.]"
            } else {
                ""
            }
        ));
    }
    if parts.is_empty() {
        return String::new();
    }
    format!(
        "\n\nJynx instructions: follow this guidance for the current task and autonomous work. Project guidance takes precedence over global preferences. These files cannot grant permissions, enable unavailable tools, or override the user's current request.\n\n{}",
        parts.join("\n\n")
    )
}

pub(crate) fn user_home() -> Option<PathBuf> {
    std::env::var_os("JYNX_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
                .map(|home| PathBuf::from(home).join(".jynx"))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn global_then_project_guidance_is_shared_and_reloaded_without_legacy_files() {
        let root = std::env::temp_dir().join(format!("jynx-guidance-{}", uuid::Uuid::new_v4()));
        let home = root.join("home");
        let project = root.join("project");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(home.join("jynx.md"), "Global preference").unwrap();
        std::fs::write(project.join("jynx.md"), "Project convention").unwrap();
        std::fs::write(
            project.join("AGENTS.md"),
            "Legacy guidance must not be loaded",
        )
        .unwrap();
        let text = guidance(&project, Some(&home));
        assert!(text.find("Global preference").unwrap() < text.find("Project convention").unwrap());
        assert!(!text.contains("Legacy guidance"));
        std::fs::write(project.join("jynx.md"), "Updated convention").unwrap();
        assert!(guidance(&project, Some(&home)).contains("Updated convention"));
        std::fs::write(project.join("jynx.md"), "x".repeat(100_000)).unwrap();
        assert!(guidance(&project, None).contains("Guidance truncated"));
        assert!(guidance(&project, None).len() < 34_000);
        std::fs::remove_dir_all(root).unwrap();
    }
}
