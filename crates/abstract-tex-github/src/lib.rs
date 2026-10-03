//! What this app asks GitHub for: a sign-in, and a repository (S10.4a, S10.5b, DESIGN.md §5.7).
//!
//! Owns three things and no more: the OAuth **device flow** ([`device`]), the OS keychain the
//! token it earns is kept in ([`store`]), and creating a repository ([`repos`]). It knows nothing
//! about Tauri, nothing about a window, and nothing about Git — pushing with the token is
//! `abstract-tex-git`'s job, at S11.1.
//!
//! **Why the device flow and not a redirect.** A desktop binary cannot keep a secret: anything
//! compiled into it can be read out of it. The device flow is the OAuth grant designed for that
//! situation — the app shows a short code, the person types it into `github.com/login/device` in
//! their own browser, and the app polls until GitHub hands over a token. Nothing in the exchange
//! is confidential except the answer, which is why §5.7 says "no client secret ever ships in a
//! desktop binary".
//!
//! **What this crate must never do:**
//!
//! - Never write a token anywhere but a [`store::SecretStore`]. Not a config file, not
//!   `abstract-tex.toml`, not a log line — §5.7 says the keychain and means it, and a token in a
//!   project file is a token in someone's repository.
//! - Never ask for a scope it does not need. [`SCOPE`] is one constant with its reasoning next
//!   to it, because a scope list is the kind of thing that grows by accident.
//! - Never invent a client id. A build without one says so ([`client_id`]); it does not fall
//!   back to somebody else's app.
//!
//! Errors are a `thiserror` enum, as `crates/abstract-tex-engine/src/lib.rs` explains once:
//! typed errors in a library, `anyhow` only at the app edge.

pub mod device;
pub mod repos;
pub mod store;

pub use device::{DeviceCode, DeviceFlow, Endpoints, Poll};
pub use repos::{NewRepository, Repos, Repository, Visibility};
pub use store::{Keychain, MemoryStore, SecretStore};

/// The HTTP client every call here uses.
///
/// One builder, shared, so that the two things GitHub is asked for cannot end up with different
/// timeouts or a different user agent — and so the reasons for both are written once.
pub(crate) fn http_client() -> Result<reqwest::blocking::Client, GitHubError> {
    Ok(reqwest::blocking::Client::builder()
        // GitHub's API rejects a request with no user agent, and one that names the app is what
        // their own documentation asks for.
        .user_agent(concat!("abstract-tex/", env!("CARGO_PKG_VERSION")))
        // A person is watching this happen. A request that hangs for a minute has failed as far
        // as they are concerned, and the sign-in's polling loop will try again anyway.
        .timeout(std::time::Duration::from_secs(20))
        .build()?)
}

/// The OAuth scope the app asks for, and the only one.
///
/// `repo` is the smallest scope that can create a *private* repository and push to it, which is
/// what §5.7 promises ("private by default, loudly"). Deliberately not `delete_repo`, not
/// `workflow`, not `admin:org`, not `gist`: an editor that can delete a repository is an editor
/// that can delete a manuscript, and nothing in the design asks for that.
///
/// `GET /user` needs no scope of its own for the signed-in account, so the login name in the
/// status bar costs nothing extra.
pub const SCOPE: &str = "repo";

/// What the keychain entry is called. One service, one account slot: this app holds at most one
/// GitHub identity at a time, and a second would need a way to choose between them that nothing
/// in the design asks for.
pub const KEYCHAIN_SERVICE: &str = "abstract-tex";
pub const KEYCHAIN_ACCOUNT: &str = "github-token";

/// The OAuth app's client id, or `None` in a build that has none.
///
/// Public, not secret — every desktop OAuth app ships one, and the device flow works because the
/// *token* is the secret and the id is not. It is compiled in from
/// `ABSTRACT_TEX_GITHUB_CLIENT_ID` and can be overridden at runtime by the same variable, which
/// is how a test or a fork points at its own app.
///
/// `None` is a real state and not a bug: nobody has registered an OAuth app for Abstract-Tex
/// yet (the ledger holds that item). The app says "sign-in is not configured in this build"
/// rather than reaching for someone else's client id.
pub fn client_id() -> Option<String> {
    if let Ok(from_environment) = std::env::var("ABSTRACT_TEX_GITHUB_CLIENT_ID") {
        if !from_environment.trim().is_empty() {
            return Some(from_environment);
        }
    }
    option_env!("ABSTRACT_TEX_GITHUB_CLIENT_ID")
        .map(str::to_string)
        .filter(|id| !id.trim().is_empty())
}

/// Who the token belongs to. Everything else `GET /user` returns is somebody's personal data we
/// have no use for, so it is not deserialised.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct Account {
    /// The `@handle`, which is what the status bar shows.
    pub login: String,
}

#[derive(Debug, thiserror::Error)]
pub enum GitHubError {
    /// No OAuth app is configured in this build. A sentence, not a failure to explain.
    #[error("Signing in to GitHub is not configured in this build of Abstract-Tex.")]
    NoClientId,

    /// GitHub answered, and what it said was an error. Its own words, because they are written
    /// for the person who has to act on them.
    #[error("GitHub said: {0}")]
    GitHub(String),

    #[error("Could not reach GitHub: {0}")]
    Network(#[from] reqwest::Error),

    /// GitHub answered with something this crate cannot read. Separate from [`Self::GitHub`]
    /// because it means *we* are wrong, not the person.
    #[error("GitHub's answer could not be understood: {0}")]
    Unreadable(String),

    #[error("The sign-in was refused on GitHub.")]
    Denied,

    #[error("The sign-in code expired. Starting again gets a new one.")]
    Expired,

    /// The token in the keychain is no longer good — revoked on github.com, most likely, which
    /// nothing tells the app about until it next tries to use it. The app's answer is to forget
    /// it and show the sign-in offer again, so this is its own variant (S10.4b).
    #[error("GitHub no longer accepts this sign-in. Signing in again fixes it.")]
    TokenRejected,

    /// A public repository was asked for without the confirmation DESIGN.md §5.7 requires having
    /// been answered. Not a message anyone should ever see: it is the guard that makes "private
    /// by default" a property of the code rather than of the panel that calls it.
    #[error("A public repository needs the confirmation to have been answered first.")]
    PublicNotConfirmed,

    /// The keychain refused. On Linux this usually means no Secret Service is running, which is
    /// worth saying plainly rather than as "platform error 0".
    #[error("The system keychain could not be used: {0}")]
    Keychain(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scope_is_the_one_scope() {
        // A test that exists to make widening the scope a deliberate act with a diff to explain.
        assert_eq!(SCOPE, "repo");
    }

    #[test]
    fn a_blank_client_id_is_no_client_id() {
        // `option_env!` of an unset variable is `None`; a variable set to whitespace, which is
        // what a half-finished CI configuration produces, must not become a client id either.
        // `set_var` is `unsafe` in current Rust because another thread reading the environment
        // at the same moment is undefined behaviour. Safe here: the only reader of this variable
        // is `client_id()`, and this is the only test in the crate that calls it.
        unsafe { std::env::set_var("ABSTRACT_TEX_GITHUB_CLIENT_ID", "   ") };
        assert_eq!(client_id(), None);
        unsafe { std::env::set_var("ABSTRACT_TEX_GITHUB_CLIENT_ID", "Iv1.abc") };
        assert_eq!(client_id().as_deref(), Some("Iv1.abc"));
        unsafe { std::env::remove_var("ABSTRACT_TEX_GITHUB_CLIENT_ID") };
    }
}
