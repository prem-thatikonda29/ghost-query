use crate::providers::{preset, CUSTOM_ID};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

const DEFAULT_ACTIVE: &str = "openrouter";

/// Per-provider, non-secret settings. API keys live in the OS keychain, never here.
/// `has_key` is only a flag so the UI never has to touch the keychain just to render.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ProviderConfig {
    /// Only used by the custom provider; presets always use their built-in URL.
    pub base_url: String,
    pub model: String,
    pub has_key: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub active: String,
    pub providers: BTreeMap<String, ProviderConfig>,
}

impl Default for Settings {
    fn default() -> Self {
        Self { active: DEFAULT_ACTIVE.to_string(), providers: BTreeMap::new() }
    }
}

/// What a request needs, fully resolved.
#[derive(Debug, Clone)]
pub struct Active {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub model: String,
    pub needs_key: bool,
}

impl Settings {
    /// Stored values layered over the preset's defaults. `None` for unknown ids.
    pub fn resolved(&self, id: &str) -> Option<ProviderConfig> {
        let preset = preset(id)?;
        let stored = self.providers.get(id).cloned().unwrap_or_default();
        Some(ProviderConfig {
            base_url: if id == CUSTOM_ID {
                stored.base_url
            } else {
                preset.base_url.to_string()
            },
            model: if stored.model.is_empty() {
                preset.default_model.to_string()
            } else {
                stored.model
            },
            has_key: stored.has_key,
        })
    }

    pub fn active(&self) -> Result<Active, String> {
        let preset = preset(&self.active).ok_or_else(|| format!("Unknown provider: {}", self.active))?;
        let config = self.resolved(&self.active).expect("preset exists");
        if config.base_url.is_empty() {
            return Err(format!("{}: set a base URL in Settings.", preset.name));
        }
        if config.model.trim().is_empty() {
            return Err(format!("{}: choose a model in Settings.", preset.name));
        }
        Ok(Active {
            id: preset.id.to_string(),
            name: preset.name.to_string(),
            base_url: config.base_url,
            model: config.model,
            needs_key: preset.needs_key,
        })
    }

    /// Save a provider's model (and, for the custom provider, its base URL).
    pub fn update_provider(&mut self, id: &str, base_url: &str, model: &str) -> Result<(), String> {
        if preset(id).is_none() {
            return Err(format!("Unknown provider: {id}"));
        }
        let model = model.trim();
        if model.is_empty() {
            return Err("Model ID must not be empty".to_string());
        }
        let entry = self.providers.entry(id.to_string()).or_default();
        entry.model = model.to_string();
        if id == CUSTOM_ID {
            let url = base_url.trim().trim_end_matches('/');
            if !(url.starts_with("https://") || url.starts_with("http://")) {
                return Err("Base URL must start with http:// or https://".to_string());
            }
            entry.base_url = url.to_string();
        }
        Ok(())
    }

    pub fn set_has_key(&mut self, id: &str, has_key: bool) {
        self.providers.entry(id.to_string()).or_default().has_key = has_key;
    }

    pub fn set_active(&mut self, id: &str) -> Result<(), String> {
        if preset(id).is_none() {
            return Err(format!("Unknown provider: {id}"));
        }
        self.active = id.to_string();
        Ok(())
    }

    /// Missing or unreadable files (including the old single-provider format) fall
    /// back to defaults.
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
    fn defaults_to_openrouter_with_a_chat_model() {
        let a = Settings::default().active().unwrap();
        assert_eq!(a.id, "openrouter");
        assert_eq!(a.base_url, "https://openrouter.ai/api/v1");
        assert!(a.model.ends_with(":free") && a.needs_key);
    }

    #[test]
    fn presets_without_a_default_model_require_choosing_one() {
        let mut s = Settings::default();
        s.set_active("groq").unwrap();
        assert!(s.active().unwrap_err().contains("choose a model"));
        s.update_provider("groq", "", "llama-x").unwrap();
        let a = s.active().unwrap();
        assert_eq!((a.base_url.as_str(), a.model.as_str()), ("https://api.groq.com/openai/v1", "llama-x"));
    }

    #[test]
    fn preset_urls_cannot_be_overridden() {
        let mut s = Settings::default();
        s.update_provider("openai", "https://evil.example/v1", "m").unwrap();
        assert_eq!(s.resolved("openai").unwrap().base_url, "https://api.openai.com/v1");
    }

    #[test]
    fn custom_provider_takes_and_normalizes_its_url() {
        let mut s = Settings::default();
        s.update_provider("custom", " https://agent.example/v1/ ", "m").unwrap();
        s.set_active("custom").unwrap();
        let a = s.active().unwrap();
        assert_eq!(a.base_url, "https://agent.example/v1");
        assert!(!a.needs_key);
    }

    #[test]
    fn rejects_bad_input() {
        let mut s = Settings::default();
        assert!(s.update_provider("custom", "ftp://x", "m").is_err());
        assert!(s.update_provider("custom", "http://x", "  ").is_err());
        assert!(s.update_provider("nope", "", "m").is_err());
        assert!(s.set_active("nope").is_err());
    }

    #[test]
    fn custom_without_url_is_an_actionable_error() {
        let mut s = Settings::default();
        s.set_active("custom").unwrap();
        assert!(s.active().unwrap_err().contains("base URL"));
    }

    #[test]
    fn has_key_flag_is_tracked_per_provider() {
        let mut s = Settings::default();
        s.set_has_key("groq", true);
        assert!(s.resolved("groq").unwrap().has_key);
        assert!(!s.resolved("openai").unwrap().has_key);
    }

    #[test]
    fn save_then_load_roundtrips_and_missing_file_gives_defaults() {
        let path = temp_path("settings.json");
        let mut s = Settings::default();
        s.update_provider("groq", "", "m").unwrap();
        s.set_active("groq").unwrap();
        s.save(&path).unwrap();
        assert_eq!(Settings::load(&path), s);
        std::fs::remove_file(&path).unwrap();
        assert_eq!(Settings::load(&path), Settings::default());
    }

    #[test]
    fn old_single_provider_or_corrupt_files_give_defaults() {
        let path = temp_path("old.json");
        std::fs::write(&path, r#"{"base_url":"https://openrouter.ai/api/v1","model":"x"}"#).unwrap();
        assert_eq!(Settings::load(&path), Settings::default());
        std::fs::write(&path, "{not json").unwrap();
        assert_eq!(Settings::load(&path), Settings::default());
        std::fs::remove_file(&path).unwrap();
    }
}
