//! API key storage in the OS credential store (macOS Keychain, Windows Credential
//! Manager, Secret Service on Linux), one entry per provider. The key is never
//! written to disk by the app and is never sent to the frontend.
//!
//! Keychain reads can show a password prompt (macOS does when the requesting app is
//! not on the item's access list, which is always true for unsigned development
//! builds), so each key is read from the keychain at most once per app launch and
//! then served from memory.

use keyring::Entry;
use std::collections::HashMap;
use std::sync::Mutex;

const SERVICE: &str = "com.ghostquery.app";
/// Account used before keys were per-provider; it held the OpenRouter key.
const LEGACY_ACCOUNT: &str = "provider-api-key";
const LEGACY_PROVIDER: &str = "openrouter";

fn account(provider: &str) -> String {
    format!("key:{provider}")
}

/// Raw access to named secrets, so the caching and migration logic can be tested
/// without touching a real keychain.
pub trait Backend: Send + Sync {
    fn get(&self, account: &str) -> Result<Option<String>, String>;
    fn set(&self, account: &str, secret: &str) -> Result<(), String>;
    fn delete(&self, account: &str) -> Result<(), String>;
}

/// The operating system's credential store.
pub struct Keychain;

impl Keychain {
    fn entry(account: &str) -> Result<Entry, String> {
        Entry::new(SERVICE, account).map_err(|e| format!("Keychain unavailable: {e}"))
    }
}

impl Backend for Keychain {
    fn get(&self, account: &str) -> Result<Option<String>, String> {
        match Self::entry(account)?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(format!("Could not read key from keychain: {e}")),
        }
    }

    fn set(&self, account: &str, secret: &str) -> Result<(), String> {
        Self::entry(account)?
            .set_password(secret)
            .map_err(|e| format!("Could not save key to keychain: {e}"))
    }

    fn delete(&self, account: &str) -> Result<(), String> {
        match Self::entry(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(format!("Could not remove key from keychain: {e}")),
        }
    }
}

pub struct KeyStore<B: Backend = Keychain> {
    backend: B,
    cache: Mutex<HashMap<String, String>>,
}

impl KeyStore<Keychain> {
    pub fn new() -> Self {
        Self::with_backend(Keychain)
    }
}

impl Default for KeyStore<Keychain> {
    fn default() -> Self {
        Self::new()
    }
}

impl<B: Backend> KeyStore<B> {
    pub fn with_backend(backend: B) -> Self {
        Self { backend, cache: Mutex::new(HashMap::new()) }
    }

    /// The provider's key, from memory if it was already read this launch. A missing key
    /// is not cached, so a key saved later is found.
    pub fn get(&self, provider: &str) -> Result<Option<String>, String> {
        if let Some(key) = self.cache.lock().unwrap().get(provider) {
            return Ok(Some(key.clone()));
        }
        let mut found = self.backend.get(&account(provider))?;
        if found.is_none() && provider == LEGACY_PROVIDER {
            found = self.migrate_legacy()?;
        }
        if let Some(key) = &found {
            self.cache.lock().unwrap().insert(provider.to_string(), key.clone());
        }
        Ok(found)
    }

    /// Moves a key saved by an earlier version (single shared account) to the
    /// per-provider account. Done once; afterwards the old item is never read again, so
    /// it cannot keep prompting.
    fn migrate_legacy(&self) -> Result<Option<String>, String> {
        let Some(key) = self.backend.get(LEGACY_ACCOUNT)? else {
            return Ok(None);
        };
        self.backend.set(&account(LEGACY_PROVIDER), &key)?;
        // Best effort: if removing fails the key still works from its new home.
        let _ = self.backend.delete(LEGACY_ACCOUNT);
        Ok(Some(key))
    }

    pub fn set(&self, provider: &str, key: &str) -> Result<(), String> {
        self.backend.set(&account(provider), key)?;
        self.cache.lock().unwrap().insert(provider.to_string(), key.to_string());
        Ok(())
    }

