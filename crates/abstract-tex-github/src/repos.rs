//! Creating a repository on GitHub (S10.5b, DESIGN.md §5.7: "create the repository from inside
//! the app", "private by default, loudly"), and listing the ones an account can clone (S11.5b).
//!
//! Owns two calls on one URL — `POST /user/repos` and `GET /user/repos` — and the guard that makes the "private by default" part a
//! property of this code rather than of whatever panel calls it.
//!
//! **What it must never do:** never create a public repository unless it has been told, as data,
//! that the person was asked and said yes. §5.7's reason is worth restating where the code is:
//! *"unpublished manuscripts, embargoed results and unblinded data are the normal contents of
//! these folders"*. A default that can be flipped by a mistyped argument is not a default; hence
//! [`Visibility`], which cannot be constructed as public without the answer.

use serde::{Deserialize, Serialize};

use crate::GitHubError;

/// Whether the new repository is to be public, and — if it is — that somebody said so.
///
/// An enum and not a `bool`, because the two are not opposites here: private needs nothing, and
/// public needs a confirmation that has already happened. A caller cannot ask for public without
/// producing the answer, and [`Repos::create`] refuses it when the answer is `false`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Visibility {
    /// The only thing this app ever asks for on its own.
    Private,
    /// `confirmed` is the answer to the dialog §5.7 requires: it says what making an unpublished
    /// manuscript public means, in those words. `false` reaches here only from a bug, and is
    /// refused rather than ignored.
    #[serde(rename_all = "camelCase")]
    Public { confirmed: bool },
}

impl Visibility {
    fn is_private(self) -> bool {
        matches!(self, Visibility::Private)
    }

    /// The check that makes the rule the code's. Separate from the call so that it is tested
    /// without a network: the network half can only ever be tested against a fake, and *this* is
    /// the half that must not be wrong.
    pub fn allowed(self) -> Result<(), GitHubError> {
        match self {
            Visibility::Private | Visibility::Public { confirmed: true } => Ok(()),
            Visibility::Public { confirmed: false } => Err(GitHubError::PublicNotConfirmed),
        }
    }
}

/// What to make.
#[derive(Debug, Clone)]
pub struct NewRepository {
    /// GitHub's own rules apply to this; a name it will not take comes back as its own sentence
    /// ("name already exists on this account" is the usual one), which is more useful than
    /// anything this crate could check in advance.
    pub name: String,
    pub visibility: Visibility,
    /// Shown on the repository's page. `None` sends none, rather than sending something about
    /// Abstract-Tex into somebody's research output.
    pub description: Option<String>,
}

/// What came back: the repository, as Git and a browser each need to name it.
///
/// Deserialised from GitHub's `snake_case` and serialised to the frontend's `camelCase`: the two
/// halves of this struct's life speak different conventions, and `rename_all(serialize = …)` is
/// how serde says exactly that. Without it, GitHub's `full_name` reads as a missing field.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct Repository {
    /// `ada/thesis`, which is what the panel shows.
    pub full_name: String,
    /// The `https://` URL that becomes the local repository's `origin`.
    pub clone_url: String,
    /// Where a person goes to look at it.
    pub html_url: String,
    /// Read back from GitHub's answer rather than assumed from the request: if this is ever
    /// `false` when the app asked for private, something is very wrong and the panel must be able
    /// to say so.
    pub private: bool,
}

/// The one call this module makes.
pub struct Repos {
    client: reqwest::blocking::Client,
    endpoint: String,
}

impl Repos {
    pub fn new() -> Result<Self, GitHubError> {
        Self::with_endpoints(crate::Endpoints::github())
    }

    pub fn with_endpoints(endpoints: crate::Endpoints) -> Result<Self, GitHubError> {
        Ok(Self { client: crate::http_client()?, endpoint: endpoints.repos })
    }

    /// Create it, under the account the token belongs to.
    pub fn create(&self, token: &str, wanted: &NewRepository) -> Result<Repository, GitHubError> {
        wanted.visibility.allowed()?;

        let body = Request {
            name: &wanted.name,
            private: wanted.visibility.is_private(),
            description: wanted.description.as_deref(),
            // The repository is for a manuscript that already exists on disk, so GitHub must not
            // put a README, a licence or a `.gitignore` of its own in it: every one of those
            // would be a commit the local history does not have, and the first push would be
            // rejected as a non-fast-forward for reasons nobody could see.
            auto_init: false,
        };

        // Serialised here rather than with `reqwest`'s `.json()`, which needs a feature the
        // workspace's `reqwest` does not enable — and enabling it across the workspace for one
        // call would be a wider change than the call deserves.
        let body = serde_json::to_string(&body).map_err(|error| GitHubError::Unreadable(error.to_string()))?;
        let response = self
            .client
            .post(&self.endpoint)
            .bearer_auth(token)
            .header(reqwest::header::ACCEPT, "application/vnd.github+json")
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .body(body)
            .send()?;

        let text = answer_text(response)?;
        let created: Repository = serde_json::from_str(&text)
            .map_err(|_| GitHubError::Unreadable(text.chars().take(200).collect()))?;
        Ok(created)
    }

