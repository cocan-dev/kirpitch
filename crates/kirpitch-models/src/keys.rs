// SPDX-License-Identifier: GPL-3.0-or-later

//! Where API keys are kept: the operating system keychain (Windows Credential
//! Manager, macOS Keychain, the Linux kernel keyring), never a file. A model
//! can also name an environment variable; that wins when it is set.

use std::collections::HashMap;
use std::sync::Mutex;

/// Name of the keychain entry group.
const SERVICE: &str = "kirpitch";

/// Why a key could not be read or written.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("the keychain says: {0}")]
pub struct KeyError(pub String);

/// A place that keeps keys by account name (the model name).
pub trait KeyStore: Send + Sync {
    /// The key of an account, `None` when there is none.
    fn get(&self, account: &str) -> Result<Option<String>, KeyError>;
    /// Stores or replaces a key.
    fn set(&self, account: &str, key: &str) -> Result<(), KeyError>;
    /// Removes a key; removing a missing key is fine.
    fn delete(&self, account: &str) -> Result<(), KeyError>;
}

/// The keychain of the operating system.
#[derive(Debug, Default, Clone, Copy)]
pub struct OsKeyStore;

fn entry(account: &str) -> Result<keyring::Entry, KeyError> {
    keyring::Entry::new(SERVICE, account).map_err(|e| KeyError(e.to_string()))
}

impl KeyStore for OsKeyStore {
    fn get(&self, account: &str) -> Result<Option<String>, KeyError> {
        match entry(account)?.get_password() {
            Ok(key) => Ok(Some(key)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(KeyError(e.to_string())),
        }
    }

    fn set(&self, account: &str, key: &str) -> Result<(), KeyError> {
        entry(account)?
            .set_password(key)
            .map_err(|e| KeyError(e.to_string()))
    }

    fn delete(&self, account: &str) -> Result<(), KeyError> {
        match entry(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(KeyError(e.to_string())),
        }
    }
}

/// Keys in memory, for tests.
#[derive(Debug, Default)]
pub struct MemoryKeyStore(Mutex<HashMap<String, String>>);

impl MemoryKeyStore {
    /// An empty store.
    pub fn new() -> MemoryKeyStore {
        MemoryKeyStore::default()
    }
}

impl KeyStore for MemoryKeyStore {
    fn get(&self, account: &str) -> Result<Option<String>, KeyError> {
        Ok(self
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(account)
            .cloned())
    }

    fn set(&self, account: &str, key: &str) -> Result<(), KeyError> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(account.to_string(), key.to_string());
        Ok(())
    }

    fn delete(&self, account: &str) -> Result<(), KeyError> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(account);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Needs a real keychain, so it only runs on request:
    /// `cargo test -p kirpitch-models keychain -- --ignored`.
    #[test]
    #[ignore = "uses the operating system keychain"]
    fn the_os_keychain_keeps_and_deletes_a_key() {
        let s = OsKeyStore;
        let account = format!("kirpitch-test-{}", std::process::id());
        assert_eq!(s.get(&account), Ok(None));
        s.set(&account, "one").unwrap();
        assert_eq!(s.get(&account), Ok(Some("one".into())));
        s.delete(&account).unwrap();
        s.delete(&account).unwrap();
        assert_eq!(s.get(&account), Ok(None));
    }

    #[test]
    fn the_memory_store_keeps_replaces_and_deletes() {
        let s = MemoryKeyStore::new();
        assert_eq!(s.get("m"), Ok(None));
        s.set("m", "one").unwrap();
        s.set("m", "two").unwrap();
        assert_eq!(s.get("m"), Ok(Some("two".into())));
        s.delete("m").unwrap();
        s.delete("m").unwrap();
        assert_eq!(s.get("m"), Ok(None));
    }
}