    pub fn clear(&self, provider: &str) -> Result<(), String> {
        self.backend.delete(&account(provider))?;
        if provider == LEGACY_PROVIDER {
            self.backend.delete(LEGACY_ACCOUNT)?;
        }
        self.cache.lock().unwrap().remove(provider);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Default)]
    struct Fake {
        items: Mutex<HashMap<String, String>>,
        reads: AtomicUsize,
    }

    impl Fake {
        fn with(items: &[(&str, &str)]) -> Self {
            let fake = Fake::default();
            for (k, v) in items {
                fake.items.lock().unwrap().insert(k.to_string(), v.to_string());
            }
            fake
        }
        fn reads(&self) -> usize {
            self.reads.load(Ordering::SeqCst)
        }
        fn has(&self, account: &str) -> bool {
            self.items.lock().unwrap().contains_key(account)
        }
    }

    impl Backend for Fake {
        fn get(&self, account: &str) -> Result<Option<String>, String> {
            self.reads.fetch_add(1, Ordering::SeqCst);
            Ok(self.items.lock().unwrap().get(account).cloned())
        }
        fn set(&self, account: &str, secret: &str) -> Result<(), String> {
            self.items.lock().unwrap().insert(account.to_string(), secret.to_string());
            Ok(())
        }
        fn delete(&self, account: &str) -> Result<(), String> {
            self.items.lock().unwrap().remove(account);
            Ok(())
        }
    }

    #[test]
    fn a_key_is_read_from_the_keychain_only_once_per_launch() {
        let store = KeyStore::with_backend(Fake::with(&[("key:groq", "gsk-1")]));
        for _ in 0..20 {
            assert_eq!(store.get("groq").unwrap().as_deref(), Some("gsk-1"));
        }
        assert_eq!(store.backend.reads(), 1, "every message must not hit the keychain");
    }

    #[test]
    fn saving_a_key_serves_it_from_memory_without_reading_back() {
        let store = KeyStore::with_backend(Fake::default());
        store.set("openai", "sk-new").unwrap();
        assert_eq!(store.get("openai").unwrap().as_deref(), Some("sk-new"));
        assert_eq!(store.backend.reads(), 0);
        store.set("openai", "sk-newer").unwrap();
        assert_eq!(store.get("openai").unwrap().as_deref(), Some("sk-newer"));
    }

    #[test]
    fn removing_a_key_forgets_it_everywhere() {
        let store = KeyStore::with_backend(Fake::with(&[("key:groq", "gsk-1")]));
        assert!(store.get("groq").unwrap().is_some());
        store.clear("groq").unwrap();
        assert_eq!(store.get("groq").unwrap(), None);
        assert!(!store.backend.has("key:groq"));
    }

    #[test]
    fn a_missing_key_is_not_cached_so_a_later_save_is_found() {
        let store = KeyStore::with_backend(Fake::default());
        assert_eq!(store.get("mistral").unwrap(), None);
        store.backend.set("key:mistral", "m-1").unwrap();
        assert_eq!(store.get("mistral").unwrap().as_deref(), Some("m-1"));
    }

    #[test]
    fn the_old_single_account_key_is_migrated_once_and_then_left_alone() {
        let store = KeyStore::with_backend(Fake::with(&[(LEGACY_ACCOUNT, "sk-or-old")]));
        assert_eq!(store.get("openrouter").unwrap().as_deref(), Some("sk-or-old"));
        assert!(store.backend.has("key:openrouter"), "copied to the per-provider account");
        assert!(!store.backend.has(LEGACY_ACCOUNT), "old item removed so it cannot prompt again");

        // A fresh launch reads only the new item.
        let new_launch = KeyStore::with_backend(Fake::with(&[("key:openrouter", "sk-or-old")]));
        assert_eq!(new_launch.get("openrouter").unwrap().as_deref(), Some("sk-or-old"));
        assert_eq!(new_launch.backend.reads(), 1);
    }

    #[test]
    fn only_the_openrouter_key_ever_looks_at_the_old_account() {
        let store = KeyStore::with_backend(Fake::with(&[(LEGACY_ACCOUNT, "sk-or-old")]));
        assert_eq!(store.get("groq").unwrap(), None);
        assert!(store.backend.has(LEGACY_ACCOUNT));
    }

    #[test]
    fn removing_the_openrouter_key_also_removes_the_old_item() {
        let store = KeyStore::with_backend(Fake::with(&[(LEGACY_ACCOUNT, "sk-or-old")]));
        store.clear("openrouter").unwrap();
        assert!(!store.backend.has(LEGACY_ACCOUNT));
        assert_eq!(store.get("openrouter").unwrap(), None);
    }
}
