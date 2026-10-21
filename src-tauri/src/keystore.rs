//! API key storage in the OS credential store (macOS Keychain, Windows Credential
//! Manager, Secret Service on Linux). The key is never written to disk by the app
//! and is never sent to the frontend; only `has_key` is exposed to it.

use keyring::Entry;

const SERVICE: &str = "com.ghostquery.app";
const ACCOUNT: &str = "provider-api-key";

fn entry() -> Result<Entry, String> {
    Entry::new(SERVICE, ACCOUNT).map_err(|e| format!("Keychain unavailable: {e}"))
}

pub fn set(key: &str) -> Result<(), String> {
    entry()?
        .set_password(key)
        .map_err(|e| format!("Could not save key to keychain: {e}"))
}

pub fn get() -> Result<Option<String>, String> {
    match entry()?.get_password() {
        Ok(key) => Ok(Some(key)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("Could not read key from keychain: {e}")),
    }
}

pub fn clear() -> Result<(), String> {
    match entry()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("Could not remove key from keychain: {e}")),
    }
}
