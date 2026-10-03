//! Where the token lives: the OS keychain, and nowhere else (DESIGN.md §5.7).
//!
//! Owns reading, writing and clearing one secret. It never decides *when* to do any of that, and
//! there is deliberately no way to ask it for a path — a token that can be pointed at a file is
//! a token that will one day be committed.
//!
//! A trait with two implementations, which is not ceremony: a CI container has no Secret Service
//! and no Keychain, so the flow above it has to be testable without one. [`MemoryStore`] is that,
//! and it forgets everything when the process ends, which is the only honest thing a fake store
//! can do.

use crate::GitHubError;

/// One secret, read and written by whoever can unlock the machine.
///
/// `Send + Sync` because the app holds one behind a `Mutex` and the sign-in task runs on another
/// thread (`spawn_blocking`, as builds do).
pub trait SecretStore: Send + Sync {
    /// Keep this token, replacing any token already kept.
    fn save(&self, token: &str) -> Result<(), GitHubError>;
    /// The token, or `None` when nobody has signed in on this machine.
    fn read(&self) -> Result<Option<String>, GitHubError>;
    /// Forget it. Signing out must leave nothing behind, so a store with nothing in it is not an
    /// error here.
    fn clear(&self) -> Result<(), GitHubError>;
}

/// The real one: Credential Manager on Windows, Keychain on macOS, Secret Service on Linux.
pub struct Keychain {
    service: String,
    account: String,
}

impl Keychain {
    /// The app's own entry — [`crate::KEYCHAIN_SERVICE`] and [`crate::KEYCHAIN_ACCOUNT`].
    pub fn for_this_app() -> Self {
        Self {
            service: crate::KEYCHAIN_SERVICE.to_string(),
            account: crate::KEYCHAIN_ACCOUNT.to_string(),
        }
    }

    /// A named entry, so a test can use one that is not the real app's.
    pub fn named(service: &str, account: &str) -> Self {
        Self {
            service: service.to_string(),
            account: account.to_string(),
        }
    }

    /// `keyring` opens the entry lazily, and every operation can fail for the same reason (no
    /// keychain service on this machine), so the handle is built per call and the error is
    /// translated in one place.
    fn entry(&self) -> Result<keyring::Entry, GitHubError> {
        keyring::Entry::new(&self.service, &self.account)
            .map_err(|error| GitHubError::Keychain(error.to_string()))
    }
}

impl SecretStore for Keychain {
    fn save(&self, token: &str) -> Result<(), GitHubError> {
        self.entry()?
            .set_password(token)
            .map_err(|error| GitHubError::Keychain(error.to_string()))
    }

    fn read(&self) -> Result<Option<String>, GitHubError> {
        match self.entry()?.get_password() {
            Ok(token) => Ok(Some(token)),
            // "Nobody has signed in" is the commonest state of this entry and not a failure.
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(GitHubError::Keychain(error.to_string())),
        }
    }

    fn clear(&self) -> Result<(), GitHubError> {
        match self.entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(GitHubError::Keychain(error.to_string())),
        }
    }
}

/// A store that keeps the token in memory for as long as the process lives.
///
/// For tests, and for nothing else: it is `pub` because the tests of crates *above* this one
/// need it too, and a second copy of it in each of them would be worse. It is not a fallback for
/// a machine with no keychain — "we could not store this securely, so we kept it somewhere else"
/// is exactly the decision §5.7 forbids being made quietly.
#[derive(Default)]
pub struct MemoryStore {
    token: std::sync::Mutex<Option<String>>,
}

impl SecretStore for MemoryStore {
    fn save(&self, token: &str) -> Result<(), GitHubError> {
        *self.token.lock().unwrap() = Some(token.to_string());
        Ok(())
    }

    fn read(&self) -> Result<Option<String>, GitHubError> {
        Ok(self.token.lock().unwrap().clone())
    }

    fn clear(&self) -> Result<(), GitHubError> {
        *self.token.lock().unwrap() = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_memory_store_keeps_reads_and_forgets() {
        let store = MemoryStore::default();
        assert_eq!(store.read().unwrap(), None);
        store.save("gho_one").unwrap();
        assert_eq!(store.read().unwrap().as_deref(), Some("gho_one"));
        // Signing in again replaces, rather than adding a second token nobody can choose between.
        store.save("gho_two").unwrap();
        assert_eq!(store.read().unwrap().as_deref(), Some("gho_two"));
        store.clear().unwrap();
        assert_eq!(store.read().unwrap(), None);
        // Signing out twice is not an error.
        store.clear().unwrap();
    }

    /// The real keychain, on the machine running the test.
    ///
    /// `#[ignore]`d for the reason every test in this workspace that needs something we do not
    /// ship is: a CI container has no Secret Service, and a developer's session may have one
    /// that prompts. It uses its own entry name, and deletes it however the test ends.
    ///
    /// Verified on the maintainer's Linux machine, 29 September 2026: saved, read back and
    /// deleted through the Secret Service.
    #[test]
    #[ignore]
    fn the_real_keychain_round_trips_a_token_and_forgets_it() {
        let store = Keychain::named("abstract-tex-test", "github-token-test");
        let _ = store.clear(); // whatever a previous run left

        store.save("gho_a_test_token").unwrap();
        assert_eq!(store.read().unwrap().as_deref(), Some("gho_a_test_token"));
        store.clear().unwrap();
        assert_eq!(
            store.read().unwrap(),
            None,
            "a signed-out machine must hold nothing"
        );
    }
}
