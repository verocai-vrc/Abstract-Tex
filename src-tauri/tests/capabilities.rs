//! What the window's permissions (`capabilities/default.json`) actually let it do.
//!
//! `no_silent_network.rs` pins which plugins the window may *name*. This file pins the other half,
//! which that one cannot see: that a permission it names is *usable*. The opener plugin's
//! `allow-open-url` permission is "the command without any pre-configured scope", and with no
//! scope the plugin refuses every address. The app shipped for weeks asking the person's browser to
//! open the GitHub sign-in page and the crash report, and being refused both times, because nothing
//! checked that the permission came with an address list (`bugs-issues-fixes.md`, fifth review
//! pass, item 1). These tests read the real file and match the real addresses the app builds
//! against it, with the same `glob` matching the plugin uses.

use std::fs;
use std::path::PathBuf;

use abstract_tex_lib::crash::issue_url;
use glob::Pattern;
use serde_json::Value;

/// The address patterns the capability file allows `open_url` to open, read the way the plugin
/// reads them: each is a `glob` pattern matched against the whole address.
fn allowed_open_url_patterns() -> Vec<Pattern> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("capabilities/default.json");
    let text = fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let capability: Value = serde_json::from_str(&text).expect("the capability file is JSON");

    let permissions = capability["permissions"].as_array().expect("a permissions list");
    let mut patterns = Vec::new();
    for permission in permissions {
        // A bare string names a permission with the scope its plugin shipped for it. For the
        // opener that is *no* scope (`allow-open-url`) or *every* web address (`default`); the
        // app wants neither, so a bare string contributes no patterns and is reported below.
        let Some(object) = permission.as_object() else {
            continue;
        };
        if object.get("identifier").and_then(Value::as_str) != Some("opener:allow-open-url") {
            continue;
        }
        for entry in object
            .get("allow")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let url = entry["url"].as_str().expect("an allowed entry has a url");
            patterns.push(Pattern::new(url).expect("a valid glob pattern"));
        }
    }
    patterns
}

fn allowed(url: &str) -> bool {
    allowed_open_url_patterns()
        .iter()
        .any(|pattern| pattern.matches(url))
}

#[test]
fn the_github_sign_in_page_can_be_opened() {
    assert!(
        allowed("https://github.com/login/device"),
        "the device-flow verification page is refused by the opener's scope"
    );
}

#[test]
fn the_prefilled_crash_report_can_be_opened() {
    let url = issue_url("Abstract-Tex crash report\nversion 0.1.0\npanic at src/x.rs:1: boom\n");
    assert!(
        allowed(&url),
        "the new-issue address built by crash::issue_url is refused by the opener's scope: {url}"
    );
    assert!(
        url.starts_with(env!("CARGO_PKG_REPOSITORY")),
        "the report goes to this project's own tracker"
    );
}

#[test]
fn nothing_else_can_be_opened() {
    for url in [
        "https://example.com/",
        "https://github.com.evil.example/login/device",
        "http://github.com/login/device",
        "file:///etc/passwd",
        "mailto:someone@example.com",
        "https://github.com/someone-else/their-repo/issues/new",
    ] {
        assert!(!allowed(url), "the opener would open {url}");
    }
}
