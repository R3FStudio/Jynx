use anyhow::Result;
use std::path::{Path, PathBuf};

pub struct Discovery {
    file: PathBuf,
}
impl Discovery {
    pub fn new(home: &Path) -> Result<Self> {
        let directory = home.join("running");
        std::fs::create_dir_all(&directory)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))?;
        }
        let instance = Self {
            file: directory.join(format!("cli-{}.json", std::process::id())),
        };
        instance.update(None, "", 32768)?;
        Ok(instance)
    }
    pub fn update(&self, endpoint: Option<&str>, model: &str, context_size: usize) -> Result<()> {
        let mut record = serde_json::json!({"version":1,"app":"jynx-cli","api":"openai-compatible","pid":std::process::id(),"model":model,"contextSize":context_size});
        if let Some(endpoint) = endpoint {
            if let Ok(mut url) = reqwest::Url::parse(endpoint) {
                if matches!(url.scheme(), "http" | "https")
                    && matches!(
                        url.host_str(),
                        Some("127.0.0.1" | "localhost" | "[::1]" | "::1")
                    )
                    && url.username().is_empty()
                    && url.password().is_none()
                    && url.query().is_none()
                    && url.fragment().is_none()
                {
                    if url.path() == "/" || url.path().is_empty() {
                        url.set_path("/v1");
                    }
                    record["baseUrl"] =
                        serde_json::Value::String(url.as_str().trim_end_matches('/').to_owned());
                }
            }
        }
        let temporary = self.file.with_extension("tmp");
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        use std::io::Write;
        let mut file = options.open(&temporary)?;
        file.write_all(serde_json::to_string(&record)?.as_bytes())?;
        drop(file);
        std::fs::rename(temporary, &self.file)?;
        Ok(())
    }
}
impl Drop for Discovery {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.file);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn discovery_exposes_only_loopback_endpoints_and_cleans_up() {
        let directory =
            std::env::temp_dir().join(format!("jynx-cli-discovery-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let file = directory
            .join("running")
            .join(format!("cli-{}.json", std::process::id()));
        {
            let discovery = Discovery::new(&directory).unwrap();
            discovery
                .update(Some("https://api.example.com/v1"), "model", 64000)
                .unwrap();
            let record: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
            assert!(record.get("baseUrl").is_none());
            assert!(record.get("apiKey").is_none());
            discovery
                .update(Some("http://127.0.0.1:8080"), "model", 64000)
                .unwrap();
            let record: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
            assert_eq!(record["baseUrl"], "http://127.0.0.1:8080/v1");
            assert_eq!(record["contextSize"], 64000);
        }
        assert!(!file.exists());
        std::fs::remove_dir_all(directory).unwrap();
    }
}
