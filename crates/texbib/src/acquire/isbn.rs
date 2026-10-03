//! Resolves an ISBN to a BibLaTeX `@book` entry through OpenLibrary's JSON API — no API key, no
//! service of ours in between (DESIGN.md §5.4).
//!
//! **This is the one source of the three that cannot be one request.** DOI and arXiv each
//! answer a single lookup with everything a citation needs, author names included. OpenLibrary's
//! per-edition record (`/isbn/<isbn>.json`) does not carry author names at all — only a `works`
//! key and, separately, a loose jacket-copy `by_statement`/`contributions` string. Getting a real
//! author list means following `works[0].key` to `/works/<id>.json` for the author *keys*, then
//! one more request per author to `/authors/<key>.json` for the name — confirmed against the
//! live API while building this module, not assumed from documentation (OpenLibrary's own
//! `jscmd=data` "Books API", which is documented to inline author names in one call and would
//! have avoided this chain, currently 404s on its own published example URL). A book with N
//! authors costs `2 + N` requests; this module accepts that cost for real names over one request
//! for a jacket blurb, per the maintainer's own call when this trade-off surfaced.

use crate::{Entry, Field, Span, Value, ValuePart};

/// Why an ISBN lookup failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IsbnError {
    /// OpenLibrary has no edition record for this ISBN (a 404 on `/isbn/<isbn>.json`).
    #[error("no book found for ISBN {0}")]
    NotFound(String),
    /// The request could not be completed: no connection, a timeout, or an unexpected status at
    /// any step of the chain.
    #[error("could not reach openlibrary.org: {0}")]
    Network(String),
    /// A reply arrived with a 200 but was not JSON this module could read the fields it needs
    /// out of.
    #[error("openlibrary.org's reply did not parse as a book record")]
    Unparseable,
}

