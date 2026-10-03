//! Detects a running Zotero (with the Better BibTeX plugin) on the local machine and links one of
//! its collections into the project's bibliography (DESIGN.md §5.4): "Detect a running
//! instance, offer to link a collection… a read-only integration that cannot corrupt anyone's
//! library." Every call this module makes is either a read (`user.groups`) or a request that
//! Better BibTeX itself write a `.bib` file to a path *we* choose on disk
//! (`autoexport.add`) — nothing here ever asks Zotero to change anything in the library itself.
//!
//! Zotero, when running, serves a local HTTP API on `127.0.0.1:23119`. Better BibTeX — a widely
//! installed Zotero plugin, not part of Zotero itself — layers a JSON-RPC endpoint on top of it
//! at `/better-bibtex/json-rpc`. `user.groups` is Better BibTeX's own method for listing the
//! libraries ("groups", in Zotero's own terminology, including the personal library) the user
//! has, optionally with each one's collections — cheap, read-only, and (per S8.2's bug-ledger
//! entry) the correct replacement for an earlier, nonexistent `item.libraries` call this module
//! used only to check the *shape* of a reply, not its content.

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

/// One library ("group", in Zotero's terms — the personal library is one too) and, when asked
/// for, its collections.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Library {
    /// Zotero's own numeric library id — the personal library and every group each have one.
    pub id: i64,
    /// The library's display name, as Zotero shows it — the user's own library, or a group's name.
    pub name: String,
    /// Top-level collections only; each one's own children are nested inside it.
    pub collections: Vec<Collection>,
}

/// One collection inside a library, with its own sub-collections (Zotero collections nest).
/// `path` is this collection's forward-slash path *within its library*, e.g. `"Reading/2024"` —
/// the form `autoexport.add`'s `collection` parameter wants, library name first.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Collection {
    /// The collection's own name, e.g. `"2024"` for `My Library/Reading/2024`.
    pub name: String,
    /// This collection's own path, library name first — the exact string `add_autoexport`'s
    /// `collection_path` argument wants.
    pub path: String,
    /// Sub-collections nested directly inside this one.
    pub children: Vec<Collection>,
}

/// Something that can perform the one HTTP POST this module needs. Mirrors
/// [`crate::acquire::doi`]'s `Transport` trait: a real `reqwest` call in production, a recorded
/// reply in tests, so this module's logic runs under plain `cargo test` with no Zotero installed
/// and no `--ignored`.
trait Transport {
    /// `Ok((status, body))` for any reply that arrived at all. `Err` is reserved for a request
    /// that could not complete — critically, this includes "connection refused", which is the
    /// normal, expected shape of "Zotero is not running" and must not be confused with a real
    /// failure worth showing the author a message about.
    fn post_json_rpc(&self, url: &str, body: &str) -> Result<(u16, String), String>;
}

const ENDPOINT: &str = "http://127.0.0.1:23119/better-bibtex/json-rpc";

/// Detect whether Zotero and Better BibTeX are reachable on this machine. Requires the `acquire`
/// feature, like every other network call in this module tree.
pub fn detect() -> ZoteroStatus {
    detect_with(&HttpTransport)
}

