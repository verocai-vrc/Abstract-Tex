//! Where a provider's API key lives: the OS keychain, and nowhere else (DESIGN.md §5.5).
//!
//! Owns reading, writing and clearing one secret per *slot*. It never decides when to do any of
//! that, and there is deliberately no way to ask it for a path — a key that can be pointed at a
//! file is a key that will one day be committed. Same shape, and same reasons for the trait, as
//! `abstract-tex-github`'s `store.rs`: a CI container has no Secret Service, so everything above
//! this has to be testable without one.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::AssistantError;

/// The keychain service every entry of this app is filed under (shared with the GitHub token's).
pub const KEYCHAIN_SERVICE: &str = "abstract-tex";

/// Keys by slot (see [`crate::Provider::key_slot`]), read and written by whoever can unlock the
/// machine. `Send + Sync` because the app holds one and calls it from `spawn_blocking`.
pub trait KeyStore: Send + Sync {
    /// Keep this key for the slot, replacing any already kept.
    fn save(&self, slot: &str, key: &str) -> Result<(), AssistantError>;
    /// The key, or `None` when nobody has saved one for this slot.
    fn read(&self, slot: &str) -> Result<Option<String>, AssistantError>;
    /// Forget it. A slot with nothing in it is not an error: removing a key must be repeatable.
    fn clear(&self, slot: &str) -> Result<(), AssistantError>;
}

/// The real one: Credential Manager on Windows, Keychain on macOS, Secret Service on Linux.
pub struct Keychain {
    service: String,
}

impl Keychain {
    pub fn for_this_app() -> Self {
        Self {
            service: KEYCHAIN_SERVICE.to_string(),
        }
    }

    /// A named service, so a test never touches the real app's entries.
    pub fn named(service: &str) -> Self {
        Self {
            service: service.to_string(),
        }
    }

    fn entry(&self, slot: &str) -> Result<keyring::Entry, AssistantError> {
        keyring::Entry::new(&self.service, &format!("assistant-key:{slot}"))
            .map_err(|error| AssistantError::Keychain(error.to_string()))
    }
}

impl KeyStore for Keychain {
    fn save(&self, slot: &str, key: &str) -> Result<(), AssistantError> {
        self.entry(slot)?
            .set_password(key)
            .map_err(|error| AssistantError::Keychain(error.to_string()))
    }

    fn read(&self, slot: &str) -> Result<Option<String>, AssistantError> {
        match self.entry(slot)?.get_password() {
            Ok(key) => Ok(Some(key)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(AssistantError::Keychain(error.to_string())),
        }
    }

    fn clear(&self, slot: &str) -> Result<(), AssistantError> {
        match self.entry(slot)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(AssistantError::Keychain(error.to_string())),
        }
    }
}

/// Keys held in memory for as long as the process lives. For tests only — `pub` because the tests
/// of crates above this one need it too. Not a fallback for a machine with no keychain: "we could
/// not store this securely, so we kept it somewhere else" is a decision nobody gets to make quietly.
#[derive(Default)]
pub struct MemoryKeyStore {
    keys: Mutex<HashMap<String, String>>,
}

impl KeyStore for MemoryKeyStore {
    fn save(&self, slot: &str, key: &str) -> Result<(), AssistantError> {
        self.keys
            .lock()
            .unwrap()
            .insert(slot.to_string(), key.to_string());
        Ok(())
    }

    fn read(&self, slot: &str) -> Result<Option<String>, AssistantError> {
        Ok(self.keys.lock().unwrap().get(slot).cloned())
    }

    fn clear(&self, slot: &str) -> Result<(), AssistantError> {
        self.keys.lock().unwrap().remove(slot);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_memory_store_keeps_one_key_per_slot() {
        let store = MemoryKeyStore::default();
        assert_eq!(store.read("anthropic").unwrap(), None);
        store.save("anthropic", "sk-one").unwrap();
        store
            .save("openai-compatible@localhost:11434", "none-needed")
            .unwrap();
        store.save("anthropic", "sk-two").unwrap(); // replaces, never accumulates
        assert_eq!(store.read("anthropic").unwrap().as_deref(), Some("sk-two"));
        assert_eq!(
            store
                .read("openai-compatible@localhost:11434")
                .unwrap()
                .as_deref(),
            Some("none-needed")
        );
        store.clear("anthropic").unwrap();
        store.clear("anthropic").unwrap(); // twice is fine
        assert_eq!(store.read("anthropic").unwrap(), None);
    }

    /// The real keychain, on the machine running the test. `#[ignore]`d for the reason the GitHub
    /// token's twin is: a CI container has no Secret Service, and a developer's session may have
    /// one that prompts. Own service name; deleted however the test ends.
    #[test]
    #[ignore]
    fn the_real_keychain_round_trips_a_key_and_forgets_it() {
        let store = Keychain::named("abstract-tex-test");
        let _ = store.clear("test-slot");
        store.save("test-slot", "sk-a-test-key").unwrap();
        assert_eq!(store.read("test-slot").unwrap().as_deref(), Some("sk-a-test-key"));
        store.clear("test-slot").unwrap();
        assert_eq!(store.read("test-slot").unwrap(), None);
    }
}