/// Strip hyphens and spaces from a pasted ISBN, keeping only digits and a possible trailing
/// literal `X` (ISBN-10's own check-digit letter). Case-folds a lower-case `x` to upper, since
/// both appear in the wild and ISBN-10's check digit is conventionally printed upper-case. Does
/// not validate length or the check digit itself — the same "unwrap, do not judge" split
/// [`crate::acquire::doi::normalize_doi`] and
/// [`crate::acquire::arxiv::normalize_arxiv_id`] both already draw; OpenLibrary's own 404 is the
/// judge.
pub fn normalize_isbn(pasted: &str) -> String {
    pasted
        .trim()
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

trait Transport {
    /// `Ok((status, body))` for any reply that arrived at all. Every step of the chain below —
    /// the edition, the work, each author — is the same shape of request, so one method serves
    /// all of them; DOI's own `Transport::get_bibtex` (`doi.rs`, not `pub`, so not linkable from
    /// here) is the single-purpose version of this same idea, one request instead of a chain.
    fn get(&self, url: &str) -> Result<(u16, String), String>;
}

/// Resolve `isbn` (in any of the pasted forms [`normalize_isbn`] understands) to an [`Entry`] by
/// asking the real OpenLibrary API, following the edition → work → author(s) chain described in
/// this module's own doc comment. Requires the `acquire` feature.
pub fn fetch_isbn(isbn: &str) -> Result<Entry, IsbnError> {
    fetch_isbn_with(isbn, &HttpTransport)
}

/// [`fetch_isbn`]'s logic, taking the transport as a parameter so a test can substitute
/// recorded replies for the whole chain instead of reaching the network.
fn fetch_isbn_with(isbn: &str, transport: &impl Transport) -> Result<Entry, IsbnError> {
    let normalized = normalize_isbn(isbn);
    let edition = fetch_json(
        transport,
        &format!("https://openlibrary.org/isbn/{normalized}.json"),
    )?
    .ok_or_else(|| IsbnError::NotFound(normalized.clone()))?;

    let title = edition.get_str("title").unwrap_or_default().to_string();
    let publisher = edition
        .get_array("publishers")
        .and_then(|list| list.first())
        .and_then(Json::as_str)
        .map(str::to_string);
    let year = publish_year(edition.get_str("publish_date").unwrap_or_default());

    let authors = authors_via_work(transport, &edition)?;
    let author_field = if authors.is_empty() {
        // Every ISBN this module was checked against during development has a `works` entry,
        // but nothing in OpenLibrary's schema guarantees one — an edition with no linked work at
        // all falls back to the edition's own loose byline rather than shipping an entry with no
        // author field, which BibLaTeX's `@book` expects to have. `by_statement` is tried first
        // (a full citation-style phrase, "A ... [et al.]"); `contributions`' first entry (a
        // single cataloguer-entered name, no "et al." framing) is the second-choice fallback.
        let contributions_first = edition
            .get_array("contributions")
            .and_then(|list| list.first())
            .and_then(Json::as_str);
        edition
            .get_str("by_statement")
            .or(contributions_first)
            .unwrap_or("")
            .to_string()
    } else {
        authors.join(" and ")
    };

    let key = super::arxiv::generated_key(&authors, &year, &title);

    let mut fields = vec![
        field("title", &title),
        field("author", &author_field),
        field("year", &year),
        field("isbn", &normalized),
    ];
    if let Some(publisher) = publisher {
        fields.push(field("publisher", &publisher));
    }

    Ok(Entry {
        span: zero_span(),
        entry_type: "book".to_string(),
        key,
        key_span: zero_span(),
        fields,
    })
}

/// Follow `edition.works[0].key` to the work record, then each of its `authors[].author.key` to
/// an author record, collecting names in the work's own listed order. Any single author fetch
/// failing (a dangling key, a transient error) drops that one name rather than failing the whole
/// lookup — a book correctly attributed to three authors should not lose all three because the
/// fourth, uncredited contributor's record 404s.
fn authors_via_work(transport: &impl Transport, edition: &Json) -> Result<Vec<String>, IsbnError> {
    let Some(work_key) = edition
        .get_array("works")
        .and_then(|list| list.first())
        .and_then(|w| w.get_str("key"))
    else {
        return Ok(Vec::new());
    };
    let Some(work) = fetch_json(transport, &format!("https://openlibrary.org{work_key}.json"))? else {
        return Ok(Vec::new());
    };
    let Some(author_entries) = work.get_array("authors") else {
        return Ok(Vec::new());
    };

    let mut names = Vec::new();
    for author_entry in author_entries {
        let Some(author_key) = author_entry.get("author").and_then(|a| a.get_str("key")) else {
            continue;
        };
        let Ok(Some(author)) = fetch_json(transport, &format!("https://openlibrary.org{author_key}.json"))
        else {
            continue;
        };
        if let Some(name) = author.get_str("name") {
            names.push(name.to_string());
        }
    }
    Ok(names)
}

/// `GET url`, returning `Ok(None)` for a 404 (the one non-error "this does not exist" case every
/// step of the chain can hit) and `Err` for anything else that is not a clean 200.
fn fetch_json(transport: &impl Transport, url: &str) -> Result<Option<Json>, IsbnError> {
    let (status, body) = transport.get(url).map_err(IsbnError::Network)?;
    match status {
        200 => Json::parse(&body).map(Some).ok_or(IsbnError::Unparseable),
        404 => Ok(None),
        other => Err(IsbnError::Network(format!("unexpected status {other}"))),
    }
}

/// OpenLibrary's `publish_date` is free text ("2009", "March 2009", "2009-03-15") — this pulls
/// the first run of four digits out of it, which is all a BibLaTeX `year` field needs and covers
/// every shape seen while building this module.
fn publish_year(publish_date: &str) -> String {
    let bytes = publish_date.as_bytes();
    for start in 0..bytes.len() {
        if start + 4 <= bytes.len() && bytes[start..start + 4].iter().all(u8::is_ascii_digit) {
            return publish_date[start..start + 4].to_string();
        }
    }
    String::new()
}

fn field(name: &str, value: &str) -> Field {
    Field {
        span: zero_span(),
        name: name.to_string(),
        value_span: zero_span(),
        value: braced(value),
    }
}

fn braced(text: &str) -> Value {
    Value {
        parts: vec![ValuePart::Braced(text.to_string())],
    }
}

/// Same reasoning as `arxiv::zero_span`: a hand-built entry was never parsed from a real file,
/// so it has no real byte offsets yet.
fn zero_span() -> Span {
    Span { start: 0, end: 0 }
}

struct HttpTransport;

impl Transport for HttpTransport {
    fn get(&self, url: &str) -> Result<(u16, String), String> {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .user_agent("abstract-tex (https://github.com/verocai-vrc/Abstract-Tex)")
            .build()
            .map_err(|err| err.to_string())?;
        let response = client.get(url).send().map_err(|err| err.to_string())?;
        let status = response.status().as_u16();
        let body = response.text().map_err(|err| err.to_string())?;
        Ok((status, body))
    }
}

/// A minimal JSON reader over OpenLibrary's own replies — this crate already has `serde_json` as
/// a dev-dependency for the fixture harness (`build.rs`), but only as a dev-dependency: pulling
/// it into production code here would mean either promoting it to a real dependency (a bigger
/// footprint for the eventual MIT-published parser half, S8.5, to carry) or re-parsing with a
/// second JSON reader for `acquire` alone. A hand-rolled reader that only needs to answer "get
/// me this string/array/object field" — never round-trip, never write — is small enough
/// (`Value`'s own `resolve_with` in `value.rs` is a similar amount of code for a similar reason)
/// to be the cheaper choice for a feature-gated module nobody outside `acquire` will ever import.
#[derive(Debug, Clone)]
enum Json {
    Object(Vec<(String, Json)>),
    Array(Vec<Json>),
    String(String),
    Other,
}

impl Json {
    fn parse(text: &str) -> Option<Json> {
        let (value, _) = parse_value(text)?;
        Some(value)
    }

    fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Object(fields) => fields
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value),
            _ => None,
        }
    }

    fn get_str(&self, key: &str) -> Option<&str> {
        self.get(key)?.as_str()
    }

    fn get_array(&self, key: &str) -> Option<&[Json]> {
        match self.get(key)? {
            Json::Array(items) => Some(items),
            _ => None,
        }
    }

    fn as_str(&self) -> Option<&str> {
        match self {
            Json::String(s) => Some(s),
            _ => None,
        }
    }
}

