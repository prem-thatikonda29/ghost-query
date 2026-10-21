use serde::{Deserialize, Serialize};
use std::path::Path;

pub const DEFAULT_BASE_URL: &str = "https://openrouter.ai/api/v1";
/// OpenRouter's router across its free models (listed in its public /models catalog).
pub const DEFAULT_MODEL: &str = "openrouter/free";

/// Non-secret provider settings. The API key lives in the OS keychain, never here.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// Up to and including `/v1`; `/chat/completions` is appended per request.
    pub base_url: String,
    pub model: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            base_url: DEFAULT_BASE_URL.to_string(),
            model: DEFAULT_MODEL.to_string(),
        }
    }
}

impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        let url = self.base_url.trim();
        if !(url.starts_with("https://") || url.starts_with("http://")) {
            return Err("Base URL must start with http:// or https://".to_string());
        }
        if self.model.trim().is_empty() {
            return Err("Model ID must not be empty".to_string());
        }
        Ok(())
    }

    pub fn normalized(&self) -> Self {
        Self {
            base_url: self.base_url.trim().trim_end_matches('/').to_string(),
            model: self.model.trim().to_string(),
        }
    }

    /// Missing or unreadable files fall back to defaults.
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("Could not create settings dir: {e}"))?;
        }
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, text).map_err(|e| format!("Could not save settings: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("ghost-query-test-{}-{name}", std::process::id()))
    }

    #[test]
    fn defaults_point_at_openrouter() {
        let s = Settings::default();
        assert_eq!(s.base_url, "https://openrouter.ai/api/v1");
        assert!(s.validate().is_ok());
    }

    #[test]
    fn rejects_bad_url_and_empty_model() {
        let bad_url = Settings { base_url: "ftp://x".into(), model: "m".into() };
        assert!(bad_url.validate().is_err());
        let no_model = Settings { base_url: "http://localhost:11434/v1".into(), model: "  ".into() };
        assert!(no_model.validate().is_err());
    }

    #[test]
    fn normalizes_whitespace_and_trailing_slash() {
        let s = Settings { base_url: " https://x.test/v1/ ".into(), model: " m ".into() };
        assert_eq!(s.normalized(), Settings { base_url: "https://x.test/v1".into(), model: "m".into() });
    }

    #[test]
    fn save_then_load_roundtrips_and_missing_file_gives_defaults() {
        let path = temp_path("settings.json");
        let s = Settings { base_url: "http://localhost:11434/v1".into(), model: "llama3".into() };
        s.save(&path).unwrap();
        assert_eq!(Settings::load(&path), s);
        std::fs::remove_file(&path).unwrap();
        assert_eq!(Settings::load(&path), Settings::default());
    }

    #[test]
    fn corrupt_file_gives_defaults() {
        let path = temp_path("corrupt.json");
        std::fs::write(&path, "{not json").unwrap();
        assert_eq!(Settings::load(&path), Settings::default());
        std::fs::remove_file(&path).unwrap();
    }
}
