//! Resolves a DOI to a BibTeX entry by content negotiation: `GET https://doi.org/<doi>` with
//! `Accept: application/x-bibtex` returns the publisher's own record directly, no API key and no
//! service of ours in between (DESIGN.md §5.4).

use std::time::Duration;

use crate::Entry;

/// Why a DOI lookup failed. Each variant already carries the one sentence a caller should show
/// the author — `Display`, derived below, is that sentence.
///
/// `thiserror::Error` is a derive macro: it writes the `std::error::Error` and `Display` impls
/// from the `#[error("...")]` string on each variant, so a caller can both `match` on the reason
/// and print it, without either impl being written by hand.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum DoiError {
    /// doi.org answered, but has no record for this identifier (a 404).
    #[error("no record found for DOI {0}")]
    NotFound(String),
    /// The request could not be completed at all — no connection, a timeout, or a status this
    /// module was not expecting. The string is whatever the transport said, since there is
    /// nothing more specific to add.
    #[error("could not reach doi.org: {0}")]
    Network(String),
    /// doi.org answered with a 200, but the body was not one parseable BibTeX entry.
    #[error("doi.org's reply did not parse as a BibTeX entry")]
    Unparseable,
}

/// Strip a known DOI prefix from what the author pasted — a `doi.org` URL in either of its two
/// hostnames, or a bare `doi:` scheme — case-insensitively, and trim whitespace. A pasted string
/// that matches none of them, including an already-bare `10.1109/tcbb.2019.000001`, passes
/// through unchanged: this function only removes wrapping, it does not judge whether what is
/// left is a real DOI. doi.org is the authority on that, via a 404.
pub fn normalize_doi(pasted: &str) -> String {
    const PREFIXES: [&str; 5] = [
        "https://doi.org/",
        "http://doi.org/",
        "https://dx.doi.org/",
        "http://dx.doi.org/",
        "doi:",
    ];
    let trimmed = pasted.trim();
    for prefix in PREFIXES {
        // `str::get` returns `None` rather than panicking when `prefix.len()` does not land on a
        // char boundary of `trimmed` — possible if the author pasted something short and strange
        // before any prefix check would otherwise run off the end of the string.
        if trimmed
            .get(..prefix.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
        {
            return trimmed[prefix.len()..].trim().to_string();
        }
    }
    trimmed.to_string()
}

/// Something that can perform the one HTTP GET this module needs. [`HttpTransport`] is the real
/// implementation, over `reqwest`; a test implements this trait itself to hand back a recorded
/// reply instead of reaching the network, which is what lets [`fetch_doi_with`] be exercised by
/// `cargo test` with no network and no `--ignored`.
trait Transport {
    /// `Ok((status, body))` for any reply that arrived at all, including a 404 — that is doi.org
    /// answering, not a transport failure. `Err` is reserved for requests that never got a
    /// status back: DNS, connection refused, a timeout.
    fn get_bibtex(&self, url: &str) -> Result<(u16, String), String>;
}

/// Resolve `doi` (in any of the pasted forms [`normalize_doi`] understands) to an [`Entry`] by
/// asking the real `doi.org`. Requires the `acquire` feature, which this whole module is behind.
pub fn fetch_doi(doi: &str) -> Result<Entry, DoiError> {
    fetch_doi_with(doi, &HttpTransport)
}

/// [`fetch_doi`]'s logic, taking the transport as a parameter so a test can substitute a fixture
/// reply for a real request. Not `pub`: the seam exists for this module's own tests, not as part
/// of the crate's public API.
fn fetch_doi_with(doi: &str, transport: &impl Transport) -> Result<Entry, DoiError> {
    let normalized = normalize_doi(doi);
    let url = format!("https://doi.org/{normalized}");
    let (status, body) = transport.get_bibtex(&url).map_err(DoiError::Network)?;
    match status {
        200 => {
            let bibliography = crate::parse(&body);
            let entry = bibliography.entries().next().cloned();
            entry.ok_or(DoiError::Unparseable)
        }
        404 => Err(DoiError::NotFound(normalized)),
        other => Err(DoiError::Network(format!("unexpected status {other}"))),
    }
}

struct HttpTransport;

impl Transport for HttpTransport {
    fn get_bibtex(&self, url: &str) -> Result<(u16, String), String> {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|err| err.to_string())?;
        let response = client
            .get(url)
            .header(reqwest::header::ACCEPT, "application/x-bibtex")
            .send()
            .map_err(|err| err.to_string())?;
        let status = response.status().as_u16();
        let body = response.text().map_err(|err| err.to_string())?;
        Ok((status, body))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A hand-authored reply in the shape a real `doi.org` content-negotiation response takes:
    /// the whole entry on one line, upper-case field names, no trailing newline — the same shape
    /// `fixtures/doi-negotiation/main.bib` records for the parser's own tests, reused here as the
    /// recorded fixture `fetch_doi_with` parses in the tests below.
    const RECORDED_REPLY: &str = include_str!("../../fixtures/doi-negotiation/main.bib");

    struct FixedReply(Result<(u16, String), String>);

    impl Transport for FixedReply {
        fn get_bibtex(&self, _url: &str) -> Result<(u16, String), String> {
            self.0.clone()
        }
    }

    #[test]
    fn a_url_form_normalizes_to_the_bare_doi() {
        assert_eq!(
            normalize_doi("https://doi.org/10.1109/tcbb.2019.000001"),
            "10.1109/tcbb.2019.000001"
        );
    }

    #[test]
    fn the_old_dx_doi_org_host_normalizes_the_same_way() {
        assert_eq!(
            normalize_doi("http://dx.doi.org/10.1109/TCBB.2019.000001"),
            "10.1109/TCBB.2019.000001"
        );
    }

    #[test]
    fn a_doi_scheme_normalizes_the_same_way() {
        assert_eq!(
            normalize_doi("doi:10.1109/tcbb.2019.000001"),
            "10.1109/tcbb.2019.000001"
        );
    }

    #[test]
    fn the_prefix_check_is_case_insensitive() {
        assert_eq!(
            normalize_doi("DOI:10.1109/tcbb.2019.000001"),
            "10.1109/tcbb.2019.000001"
        );
        assert_eq!(
            normalize_doi("HTTPS://DOI.ORG/10.1109/tcbb.2019.000001"),
            "10.1109/tcbb.2019.000001"
        );
    }

    #[test]
    fn an_already_bare_doi_passes_through_unchanged() {
        assert_eq!(
            normalize_doi("  10.1109/tcbb.2019.000001  "),
            "10.1109/tcbb.2019.000001"
        );
    }

    #[test]
    fn a_short_pasted_string_does_not_panic_on_prefix_matching() {
        // Shorter than every prefix above; `str::get` inside `normalize_doi` must return `None`
        // rather than slicing off the end of the string.
        assert_eq!(normalize_doi("hi"), "hi");
    }

    #[test]
    fn the_three_pasted_forms_resolve_to_the_same_entry() {
        let forms = [
            "https://doi.org/10.1109/tcbb.2019.000001",
            "doi:10.1109/tcbb.2019.000001",
            "10.1109/tcbb.2019.000001",
        ];
        let entries: Vec<Entry> = forms
            .iter()
            .map(|doi| {
                let transport = FixedReply(Ok((200, RECORDED_REPLY.to_string())));
                fetch_doi_with(doi, &transport).expect("the recorded reply parses")
            })
            .collect();
        assert_eq!(entries[0], entries[1]);
        assert_eq!(entries[1], entries[2]);
        assert_eq!(entries[0].key, "Smith_2019");
    }

    #[test]
    fn a_404_becomes_not_found() {
        let transport = FixedReply(Ok((404, String::new())));
        assert_eq!(
            fetch_doi_with("10.0000/nope", &transport),
            Err(DoiError::NotFound("10.0000/nope".to_string()))
        );
    }

    #[test]
    fn an_unexpected_status_becomes_a_network_error() {
        let transport = FixedReply(Ok((503, String::new())));
        assert_eq!(
            fetch_doi_with("10.1109/tcbb.2019.000001", &transport),
            Err(DoiError::Network("unexpected status 503".to_string()))
        );
    }

    #[test]
    fn a_transport_failure_becomes_a_network_error() {
        let transport = FixedReply(Err("connection refused".to_string()));
        assert_eq!(
            fetch_doi_with("10.1109/tcbb.2019.000001", &transport),
            Err(DoiError::Network("connection refused".to_string()))
        );
    }

    #[test]
    fn a_200_that_is_not_bibtex_is_unparseable() {
        let transport = FixedReply(Ok((200, "<html>this is not a .bib file</html>".to_string())));
        assert_eq!(
            fetch_doi_with("10.1109/tcbb.2019.000001", &transport),
            Err(DoiError::Unparseable)
        );
    }

    /// The one test in this file that reaches the real network, `cargo test`'s default `--ignored`
    /// filter keeps it out of the normal run — it needs a live connection to doi.org and depends
    /// on a real DOI staying registered. Run by hand with
    /// `cargo test -p texbib --features acquire -- --ignored`.
    #[test]
    #[ignore]
    fn the_real_doi_org_resolves_a_known_doi() {
        let entry = fetch_doi("10.1371/journal.pcbi.1000387").expect("a real, stable DOI");
        assert!(entry.field("title").is_some());
    }
}
