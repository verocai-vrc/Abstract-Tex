//! Detects a running Zotero (with the Better BibTeX plugin) on the local machine — the first
//! half of DESIGN.md §5.4's Zotero integration: "Detect a running instance, offer to link a
//! collection… a read-only integration that cannot corrupt anyone's library." This module only
//! ever sends one GET; nothing here writes to Zotero or asks it to change anything.
//!
//! Zotero, when running, serves a local HTTP API on `127.0.0.1:23119`. Better BibTeX — a widely
//! installed Zotero plugin, not part of Zotero itself — layers a JSON-RPC endpoint on top of it
//! at `/better-bibtex/json-rpc`. `item.libraries` is Better BibTeX's own cheapest read-only
//! method (it lists the library IDs and names, wanted by nobody yet — S8.2 is the first real
//! caller of what it returns): asking for it and looking only at whether a reply comes back,
//! and in what shape, is enough to place a Zotero-and-Better-BibTeX check into exactly one of
//! three buckets.

use std::time::Duration;

/// What was found on port 23119, from least to most complete. Ordered so a caller can compare
/// `>=` if it ever needs "at least Zotero is running" without matching every variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ZoteroStatus {
    /// Nothing answered on the port at all: Zotero is not running, or its local API is disabled.
    NotRunning,
    /// Something answered on the port, but not with a Better BibTeX JSON-RPC reply: Zotero is
    /// running without the Better BibTeX plugin installed or enabled.
    NoBetterBibtex,
    /// A valid Better BibTeX JSON-RPC reply came back. Linking a collection (S8.2) is possible.
    Ready,
}

/// Something that can perform the one HTTP POST this module needs. Mirrors
/// [`crate::acquire::doi`]'s `Transport` trait: a real `reqwest` call in production, a recorded
/// reply in tests, so [`detect_with`] runs under plain `cargo test` with no Zotero installed and
/// no `--ignored`.
trait Transport {
    /// `Ok((status, body))` for any reply that arrived at all. `Err` is reserved for a request
    /// that could not complete — critically, this includes "connection refused", which is the
    /// normal, expected shape of "Zotero is not running" and must not be confused with a real
    /// failure worth showing the author a message about.
    fn post_json_rpc(&self, url: &str, body: &str) -> Result<(u16, String), String>;
}

/// Detect whether Zotero and Better BibTeX are reachable on this machine. Requires the `acquire`
/// feature, like every other network call in this module tree.
pub fn detect() -> ZoteroStatus {
    detect_with(&HttpTransport)
}

/// [`detect`]'s logic, taking the transport as a parameter so a test can substitute a fixture
/// reply — or a simulated connection refusal — for a real request.
fn detect_with(transport: &impl Transport) -> ZoteroStatus {
    let url = "http://127.0.0.1:23119/better-bibtex/json-rpc";
    let request = r#"{"jsonrpc":"2.0","method":"item.libraries","params":[]}"#;
    let Ok((status, body)) = transport.post_json_rpc(url, request) else {
        // A connection refused, a timeout, or DNS failing on `127.0.0.1` (it will not) all mean
        // the same thing here: nobody is listening on the port.
        return ZoteroStatus::NotRunning;
    };
    if status == 200 && looks_like_json_rpc_reply(&body) {
        ZoteroStatus::Ready
    } else {
        // Port 23119 answered — that is Zotero's own local API, which only exists when Zotero is
        // running — but not with something Better BibTeX would have said, so the plugin is
        // missing or disabled. `/better-bibtex/json-rpc` is a 404 from Zotero's own HTTP server
        // (a real body, real status, no reply shape to speak of) when Better BibTeX isn't there.
        ZoteroStatus::NoBetterBibtex
    }
}

/// A minimal check that `body` is a JSON-RPC 2.0 reply — a `"jsonrpc":"2.0"` marker and either
/// a `"result"` or an `"error"` member — without pulling in a JSON parser for one field. Good
/// enough to tell "Better BibTeX answered" from "Zotero's own 404 page answered", which is all
/// this call needs; parsing the reply for real is S8.2's job, once there is something in it this
/// app acts on.
fn looks_like_json_rpc_reply(body: &str) -> bool {
    body.contains("\"jsonrpc\"") && (body.contains("\"result\"") || body.contains("\"error\""))
}

struct HttpTransport;

impl Transport for HttpTransport {
    fn post_json_rpc(&self, url: &str, body: &str) -> Result<(u16, String), String> {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .map_err(|err| err.to_string())?;
        let response = client
            .post(url)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body.to_string())
            .send()
            .map_err(|err| err.to_string())?;
        let status = response.status().as_u16();
        let text = response.text().map_err(|err| err.to_string())?;
        Ok((status, text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FixedReply(Result<(u16, String), String>);

    impl Transport for FixedReply {
        fn post_json_rpc(&self, _url: &str, _body: &str) -> Result<(u16, String), String> {
            self.0.clone()
        }
    }

    #[test]
    fn a_connection_refusal_means_not_running() {
        let transport = FixedReply(Err("connection refused".to_string()));
        assert_eq!(detect_with(&transport), ZoteroStatus::NotRunning);
    }

    #[test]
    fn a_valid_json_rpc_reply_means_ready() {
        let reply = r#"{"jsonrpc":"2.0","result":[{"id":1,"name":"My Library"}]}"#;
        let transport = FixedReply(Ok((200, reply.to_string())));
        assert_eq!(detect_with(&transport), ZoteroStatus::Ready);
    }

    #[test]
    fn a_json_rpc_error_reply_still_means_ready() {
        // An error member is still Better BibTeX talking JSON-RPC back to us; the *method*
        // failing is not the same question as "is the plugin there at all".
        let reply = r#"{"jsonrpc":"2.0","error":{"code":-32601,"message":"method not found"}}"#;
        let transport = FixedReply(Ok((200, reply.to_string())));
        assert_eq!(detect_with(&transport), ZoteroStatus::Ready);
    }

    #[test]
    fn zoteros_own_404_for_the_missing_plugin_means_no_better_bibtex() {
        let transport = FixedReply(Ok((404, "Not Found".to_string())));
        assert_eq!(detect_with(&transport), ZoteroStatus::NoBetterBibtex);
    }

    #[test]
    fn a_200_with_an_unrecognised_body_means_no_better_bibtex() {
        // Defensive: something is on the port and answers 200, but not with a shape Better
        // BibTeX would send. Treated the same as "plugin missing" rather than crashing on it.
        let transport = FixedReply(Ok((200, "<html>hello</html>".to_string())));
        assert_eq!(detect_with(&transport), ZoteroStatus::NoBetterBibtex);
    }

    #[test]
    fn status_ordering_places_ready_above_no_better_bibtex_above_not_running() {
        assert!(ZoteroStatus::Ready > ZoteroStatus::NoBetterBibtex);
        assert!(ZoteroStatus::NoBetterBibtex > ZoteroStatus::NotRunning);
    }

    /// The one test that reaches a real, local Zotero. Kept `#[ignore]`d because CI and most
    /// development sessions have no Zotero running — run by hand with
    /// `cargo test -p texbib --features acquire -- --ignored zotero`.
    #[test]
    #[ignore]
    fn a_real_local_zotero_is_detected_if_one_happens_to_be_running() {
        // Deliberately does not assert a specific variant: this test's only job is to prove
        // `detect()` does not panic or hang against a real machine, whatever the answer is.
        let _ = detect();
    }
}
