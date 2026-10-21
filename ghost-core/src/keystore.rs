//! API key storage in the OS credential store (macOS Keychain, Windows Credential
//! Manager, Secret Service on Linux), one entry per provider. The key is never
//! written to disk by the app and is never sent to the frontend.

use keyring::Entry;

const SERVICE: &str = "com.ghostquery.app";
/// Account used before keys were per-provider; it held the OpenRouter key.
const LEGACY_ACCOUNT: &str = "provider-api-key";

fn entry(account: &str) -> Result<Entry, String> {
    Entry::new(SERVICE, account).map_err(|e| format!("Keychain unavailable: {e}"))
}

fn account(provider: &str) -> String {
    format!("key:{provider}")
}

fn read(account: &str) -> Result<Option<String>, String> {
    match entry(account)?.get_password() {
        Ok(key) => Ok(Some(key)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("Could not read key from keychain: {e}")),
    }
}

fn delete(account: &str) -> Result<(), String> {
    match entry(account)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("Could not remove key from keychain: {e}")),
    }
}

pub fn set(provider: &str, key: &str) -> Result<(), String> {
    entry(&account(provider))?
        .set_password(key)
        .map_err(|e| format!("Could not save key to keychain: {e}"))
}

pub fn get(provider: &str) -> Result<Option<String>, String> {
    match read(&account(provider))? {
        Some(key) => Ok(Some(key)),
        None if provider == "openrouter" => read(LEGACY_ACCOUNT),
        None => Ok(None),
    }
}

pub fn clear(provider: &str) -> Result<(), String> {
    delete(&account(provider))?;
    if provider == "openrouter" {
        delete(LEGACY_ACCOUNT)?;
    }
    Ok(())
}
