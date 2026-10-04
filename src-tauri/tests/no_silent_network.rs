//! S13.4: the structural half of DESIGN.md §8's "No silent data paths".
//!
//! > A test asserts that with no API key configured and the network blocked, the application is
//! > fully functional and makes zero outbound requests.
//!
//! Nothing here starts the app. It pins, in files a reviewer can read, *which parts of the program
//! are able to reach the network at all*, so that adding another one is a decision somebody had to
//! write down here and defend, rather than a dependency line nobody noticed. The runtime half is a
//! measurement (`strace` on a real run; see S13.4 in `0.1/SPRINTS.md`), which no unit test can be.
//!
//! What it checks:
//!
//! 1. Only the crates in [`MAY_REACH_THE_NETWORK`] depend on an HTTP or WebSocket client, or ask
//!    libgit2 for `https`/`ssh`, and no Tauri plugin that talks on its own (updater, http,
//!    websocket, upload) is installed.
//! 2. The window's content-security policy lets the page connect nowhere but this computer or —
//!    S14.2c's one deliberate exception — a `ws:`/`wss:` address nobody but the person using the
//!    app chose, and the window's permissions name no network plugin.
//! 3. The frontend's only `fetch` is the PDF viewer reading a local `asset:` URL, its only
//!    `new WebSocket` is the live relay a person explicitly shared or joined, and nothing in it
//!    opens a beacon or remote script.

use std::fs;
use std::path::{Path, PathBuf};

/// Where the workspace starts: the parent of `src-tauri`.
fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("src-tauri has a parent")
        .to_path_buf()
}

/// The crates that may reach the network, and the only reason each is allowed to. Every one of them
/// acts on a person's request: nothing runs at start-up, on a timer or on opening a project.
const MAY_REACH_THE_NETWORK: &[(&str, &str)] = &[
    (
        "crates/abstract-tex-assistant",
        "a model call: the Test button, a rewrite or an explanation, each after a click (and, for the last two, a dialog showing the request)",
    ),
    (
        "crates/abstract-tex-github",
        "GitHub sign-in and repository creation, started by the person",
    ),
    (
        "crates/abstract-tex-git",
        "fetch, push, sync and clone over https, started by the person",
    ),
    (
        "crates/texbib",
        "the optional `acquire` feature: DOI, arXiv and ISBN lookups and the local Zotero link, asked for",
    ),
    (
        "crates/abstract-tex-relay",
        "the v0.8 relay binary (`abstract-tex-relay`) — not this app. It is a separate process a \
         person runs on their own server; this row is still only about this crate's own Cargo \
         dependencies, which have not changed. S14.2c did add a connection to a relay, but it is a \
         plain browser `WebSocket` in the frontend (`src/lib/relay/live-session.ts`), never this \
         crate or any Rust network client — tracked by `the_frontend_opens_no_connection_of_its_own` \
         and the CSP test below, not this list",
    ),
];

/// Dependencies that give a crate a way out.
const NETWORK_CLIENTS: &[&str] = &[
    "reqwest",
    "ureq",
    "hyper",
    "isahc",
    "surf",
    "attohttpc",
    "curl",
    "tungstenite",
    "tokio-tungstenite",
    "websocket",
    "tauri-plugin-http",
    "tauri-plugin-updater",
    "tauri-plugin-websocket",
    "tauri-plugin-upload",
];