    /// The repositories this account can clone, most recently pushed first (S11.5b).
    ///
    /// Owned, collaborated-on and organisation repositories, because a coauthor's paper is
    /// usually the second kind. Capped at three pages (`MAX_LIST_PAGES`): a picker is for finding one
    /// project quickly, and an account with more than that is better served by the URL field the
    /// Clone window has beside it than by an ever longer wait.
    pub fn list(&self, token: &str) -> Result<Vec<Repository>, GitHubError> {
        let mut found = Vec::new();
        for page in 1..=MAX_LIST_PAGES {
            let response = self
                .client
                .get(&self.endpoint)
                .query(&[
                    ("per_page", LIST_PAGE_SIZE.to_string()),
                    ("page", page.to_string()),
                    ("sort", "pushed".to_string()),
                    ("affiliation", "owner,collaborator,organization_member".to_string()),
                ])
                .bearer_auth(token)
                .header(reqwest::header::ACCEPT, "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28")
                .send()?;
            let text = answer_text(response)?;
            let batch: Vec<Repository> = serde_json::from_str(&text)
                .map_err(|_| GitHubError::Unreadable(text.chars().take(200).collect()))?;
            let was_full_page = batch.len() == LIST_PAGE_SIZE;
            found.extend(batch);
            if !was_full_page {
                break;
            }
        }
        Ok(found)
    }
}

/// GitHub's own maximum for `per_page`.
const LIST_PAGE_SIZE: usize = 100;
/// Three hundred repositories is more than a person scrolls; see [`Repos::list`].
const MAX_LIST_PAGES: usize = 3;

/// The body of a successful answer, or the right error for an unsuccessful one: a revoked token
/// is its own variant (the app forgets it), and anything else carries GitHub's own sentence.
fn answer_text(response: reqwest::blocking::Response) -> Result<String, GitHubError> {
    let status = response.status();
    let text = response.text()?;
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(GitHubError::TokenRejected);
    }
    if !status.is_success() {
        return Err(GitHubError::GitHub(sentence_from(&text)));
    }
    Ok(text)
}

/// GitHub's error body, turned into one line.
///
/// `message` is the summary ("Repository creation failed.") and `errors[]` holds the part that
/// says what to change ("name already exists on this account"). Both, because either alone
/// leaves the person guessing.
fn sentence_from(body: &str) -> String {
    let Ok(error) = serde_json::from_str::<ErrorBody>(body) else {
        return body.chars().take(200).collect();
    };
    let detail = error.errors.into_iter().filter_map(|one| one.message).collect::<Vec<_>>().join("; ");
    match (error.message, detail.is_empty()) {
        (Some(message), true) => message,
        (Some(message), false) => format!("{message} {detail}"),
        (None, false) => detail,
        (None, true) => body.chars().take(200).collect(),
    }
}

#[derive(Serialize)]
struct Request<'a> {
    name: &'a str,
    private: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<&'a str>,
    auto_init: bool,
}

#[derive(Deserialize)]
struct ErrorBody {
    message: Option<String>,
    #[serde(default)]
    errors: Vec<ErrorDetail>,
}

#[derive(Deserialize)]
struct ErrorDetail {
    message: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rule of this module, tested where it cannot depend on a server answering.
    #[test]
    fn a_public_repository_without_the_answer_is_refused() {
        assert!(Visibility::Private.allowed().is_ok());
        assert!(Visibility::Public { confirmed: true }.allowed().is_ok());
        let refused = Visibility::Public { confirmed: false }.allowed().unwrap_err();
        assert!(matches!(refused, GitHubError::PublicNotConfirmed), "{refused}");
    }

    #[test]
    fn private_is_the_only_visibility_that_needs_nothing() {
        assert!(Visibility::Private.is_private());
        assert!(!Visibility::Public { confirmed: true }.is_private());
    }

    #[test]
    fn githubs_own_words_are_kept_together_in_one_line() {
        let body = r#"{"message":"Repository creation failed.","errors":[{"resource":"Repository","field":"name","message":"name already exists on this account"}]}"#;
        assert_eq!(sentence_from(body), "Repository creation failed. name already exists on this account");
    }

    #[test]
    fn a_body_with_only_a_message_is_that_message() {
        assert_eq!(sentence_from(r#"{"message":"Bad credentials"}"#), "Bad credentials");
    }

    #[test]
    fn a_body_that_is_not_json_is_still_something_to_show() {
        assert_eq!(sentence_from("<html>502</html>"), "<html>502</html>");
    }
}
