use crate::providers::{preset, CUSTOM_ID};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

const DEFAULT_ACTIVE: &str = "openrouter";

/// Host part of an http(s) URL, lowercased, without userinfo or port.
fn host_of(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let authority = rest.split(['/', '?', '#']).next()?;
    let authority = authority.rsplit('@').next()?;
    let host = if let Some(stripped) = authority.strip_prefix('[') {
        stripped.split(']').next()?
    } else {
        authority.split(':').next()?
    };
    (!host.is_empty()).then(|| host.to_lowercase())
}

/// Loopback, private-network and `.local` hosts, where plain http does not cross the
/// public internet.
fn is_local_host(host: &str) -> bool {
    use std::net::IpAddr;
    if host == "localhost" || host.ends_with(".localhost") || host.ends_with(".local") {
        return true;
    }
    match host.parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) => ip.is_loopback() || ip.is_private() || ip.is_link_local(),
        Ok(IpAddr::V6(ip)) => ip.is_loopback() || (ip.segments()[0] & 0xfe00) == 0xfc00,
        Err(_) => false,
    }
}

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
    /// Ask the OS to exclude the window from screenshots and screen sharing.
    pub hide_from_capture: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            active: DEFAULT_ACTIVE.to_string(),
            providers: BTreeMap::new(),
            hide_from_capture: true,
        }
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
            // The API key travels with every request, so keep it off plaintext public links.
            if url.starts_with("http://") && !host_of(url).is_some_and(|h| is_local_host(&h)) {
                return Err("Plain http:// is only allowed for local addresses (localhost, private networks); use https:// for anything else.".to_string());
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
    fn host_parsing_handles_ports_userinfo_and_ipv6() {
        assert_eq!(host_of("http://Localhost:11434/v1").as_deref(), Some("localhost"));
        assert_eq!(host_of("https://user:pw@example.com:8443/x?y#z").as_deref(), Some("example.com"));
        assert_eq!(host_of("http://[::1]:8080/v1").as_deref(), Some("::1"));
        assert_eq!(host_of("http:///v1"), None);
    }

    #[test]
    fn plain_http_is_limited_to_local_hosts() {
        let mut s = Settings::default();
        for ok in [
            "http://localhost:11434/v1",
            "http://127.0.0.1:1234/v1",
            "http://192.168.1.20:8080/v1",
            "http://10.0.0.5/v1",
            "http://172.16.3.4/v1",
            "http://[::1]:8080/v1",
            "http://my-box.local:9000/v1",
            "https://api.example.com/v1",
        ] {
            assert!(s.update_provider("custom", ok, "m").is_ok(), "{ok} should be allowed");
        }
        for bad in [
            "http://api.example.com/v1",
            "http://8.8.8.8/v1",
            "http://172.32.0.1/v1",
            "http://localhost.evil.com/v1",
            "http://user@evil.com:80/v1",
        ] {
            assert!(s.update_provider("custom", bad, "m").is_err(), "{bad} should be rejected");
        }
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
    fn capture_hiding_is_on_by_default_including_for_older_settings_files() {
        assert!(Settings::default().hide_from_capture);
        let path = temp_path("pre-capture.json");
        std::fs::write(&path, r#"{"active":"groq","providers":{}}"#).unwrap();
        let loaded = Settings::load(&path);
        assert_eq!(loaded.active, "groq");
        assert!(loaded.hide_from_capture);
        std::fs::remove_file(&path).unwrap();
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