/// Every `Cargo.toml` that belongs to this workspace's own code, as paths relative to it.
fn manifests() -> Vec<(String, String)> {
    let root = workspace();
    let mut found = vec![("src-tauri".to_string(), read(&root.join("src-tauri/Cargo.toml")))];
    for entry in fs::read_dir(root.join("crates")).expect("crates/") {
        let dir = entry.unwrap().path();
        let manifest = dir.join("Cargo.toml");
        if manifest.is_file() {
            let name = format!("crates/{}", dir.file_name().unwrap().to_string_lossy());
            found.push((name, read(&manifest)));
        }
    }
    found
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// The dependency lines of a manifest that are not comments.
fn code_lines(manifest: &str) -> impl Iterator<Item = &str> {
    manifest
        .lines()
        .map(str::trim)
        .filter(|line| !line.starts_with('#'))
}

/// Whether a manifest names this dependency (`name = …`, `name.workspace = true`, or a feature
/// that turns it on, `"dep:name"`).
fn depends_on(manifest: &str, name: &str) -> bool {
    code_lines(manifest).any(|line| {
        line.starts_with(&format!("{name} "))
            || line.starts_with(&format!("{name}="))
            || line.starts_with(&format!("{name}."))
            || line.contains(&format!("\"dep:{name}\""))
    })
}

/// A `git2` dependency that can speak https or ssh. Every other crate's `git2` is built with
/// `default-features = false` and can only read and write the local repository.
fn git2_can_go_online(manifest: &str) -> bool {
    code_lines(manifest)
        .filter(|line| line.starts_with("git2"))
        .any(|line| line.contains("\"https\"") || line.contains("\"ssh\""))
}

#[test]
fn only_the_listed_crates_can_reach_the_network() {
    let allowed: Vec<&str> = MAY_REACH_THE_NETWORK.iter().map(|(name, _)| *name).collect();
    let mut found: Vec<String> = Vec::new();
    for (name, manifest) in manifests() {
        let has_client = NETWORK_CLIENTS.iter().any(|client| depends_on(&manifest, client));
        if has_client || git2_can_go_online(&manifest) {
            found.push(name);
        }
    }
    found.sort();
    let mut expected: Vec<String> = allowed.iter().map(|name| name.to_string()).collect();
    expected.sort();
    assert_eq!(
        found, expected,
        "a crate gained (or lost) a way to reach the network. If it is on purpose, add it to \
         MAY_REACH_THE_NETWORK with the person's action that starts it; if not, remove the dependency."
    );
}

#[test]
fn the_app_crate_itself_has_no_client_of_its_own() {
    // `src-tauri` calls the crates above; it must not grow a request of its own, where the
    // "which action starts this" question is easy to skip.
    let manifest = read(&workspace().join("src-tauri/Cargo.toml"));
    for client in NETWORK_CLIENTS {
        assert!(!depends_on(&manifest, client), "src-tauri depends on {client}");
    }
    assert!(!git2_can_go_online(&manifest));
}

#[test]
fn the_pages_content_security_policy_connects_nowhere_but_this_computer_or_a_chosen_relay() {
    let config = read(&workspace().join("src-tauri/tauri.conf.json"));
    let config: serde_json::Value = serde_json::from_str(&config).unwrap();
    let policy = config["app"]["security"]["csp"].as_str().expect("a CSP is set");

    for directive in policy.split(';').map(str::trim).filter(|d| !d.is_empty()) {
        let (name, sources) = directive.split_once(' ').unwrap_or((directive, ""));
        for source in sources.split_whitespace() {
            assert!(source != "*", "`{directive}` allows every host");
            assert!(
                !source.starts_with("https:"),
                "`{directive}` allows https hosts: {source}"
            );
            if let Some(host) = source.strip_prefix("http://") {
                assert!(
                    host.starts_with("localhost") || host.ends_with(".localhost"),
                    "`{directive}` allows a remote http host: {source}"
                );
            }
            // S14.2c: `connect-src` alone may open a `ws:`/`wss:` socket — a live session's relay
            // address is the one thing in this app that is never known ahead of time (DESIGN.md
            // §10 is "host nothing"; a person points this at a server of their own). Allowed only
            // as the bare scheme, naming no host, so this still cannot be read as endorsing one
            // remote address over another; every other directive must still resolve to this
            // computer, and a WebSocket is still refused everywhere else.
            let websocket = source.starts_with("ws:") || source.starts_with("wss:");
            assert!(
                !websocket || name == "connect-src",
                "`{directive}` allows a WebSocket outside connect-src: {source}"
            );
            assert!(
                !websocket || source == "ws:" || source == "wss:",
                "`{directive}` names a specific WebSocket host rather than the bare scheme: {source}"
            );
        }
    }
    assert!(
        policy.contains("connect-src"),
        "connect-src must be stated, not inherited"
    );
}

#[test]
fn the_window_has_no_updater_and_may_call_no_network_plugin() {
    let config: serde_json::Value =
        serde_json::from_str(&read(&workspace().join("src-tauri/tauri.conf.json"))).unwrap();
    let plugins = config.get("plugins").and_then(|plugins| plugins.as_object());
    assert!(
        plugins.map_or(true, |plugins| plugins.is_empty()),
        "tauri.conf.json configures plugins {plugins:?}: an updater checks for new versions without being asked"
    );

    let capabilities = read(&workspace().join("src-tauri/capabilities/default.json"));
    for forbidden in ["http:", "updater:", "websocket:", "upload:", "shell:"] {
        assert!(
            !capabilities.contains(&format!("\"{forbidden}")),
            "the main window is granted a `{forbidden}` permission"
        );
    }
}

/// Every `.ts`, `.svelte` and `.html` source of the frontend that is not a test.
fn frontend_sources() -> Vec<(String, String)> {
    fn walk(dir: &Path, into: &mut Vec<(String, String)>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, into);
                continue;
            }
            let name = path.to_string_lossy().into_owned();
            let is_source = [".ts", ".svelte"].iter().any(|ext| name.ends_with(ext));
            if is_source && !name.ends_with(".test.ts") {
                into.push((name, read(&path)));
            }
        }
    }
    let mut sources = Vec::new();
    walk(&workspace().join("src"), &mut sources);
    sources.push(("index.html".to_string(), read(&workspace().join("index.html"))));
    sources
}

