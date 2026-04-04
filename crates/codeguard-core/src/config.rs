use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize, Default, Clone)]
#[serde(default)]
pub struct Config {
    pub select: Option<Vec<String>>,
    pub ignore: Vec<String>,
    pub strict: bool,
    pub offline: bool,
    pub fix: bool,
    pub verbose: bool,
    pub python_executable: Option<PathBuf>,
    pub cache_dir: Option<PathBuf>,
    pub pypi_cache_ttl_secs: Option<u64>,
}

impl Config {
    pub fn cache_dir(&self) -> PathBuf {
        self.cache_dir.clone().unwrap_or_else(|| {
            dirs_or_default().join("codeguard")
        })
    }

    pub fn pypi_cache_ttl(&self) -> u64 {
        self.pypi_cache_ttl_secs.unwrap_or(86400)
    }

    pub fn python_exec(&self) -> &str {
        self.python_executable
            .as_ref()
            .and_then(|p| p.to_str())
            .unwrap_or("python3")
    }

    pub fn is_rule_enabled(&self, code: &str) -> bool {
        match &self.select {
            None => true,
            Some(selectors) => selectors.iter().any(|s| {
                let s_upper = s.to_uppercase();
                code.to_uppercase().starts_with(&s_upper)
            }),
        }
    }

    pub fn load_from_file(path: &Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    pub fn discover(project_root: &Path) -> Self {
        let candidates = [
            project_root.join("codeguard.toml"),
            project_root.join(".codeguard.toml"),
        ];
        for path in &candidates {
            if path.exists() {
                if let Ok(config) = Self::load_from_file(path) {
                    return config;
                }
            }
        }
        Self::default()
    }
}

fn dirs_or_default() -> PathBuf {
    if let Some(cache) = std::env::var_os("XDG_CACHE_HOME") {
        PathBuf::from(cache)
    } else if let Some(home) = std::env::var_os("HOME") {
        PathBuf::from(home).join(".cache")
    } else {
        PathBuf::from("/tmp")
    }
}
