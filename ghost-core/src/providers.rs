//! Built-in provider presets. Each is an OpenAI-compatible endpoint
//! (`{base_url}/chat/completions`); base URLs were probed to exist on 2026-10-21.
//! Default models are only set where verified; otherwise the user picks one from the
//! provider's `/models` list in Settings.

pub const CUSTOM_ID: &str = "custom";

pub struct Preset {
    pub id: &'static str,
    pub name: &'static str,
    pub base_url: &'static str,
    pub needs_key: bool,
    pub default_model: &'static str,
    pub key_hint: &'static str,
}

pub const PRESETS: &[Preset] = &[
    Preset {
        id: "openrouter",
        name: "OpenRouter",
        base_url: "https://openrouter.ai/api/v1",
        needs_key: true,
        // Not `openrouter/free`: that router can pick a content-safety classifier.
        default_model: "google/gemma-4-31b-it:free",
        key_hint: "sk-or-...",
    },
    Preset {
        id: "openai",
        name: "OpenAI",
        base_url: "https://api.openai.com/v1",
        needs_key: true,
        default_model: "",
        key_hint: "sk-...",
    },
    Preset {
        id: "anthropic",
        name: "Claude (Anthropic)",
        base_url: "https://api.anthropic.com/v1",
        needs_key: true,
        default_model: "",
        key_hint: "sk-ant-...",
    },
    Preset {
        id: "gemini",
        name: "Google Gemini",
        base_url: "https://generativelanguage.googleapis.com/v1beta/openai",
        needs_key: true,
        default_model: "",
        key_hint: "AIza...",
    },
    Preset {
        id: "groq",
        name: "Groq",
        base_url: "https://api.groq.com/openai/v1",
        needs_key: true,
        default_model: "",
        key_hint: "gsk_...",
    },
    Preset {
        id: "nvidia",
        name: "NVIDIA NIM",
        base_url: "https://integrate.api.nvidia.com/v1",
        needs_key: true,
        default_model: "",
        key_hint: "nvapi-...",
    },
    Preset {
        id: "mistral",
        name: "Mistral",
        base_url: "https://api.mistral.ai/v1",
        needs_key: true,
        default_model: "",
        key_hint: "",
    },
    Preset {
        id: "together",
        name: "Together AI",
        base_url: "https://api.together.xyz/v1",
        needs_key: true,
        default_model: "",
        key_hint: "",
    },
    Preset {
        id: "deepseek",
        name: "DeepSeek",
        base_url: "https://api.deepseek.com/v1",
        needs_key: true,
        default_model: "",
        key_hint: "sk-...",
    },
    Preset {
        id: "ollama",
        name: "Ollama (local)",
        base_url: "http://localhost:11434/v1",
        needs_key: false,
        default_model: "",
        key_hint: "",
    },
    // Your own endpoint (AgentRouter, a self-hosted gateway, ...): URL is user-supplied.
    Preset {
        id: CUSTOM_ID,
        name: "Custom (your own URL)",
        base_url: "",
        needs_key: false,
        default_model: "",
        key_hint: "",
    },
];

pub fn preset(id: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|p| p.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_urls_well_formed() {
        let mut ids: Vec<_> = PRESETS.iter().map(|p| p.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), PRESETS.len());
        for p in PRESETS.iter().filter(|p| p.id != CUSTOM_ID) {
            assert!(p.base_url.starts_with("http"), "{}", p.id);
            assert!(!p.base_url.ends_with('/'), "{}", p.id);
        }
    }

    #[test]
    fn lookup_works() {
        assert_eq!(preset("groq").unwrap().name, "Groq");
        assert!(preset("nope").is_none());
    }
}