#[test]
fn the_frontend_opens_no_connection_of_its_own() {
    let mut fetches: Vec<String> = Vec::new();
    let mut websockets: Vec<String> = Vec::new();
    for (file, text) in frontend_sources() {
        // Comments may talk about all of these; code may not do them.
        let code: String = text
            .lines()
            .filter(|line| {
                let line = line.trim_start();
                !(line.starts_with("//") || line.starts_with('*') || line.starts_with("/*"))
            })
            .collect::<Vec<_>>()
            .join("\n");
        for forbidden in [
            "XMLHttpRequest",
            "sendBeacon",
            "new EventSource",
            "importScripts(",
        ] {
            assert!(!code.contains(forbidden), "{file} uses {forbidden}");
        }
        assert!(
            !code.contains("src=\"http")
                && !code.contains("href=\"http://")
                && !code.contains("@import url(http"),
            "{file} loads something from a remote address"
        );
        if code.contains("fetch(") {
            fetches.push(file.replace(&format!("{}/", workspace().display()), ""));
        }
        if code.contains("new WebSocket") {
            websockets.push(file.replace(&format!("{}/", workspace().display()), ""));
        }
    }
    assert_eq!(
        fetches,
        vec!["src/lib/pdf/viewer.ts".to_string()],
        "a new `fetch(` appeared in the frontend. The CSP would stop a remote one, but a fetch needs a \
         reason written down: if it reads a local asset, add its file here."
    );
    assert_eq!(
        websockets,
        vec!["src/lib/relay/live-session.ts".to_string()],
        "a new `new WebSocket` appeared in the frontend. S14.2c's live relay is the one place this app \
         opens a socket of its own — to the address a person typed into *Share* or pasted into *Join*, \
         never one this app chose — and the CSP's `connect-src` was loosened for exactly that one case. \
         A second file doing this needs the same explicit review this one got, not a silent addition here."
    );
}