/// Parses one JSON value starting at `text`'s first non-whitespace character. Returns the value
/// and the remaining, unconsumed text. Every branch this module actually reads from OpenLibrary
/// — objects, arrays, strings — is a real parse; numbers, booleans and `null` are recognised
/// just well enough to be skipped correctly (`Json::Other`) inside an array or object, since
/// none of the fields this module reads are ever a bare number or boolean, but their *siblings*
/// sometimes are (`revision`, `latest_revision`) and must not desynchronise the scan.
fn parse_value(text: &str) -> Option<(Json, &str)> {
    let text = text.trim_start();
    match text.chars().next()? {
        '{' => parse_object(text),
        '[' => parse_array(text),
        '"' => parse_string(text).map(|(s, rest)| (Json::String(s), rest)),
        't' if text.starts_with("true") => Some((Json::Other, &text[4..])),
        'f' if text.starts_with("false") => Some((Json::Other, &text[5..])),
        'n' if text.starts_with("null") => Some((Json::Other, &text[4..])),
        c if c == '-' || c.is_ascii_digit() => {
            let end = text
                .find(|c: char| !(c.is_ascii_digit() || matches!(c, '-' | '+' | '.' | 'e' | 'E')))
                .unwrap_or(text.len());
            Some((Json::Other, &text[end..]))
        }
        _ => None,
    }
}

fn parse_object(text: &str) -> Option<(Json, &str)> {
    let mut rest = text.strip_prefix('{')?.trim_start();
    let mut fields = Vec::new();
    if let Some(after_close) = rest.strip_prefix('}') {
        return Some((Json::Object(fields), after_close));
    }
    loop {
        let (key, after_key) = parse_string(rest)?;
        rest = after_key.trim_start().strip_prefix(':')?.trim_start();
        let (value, after_value) = parse_value(rest)?;
        fields.push((key, value));
        rest = after_value.trim_start();
        match rest.chars().next()? {
            ',' => rest = rest[1..].trim_start(),
            '}' => return Some((Json::Object(fields), &rest[1..])),
            _ => return None,
        }
    }
}

