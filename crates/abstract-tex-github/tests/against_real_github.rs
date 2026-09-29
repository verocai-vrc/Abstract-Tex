//! S10.4a: the one thing a fake GitHub cannot check — that github.com behaves as documented.
//!
//! `#[ignore]`d, like every test here that needs something we do not ship: this one needs the
//! network. `cargo test -p abstract-tex-github --test against_real_github -- --ignored`.
//!
//! It signs nobody in, and it cannot: a real code request needs a registered OAuth app, and
//! nobody has registered one for Abstract-Tex yet (the ledger holds that item). What it *can*
//! prove is the half that breaks silently — that the request this crate builds is one GitHub
//! understands. A malformed request and an unregistered client id fail in completely different
//! ways, and only one of them is our problem:
//!
//! - wrong URL, wrong method, wrong content type → an HTML error page, which comes back as
//!   `Unreadable`;
//! - a well-formed request with a client id GitHub has never seen → GitHub's own JSON error,
//!   which comes back as `GitHub(..)`.
//!
//! So `GitHub(..)` is the passing outcome, and that is worth a test rather than a note.
//!
//! Run on this Linux machine, 29 September 2026: GitHub answered `{"error":"Not Found"}` with a
//! 404 to a `Iv1.0000000000000000` client id — JSON, from GitHub, parsed into a sentence.

use abstract_tex_github::{DeviceFlow, GitHubError};

/// A client id of the right shape that belongs to no app.
const NOBODYS_CLIENT_ID: &str = "Iv1.0000000000000000";

#[test]
#[ignore]
fn github_understands_the_request_this_crate_builds() {
    let flow = DeviceFlow::new().unwrap();
    let error = flow.request_code(NOBODYS_CLIENT_ID).expect_err("nobody owns that client id");

    match error {
        // The outcome this test is for: GitHub read the request and answered about the client id.
        GitHubError::GitHub(sentence) => {
            assert!(!sentence.is_empty(), "GitHub's error should carry words");
        }
        // Anything else means either the request shape is wrong (which is the bug this catches)
        // or the machine has no network (which is not a result).
        GitHubError::Network(error) => {
            eprintln!("skipped: no network to github.com ({error})");
        }
        other => panic!("GitHub did not recognise the request this crate builds: {other}"),
    }
}

/// The same for the token endpoint, which has its own URL, its own grant type and its own way of
/// being wrong.
#[test]
#[ignore]
fn github_understands_the_poll_this_crate_builds() {
    let flow = DeviceFlow::new().unwrap();
    let outcome = flow.poll(NOBODYS_CLIENT_ID, "a-device-code-that-was-never-issued");

    match outcome {
        Err(GitHubError::GitHub(sentence)) => assert!(!sentence.is_empty()),
        // A never-issued device code is exactly the case GitHub answers `expired_token` or
        // `access_denied` to, and both are this crate's own typed answers rather than a slug.
        Err(GitHubError::Expired) | Err(GitHubError::Denied) => {}
        Err(GitHubError::Network(error)) => eprintln!("skipped: no network to github.com ({error})"),
        other => panic!("GitHub did not recognise the poll this crate builds: {other:?}"),
    }
}