fn detect_with(transport: &impl Transport) -> ZoteroStatus {
    let request = r#"{"jsonrpc":"2.0","method":"user.groups","params":[]}"#;
    let Ok((status, body)) = transport.post_json_rpc(ENDPOINT, request) else {
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
/// enough to tell "Better BibTeX answered" from "Zotero's own 404 page answered".
fn looks_like_json_rpc_reply(body: &str) -> bool {
    body.contains("\"jsonrpc\"") && (body.contains("\"result\"") || body.contains("\"error\""))
}

/// Errors listing libraries or linking a collection. Kept separate from [`ZoteroStatus`]: that
/// enum is "what state is Zotero in", a fact worth showing quietly in a status bar; this is "the
/// author asked for something and it failed", a fact worth a message.
#[derive(Debug, thiserror::Error)]
pub enum ZoteroError {
    /// No reply arrived at all — same condition [`ZoteroStatus::NotRunning`] reports quietly;
    /// this variant is for a call the author explicitly asked for, so it becomes a message.
    #[error("Zotero (with Better BibTeX) is not reachable on this machine.")]
    NotReachable,
    /// A reply arrived but was not a JSON-RPC envelope this module could read, or was itself a
    /// JSON-RPC `error` member — the raw body, for a support report.
    #[error("Zotero replied, but not in a way this app understands: {0}")]
    UnexpectedReply(String),
}

/// List every library the user has, each with its full collection tree. The one real caller of
/// `user.groups`'s `includeCollections` — [`detect`] does not need collections, so it asks for
/// the cheaper reply.
pub fn list_libraries() -> Result<Vec<Library>, ZoteroError> {
    list_libraries_with(&HttpTransport)
}

fn list_libraries_with(transport: &impl Transport) -> Result<Vec<Library>, ZoteroError> {
    let request = r#"{"jsonrpc":"2.0","method":"user.groups","params":[true]}"#;
    let (status, body) = transport
        .post_json_rpc(ENDPOINT, request)
        .map_err(|_| ZoteroError::NotReachable)?;
    if status != 200 {
        return Err(ZoteroError::UnexpectedReply(body));
    }
    parse_groups_reply(&body)
}

/// Ask Better BibTeX to keep `output_path` updated with `collection_path`'s contents in BibTeX
/// format, whenever the library changes — Better BibTeX's own auto-export feature
/// (`autoexport.add`), configured once from here rather than by the author opening Zotero's
/// export dialog by hand. This app never re-triggers or reads back the export itself: once
/// requested, the `.bib` file is Better BibTeX's to keep current, and this app's own watcher
/// (S7.2) picks up its changes the same as any other `.bib` on disk.
///
/// `collection_path` is a library name followed by the collection's own path, e.g.
/// `"My Library/Reading/2024"` — the shape [`Collection::path`] is built in.
pub fn add_autoexport(collection_path: &str, output_path: &str) -> Result<(), ZoteroError> {
    add_autoexport_with(&HttpTransport, collection_path, output_path)
}

fn add_autoexport_with(
    transport: &impl Transport,
    collection_path: &str,
    output_path: &str,
) -> Result<(), ZoteroError> {
    // "Better BibTeX" is the translator name Better BibTeX itself registers for plain-BibTeX
    // export; escaping here is the same ad hoc JSON string escaping `doi.rs`'s fetchers already
    // use for a handful of characters, not a general-purpose encoder, because these three values
    // are never attacker-controlled — they come from a collection this app just listed and a
    // path this app just resolved.
    let request = format!(
        r#"{{"jsonrpc":"2.0","method":"autoexport.add","params":["{}","Better BibTeX","{}",{{}},true]}}"#,
        escape_json(collection_path),
        escape_json(output_path),
    );
    let (status, body) = transport
        .post_json_rpc(ENDPOINT, &request)
        .map_err(|_| ZoteroError::NotReachable)?;
    if status != 200 || !looks_like_json_rpc_reply(&body) || body.contains("\"error\"") {
        return Err(ZoteroError::UnexpectedReply(body));
    }
    Ok(())
}

fn escape_json(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Pull `result` out of `user.groups(true)`'s reply and walk each library's collection tree.
/// Hand-rolled rather than a full JSON parser: the shape needed here is narrow (an array of
/// objects with `id`, `name`, and a nested `collections` array of the same shape as Zotero's own
/// collection objects), and `texbib` otherwise has no JSON dependency to justify pulling one in
/// for this alone — the same call `doi.rs` made about not needing a bibtex-specific crate.
fn parse_groups_reply(body: &str) -> Result<Vec<Library>, ZoteroError> {
    let value: serde_json::Value =
        serde_json::from_str(body).map_err(|_| ZoteroError::UnexpectedReply(body.to_string()))?;
    let result = value
        .get("result")
        .ok_or_else(|| ZoteroError::UnexpectedReply(body.to_string()))?;
    let groups = result
        .as_array()
        .ok_or_else(|| ZoteroError::UnexpectedReply(body.to_string()))?;

    Ok(groups
        .iter()
        .map(|group| {
            let id = group.get("id").and_then(|v| v.as_i64()).unwrap_or_default();
            let name = group
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("Untitled library")
                .to_string();
            let collections = group
                .get("collections")
                .and_then(|v| v.as_array())
                .map(|items| collections_from_json(items, &name))
                .unwrap_or_default();
            Library {
                id,
                name,
                collections,
            }
        })
        .collect())
}

/// Recursively read Zotero's own collection objects (`{name, key, collections: [...]}` at every
/// level, `collections` sometimes absent instead of empty) into our [`Collection`] tree, building
/// each node's `path` by joining `parent_path` and the node's name.
fn collections_from_json(items: &[serde_json::Value], parent_path: &str) -> Vec<Collection> {
    items
        .iter()
        .filter_map(|item| {
            let name = item.get("name").and_then(|v| v.as_str())?.to_string();
            let path = format!("{parent_path}/{name}");
            let children = item
                .get("collections")
                .and_then(|v| v.as_array())
                .map(|nested| collections_from_json(nested, &path))
                .unwrap_or_default();
            Some(Collection { name, path, children })
        })
        .collect()
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

    #[test]
    fn list_libraries_reads_nested_collections_into_forward_slash_paths() {
        let reply = r#"{"jsonrpc":"2.0","result":[
            {"id":1,"name":"My Library","collections":[
                {"name":"Reading","key":"AAAA","collections":[
                    {"name":"2024","key":"BBBB","collections":[]}
                ]},
                {"name":"Archive","key":"CCCC"}
            ]},
            {"id":2,"name":"Team Group","collections":[]}
        ]}"#;
        let transport = FixedReply(Ok((200, reply.to_string())));
        let libraries = list_libraries_with(&transport).expect("parses");

        assert_eq!(libraries.len(), 2);
        assert_eq!(libraries[0].name, "My Library");
        assert_eq!(libraries[0].collections[0].path, "My Library/Reading");
        assert_eq!(
            libraries[0].collections[0].children[0].path,
            "My Library/Reading/2024"
        );
        assert_eq!(libraries[0].collections[1].path, "My Library/Archive");
        assert_eq!(libraries[1].name, "Team Group");
        assert!(libraries[1].collections.is_empty());
    }

    #[test]
    fn list_libraries_rejects_a_connection_refusal_as_not_reachable() {
        let transport = FixedReply(Err("connection refused".to_string()));
        assert!(matches!(
            list_libraries_with(&transport),
            Err(ZoteroError::NotReachable)
        ));
    }

    #[test]
    fn list_libraries_rejects_a_non_json_body() {
        let transport = FixedReply(Ok((200, "<html>hello</html>".to_string())));
        assert!(matches!(
            list_libraries_with(&transport),
            Err(ZoteroError::UnexpectedReply(_))
        ));
    }

    #[test]
    fn add_autoexport_accepts_a_clean_result_reply() {
        let reply = r#"{"jsonrpc":"2.0","result":{"id":1,"key":"AAAA","libraryID":1}}"#;
        let transport = FixedReply(Ok((200, reply.to_string())));
        assert!(add_autoexport_with(&transport, "My Library/Reading", "C:/proj/reading.bib").is_ok());
    }

    #[test]
    fn add_autoexport_surfaces_a_json_rpc_error_as_unexpected_reply() {
        let reply = r#"{"jsonrpc":"2.0","error":{"code":-32602,"message":"collection not found"}}"#;
        let transport = FixedReply(Ok((200, reply.to_string())));
        assert!(matches!(
            add_autoexport_with(&transport, "My Library/Missing", "C:/proj/reading.bib"),
            Err(ZoteroError::UnexpectedReply(_))
        ));
    }

    #[test]
    fn add_autoexport_never_sends_a_write_method_other_than_autoexport_add() {
        // A cheap guard against a future edit accidentally calling a different (and possibly
        // library-mutating) method: this is the one place in the module a request body is sent
        // for a *write*, so its method name is worth pinning directly.
        struct CapturingTransport(std::cell::RefCell<Option<String>>);
        impl Transport for CapturingTransport {
            fn post_json_rpc(&self, _url: &str, body: &str) -> Result<(u16, String), String> {
                *self.0.borrow_mut() = Some(body.to_string());
                Ok((200, r#"{"jsonrpc":"2.0","result":{}}"#.to_string()))
            }
        }
        let transport = CapturingTransport(std::cell::RefCell::new(None));
        add_autoexport_with(&transport, "My Library/Reading", "C:/proj/reading.bib").unwrap();
        let sent = transport.0.borrow().clone().unwrap();
        assert!(sent.contains("\"method\":\"autoexport.add\""));
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