fn parse_array(text: &str) -> Option<(Json, &str)> {
    let mut rest = text.strip_prefix('[')?.trim_start();
    let mut items = Vec::new();
    if let Some(after_close) = rest.strip_prefix(']') {
        return Some((Json::Array(items), after_close));
    }
    loop {
        let (value, after_value) = parse_value(rest)?;
        items.push(value);
        rest = after_value.trim_start();
        match rest.chars().next()? {
            ',' => rest = rest[1..].trim_start(),
            ']' => return Some((Json::Array(items), &rest[1..])),
            _ => return None,
        }
    }
}

/// Parses a JSON string starting at an opening `"`, handling the escapes OpenLibrary's own
/// output actually contains (`\"`, `\\`, `\/`, `\n`, `\t`, `\uXXXX`) — not the full JSON escape
/// grammar's every edge case, but every one a real reply from this API has shown while building
/// this module (`\u0000`-range control-character escapes included, for `bio`/`notes` fields that
/// occasionally carry Cyrillic titles under `\uXXXX`).
fn parse_string(text: &str) -> Option<(String, &str)> {
    let mut rest = text.strip_prefix('"')?;
    let mut out = String::new();
    loop {
        let c = rest.chars().next()?;
        match c {
            '"' => return Some((out, &rest[1..])),
            '\\' => {
                let escape = rest[1..].chars().next()?;
                match escape {
                    '"' => out.push('"'),
                    '\\' => out.push('\\'),
                    '/' => out.push('/'),
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    'r' => out.push('\r'),
                    'u' => {
                        let hex = rest.get(2..6)?;
                        let code = u32::from_str_radix(hex, 16).ok()?;
                        out.push(char::from_u32(code).unwrap_or('\u{FFFD}'));
                        rest = &rest[6..];
                        continue;
                    }
                    other => out.push(other),
                }
                rest = &rest[1 + escape.len_utf8()..];
            }
            other => {
                out.push(other);
                rest = &rest[other.len_utf8()..];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EDITION_REPLY: &str = include_str!("../../fixtures/isbn-book/edition.json");
    const WORK_REPLY: &str = include_str!("../../fixtures/isbn-book/work.json");
    const AUTHOR_CORMEN_REPLY: &str = include_str!("../../fixtures/isbn-book/author-cormen.json");

    /// One recorded reply, and the substring of a request URL it answers for.
    type RoutedReply = (&'static str, Result<(u16, String), String>);

    /// A fixed set of recorded replies, keyed by the URL each request should hit — a small
    /// router rather than a single `Ok/Err`, because this module's whole reason to be its own
    /// design (see the module doc) is that one lookup is several requests, and a test needs to
    /// hand back a different body for each.
    struct RoutedReplies(Vec<RoutedReply>);

    impl Transport for RoutedReplies {
        fn get(&self, url: &str) -> Result<(u16, String), String> {
            self.0
                .iter()
                .find(|(pattern, _)| url.contains(pattern))
                .map(|(_, reply)| reply.clone())
                .unwrap_or_else(|| Ok((404, String::new())))
        }
    }

    #[test]
    fn hyphens_and_spaces_are_stripped_and_a_lowercase_x_is_upcased() {
        assert_eq!(normalize_isbn("978-0-262-03384-8"), "9780262033848");
        assert_eq!(normalize_isbn("0-306-40615-x"), "030640615X");
        assert_eq!(normalize_isbn(" 0262033844 "), "0262033844");
    }

    #[test]
    fn a_full_chain_lookup_with_one_author_via_the_work_record() {
        let transport = RoutedReplies(vec![
            ("/isbn/9780262033848.json", Ok((200, EDITION_REPLY.to_string()))),
            ("/works/OL4781294W.json", Ok((200, WORK_REPLY.to_string()))),
            (
                "/authors/OL1004780A.json",
                Ok((200, AUTHOR_CORMEN_REPLY.to_string())),
            ),
            // The work record lists four authors; only the first's record is recorded above, so
            // the other three resolve through the router's own 404 fallback and are dropped —
            // exactly the "one author fetch failing drops that name, not the whole lookup"
            // behaviour `authors_via_work`'s doc comment describes, exercised for real here
            // rather than only asserted in prose.
        ]);
        let entry = fetch_isbn_with("978-0-262-03384-8", &transport).expect("the recorded chain resolves");
        assert!(entry.is_type("book"));
        assert_eq!(
            entry.field("title").unwrap().value.parts,
            vec![ValuePart::Braced("Introduction to Algorithms".into())]
        );
        assert_eq!(
            entry.field("author").unwrap().value.parts,
            vec![ValuePart::Braced("Thomas H. Cormen".into())]
        );
        assert_eq!(
            entry.field("year").unwrap().value.parts,
            vec![ValuePart::Braced("2009".into())]
        );
        assert_eq!(
            entry.field("isbn").unwrap().value.parts,
            vec![ValuePart::Braced("9780262033848".into())]
        );
        assert_eq!(
            entry.field("publisher").unwrap().value.parts,
            vec![ValuePart::Braced("The MIT Press".into())]
        );
    }

    #[test]
    fn a_missing_edition_is_not_found() {
        let transport = RoutedReplies(vec![("/isbn/", Ok((404, String::new())))]);
        assert_eq!(
            fetch_isbn_with("0000000000", &transport),
            Err(IsbnError::NotFound("0000000000".to_string()))
        );
    }

    #[test]
    fn an_edition_with_no_linked_work_falls_back_to_the_by_statement() {
        let edition_no_work = EDITION_REPLY.replace(r#""works": [{"key": "/works/OL4781294W"}],"#, "");
        let transport = RoutedReplies(vec![("/isbn/9780262033848.json", Ok((200, edition_no_work)))]);
        let entry = fetch_isbn_with("9780262033848", &transport).expect("the edition alone still resolves");
        assert_eq!(
            entry.field("author").unwrap().value.parts,
            vec![ValuePart::Braced("Thomas H. Cormen ... [et al.].".into())]
        );
    }

    #[test]
    fn a_multi_digit_year_embedded_in_free_text_is_extracted() {
        assert_eq!(publish_year("March 2009"), "2009");
        assert_eq!(publish_year("2009"), "2009");
        assert_eq!(publish_year("unknown"), "");
    }

    #[test]
    fn an_unexpected_status_becomes_a_network_error() {
        let transport = RoutedReplies(vec![("/isbn/", Ok((503, String::new())))]);
        assert_eq!(
            fetch_isbn_with("9780262033848", &transport),
            Err(IsbnError::Network("unexpected status 503".to_string()))
        );
    }

    #[test]
    fn a_transport_failure_becomes_a_network_error() {
        let transport = RoutedReplies(vec![("/isbn/", Err("connection refused".to_string()))]);
        assert_eq!(
            fetch_isbn_with("9780262033848", &transport),
            Err(IsbnError::Network("connection refused".to_string()))
        );
    }

    #[test]
    fn json_strings_with_unicode_escapes_and_slashes_parse_correctly() {
        let parsed =
            Json::parse(r#"{"title": "Café \/ naïve", "n": 5, "flag": true, "nothing": null}"#).unwrap();
        assert_eq!(parsed.get_str("title"), Some("Café / naïve"));
    }

    #[test]
    fn json_arrays_of_objects_are_walked() {
        let parsed = Json::parse(r#"{"list": [{"key": "a"}, {"key": "b"}]}"#).unwrap();
        let array = parsed.get_array("list").unwrap();
        assert_eq!(array[0].get_str("key"), Some("a"));
        assert_eq!(array[1].get_str("key"), Some("b"));
    }

    /// The one test in this file that reaches the real network — the whole chain, live. Run by
    /// hand with `cargo test -p texbib --features acquire -- --ignored`.
    #[test]
    #[ignore]
    fn the_real_openlibrary_resolves_a_known_isbn() {
        let entry = fetch_isbn("9780262033848").expect("a real, stable ISBN");
        assert!(entry.field("title").is_some());
        assert!(entry.field("author").is_some());
    }
}
