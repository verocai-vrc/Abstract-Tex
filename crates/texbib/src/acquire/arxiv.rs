//! Resolves an arXiv identifier to a BibLaTeX `@online` entry through arXiv's own Atom API
//! (`export.arxiv.org/api/query?id_list=<id>`) — no API key, no service of ours in between
//! (DESIGN.md §5.4).
//!
//! The reply is Atom, not JSON, and this module reads exactly the handful of tags a citation
//! needs (`title`, `author/name`, `summary`, `published`, the canonical id) with plain
//! substring scanning rather than a general XML parser dependency — the same "hand-written
//! subset, not a whole library" choice this project already made for LSP's protocol types
//! (`src/lib/lsp-protocol.ts`), and safe here because arXiv's own feed is a fixed, documented
//! shape this module does not need to parse in general.

use crate::{Entry, Field, Span, Value, ValuePart};

/// Why an arXiv lookup failed. Each variant is the one sentence a caller should show the author.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ArxivError {
    /// The identifier is not shaped like an arXiv id at all — arXiv answered with its own
    /// `incorrect id format` error entry (an HTTP 400).
    #[error("'{0}' is not a recognisable arXiv identifier")]
    NotAnArxivId(String),
    /// The identifier is shaped correctly, but arXiv's own search returned no entry for it —
    /// `<opensearch:totalResults>0</opensearch:totalResults>` with no `<entry>` at all.
    #[error("no arXiv paper found for {0}")]
    NotFound(String),
    /// The request could not be completed: no connection, a timeout, or a status this module
    /// did not expect.
    #[error("could not reach arxiv.org: {0}")]
    Network(String),
    /// arXiv answered with a 200, but the body was not a feed this module could read the fields
    /// it needs out of — arXiv's own reply shape changing underneath it, in practice.
    #[error("arxiv.org's reply did not parse as an entry")]
    Unparseable,
}

/// Strip a known arXiv wrapper — an `https://arxiv.org/abs/…`, `.../pdf/…`, or `arXiv:` prefix
/// — case-insensitively, and trim whitespace and a trailing `.pdf`. What is left is handed to
/// arXiv's own API unchanged, the same "only remove wrapping, let the source judge the rest"
/// rule [`crate::acquire::doi::normalize_doi`] already follows. An arXiv id has two live shapes:
/// new-style `YYMM.NNNNN[vN]` (2007 onward) and old-style `archive/YYMMNNN` (e.g.
/// `hep-th/9901001`); this function does not choose between them, it only unwraps.
pub fn normalize_arxiv_id(pasted: &str) -> String {
    const PREFIXES: [&str; 5] = [
        "https://arxiv.org/abs/",
        "http://arxiv.org/abs/",
        "https://arxiv.org/pdf/",
        "http://arxiv.org/pdf/",
        "arxiv:",
    ];
    let trimmed = pasted.trim();
    for prefix in PREFIXES {
        if trimmed
            .get(..prefix.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
        {
            let rest = trimmed[prefix.len()..].trim();
            return rest.strip_suffix(".pdf").unwrap_or(rest).to_string();
        }
    }
    trimmed.to_string()
}

/// Drops a trailing `v` followed by one or more digits (`1706.03762v7` -> `1706.03762`,
/// `hep-th/9901001v2` -> `hep-th/9901001`), or returns `id` unchanged if it has none. arXiv
/// version numbers are always small (single or double digit in practice), but this accepts any
/// digit run rather than guessing a bound. `pub(super)` rather than private: shared with
/// [`crate::acquire::identify`], which needs the same unwrapping to recognise an id's shape
/// without being fooled by a version suffix it has not yet decided is one.
pub(super) fn strip_version_suffix(id: &str) -> &str {
    let Some(v_pos) = id.rfind('v') else { return id };
    let digits = &id[v_pos + 1..];
    if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
        &id[..v_pos]
    } else {
        id
    }
}

trait Transport {
    /// `Ok((status, body))` for any reply that arrived at all. `Err` is reserved for a request
    /// that never got a status back.
    fn get(&self, url: &str) -> Result<(u16, String), String>;
}

/// Resolve `id` (in any of the pasted forms [`normalize_arxiv_id`] understands) to an [`Entry`]
/// by asking the real arXiv API. Requires the `acquire` feature.
pub fn fetch_arxiv(id: &str) -> Result<Entry, ArxivError> {
    fetch_arxiv_with(id, &HttpTransport)
}

/// [`fetch_arxiv`]'s logic, taking the transport as a parameter so a test can substitute a
/// fixture reply for a real request.
fn fetch_arxiv_with(id: &str, transport: &impl Transport) -> Result<Entry, ArxivError> {
    let normalized = normalize_arxiv_id(id);
    let url = format!("https://export.arxiv.org/api/query?id_list={normalized}");
    let (status, body) = transport.get(&url).map_err(ArxivError::Network)?;
    match status {
        200 => entry_from_feed(&body).ok_or_else(|| ArxivError::NotFound(normalized.clone())),
        400 => Err(ArxivError::NotAnArxivId(normalized)),
        other => Err(ArxivError::Network(format!("unexpected status {other}"))),
    }
}

/// Build an [`Entry`] from arXiv's Atom feed body, or `None` if the feed has no real `<entry>`
/// — either because `totalResults` was `0` (a well-formed id arXiv has never heard of) or
/// because the one `<entry>` present is arXiv's own error entry (`id` containing
/// `arxiv.org/api/errors`, the shape a malformed id produces alongside the 400 status this
/// function's caller already separates out).
fn entry_from_feed(body: &str) -> Option<Entry> {
    let entry_xml = tag_content(body, "entry")?;
    // `<id>` is a full URL, `http://arxiv.org/abs/1706.03762v7` — arXiv's own canonical
    // resolution of the request, and it always carries a version number even when the caller's
    // own `id_list=` gave none. Reading the bare id back out of *this* rather than out of
    // whatever the author pasted is why `entry_from_feed` no longer needs the pasted id passed
    // in at all: one source of truth (what arXiv resolved to) instead of two format-specific
    // guesses (old-style vs. new-style) about where a hand-typed id's version marker can and
    // cannot appear.
    let entry_id = tag_content(entry_xml, "id").unwrap_or_default();
    if entry_id.contains("api/errors") {
        return None;
    }

    let title = collapse_whitespace(&decode_entities(tag_content(entry_xml, "title")?));
    let summary = tag_content(entry_xml, "summary").map(|s| collapse_whitespace(&decode_entities(s)));
    let published = tag_content(entry_xml, "published").unwrap_or_default();
    let year = published.get(..4).unwrap_or_default();
    let authors = author_names(entry_xml);

    // The bare id without a version suffix (`1706.03762v7` -> `1706.03762`) is what a citation
    // should key on and what the `eprint` field names: a reader following the citation wants
    // the paper, and arXiv's own abstract page redirects any version to the latest by default.
    let id_with_version = entry_id.rsplit('/').next().unwrap_or(entry_id);
    let bare_id = strip_version_suffix(id_with_version);

    let mut fields = vec![
        field("title", &title),
        field("author", &authors.join(" and ")),
        field("year", year),
        field("eprint", bare_id),
        field("eprinttype", "arxiv"),
        field("url", &format!("https://arxiv.org/abs/{bare_id}")),
    ];
    if let Some(summary) = summary {
        fields.push(field("abstract", &summary));
    }

    let key = generated_key(&authors, year, &title);
    Some(Entry {
        span: zero_span(),
        entry_type: "online".to_string(),
        key,
        key_span: zero_span(),
        fields,
    })
}

/// Every `<author><name>…</name></author>` in an entry, in document order. arXiv's own feed
/// never omits the author list (unlike a title or summary, which this function still guards
/// with `Option`), but an empty `Vec` here is handled the same as anywhere else a caller joins
/// authors with `" and "` — an empty string, not a panic.
fn author_names(entry_xml: &str) -> Vec<String> {
    let mut authors = Vec::new();
    let mut rest = entry_xml;
    while let Some(author_start) = rest.find("<author>") {
        let after_open = &rest[author_start + "<author>".len()..];
        let Some(author_end) = after_open.find("</author>") else {
            break;
        };
        let author_xml = &after_open[..author_end];
        if let Some(name) = tag_content(author_xml, "name") {
            authors.push(collapse_whitespace(&decode_entities(name)));
        }
        rest = &after_open[author_end + "</author>".len()..];
    }
    authors
}

/// The text strictly between the first `<tag>` and its matching `</tag>` in `xml`, or `None` if
/// either is missing. Deliberately naive — no attribute handling, no nesting awareness beyond
/// "first open, first matching close" — because every tag this module reads
/// (`entry`/`title`/`summary`/`published`/`id`/`name`) is a leaf or, for `entry`, is only ever
/// asked for once per feed at the top level, so the first close is always the right one.
fn tag_content<'a>(xml: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml.find(&open)? + open.len();
    let end = start + xml[start..].find(&close)?;
    Some(&xml[start..end])
}

/// The handful of entities arXiv's feed actually uses in a title or summary. Not a general XML
/// decoder — `&#NNN;` numeric references do not appear in practice here, and adding a rule for
/// them speculatively is exactly the kind of guessed-at scope `CLAUDE.md` asks this codebase to
/// avoid.
fn decode_entities(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&apos;", "'")
        .replace("&quot;", "\"")
}

/// arXiv wraps a summary across many lines at a fixed column, the same wrapping shape
/// `texlog::tokenizer` already has to undo in TeX's own log output — collapsed here the same
/// way `texbib::value`'s `push_collapsed` already collapses a `.bib` field's own whitespace, so
/// the two crates' fixtures do not silently drift into two different ideas of "one line".
fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
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

/// A hand-built [`Entry`] has no byte offsets into any real `.bib` file — it was never parsed
/// from one — so every span on it is `0..0`. [`crate::acquire::doi::fetch_doi`] never needs
/// this, since its entry comes back through [`crate::parse()`] on the publisher's own BibTeX
/// text and inherits real spans from it; arXiv and ISBN hand back structured data instead, with
/// nothing for a span to point at until `texbib::render` (S7.6, not written yet) writes this
/// entry out as text for the first time. A caller must not read these spans as real positions.
fn zero_span() -> Span {
    Span { start: 0, end: 0 }
}

/// `surnameYEARfirstword`, ASCII-folded and lower-cased — enough to dedupe two lookups of the
/// same paper or book against each other and against an existing `.bib` entry by eye, which is
/// all this loop's own done-when asks for ("an entry with a stable generated key"). Shared with
/// [`crate::acquire::isbn`] rather than duplicated, since the shape is identical either way: one
/// generated key format for the two sources whose `Entry` this module builds by hand, not
/// parsed from someone else's text the way `doi::fetch_doi`'s is. S7.6's own key generator
/// (`crates/texbib/src/keys.rs`, not written yet) is free to replace this with whatever
/// collision-avoidance and capitalisation convention it settles on once it exists and has a real
/// index to check uniqueness against — the same deferral S7.3 made for name-splitting rather
/// than guess a shape nothing calls yet.
pub(super) fn generated_key(authors: &[String], year: &str, title: &str) -> String {
    let surname = authors
        .first()
        .map(|name| surname_of(name))
        .unwrap_or_else(|| "unknown".to_string());
    let first_word = title
        .split_whitespace()
        .find(|word| word.chars().any(char::is_alphanumeric))
        .unwrap_or("");
    format!("{surname}{year}{}", ascii_fold_lower(first_word))
}

/// The last space-separated word of a full name (`"Ashish Vaswani"` -> `"vaswani"`), which is
/// the common case arXiv's `<name>` and OpenLibrary's author records both use; a single-word
/// corporate or mononymous name passes through as itself.
fn surname_of(full_name: &str) -> String {
    ascii_fold_lower(full_name.split_whitespace().last().unwrap_or(full_name))
}

/// Lower-cases and keeps only ASCII letters and digits — good enough for a key a BibTeX/Biber
/// engine will accept everywhere, not an attempt at real transliteration (`Müller` becomes
/// `mller`, not `mueller`); a health check or the author's own eye is the backstop, the same
/// division of labour `texbib`'s own module doc already draws between this crate and its
/// callers.
fn ascii_fold_lower(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Recorded from a real `export.arxiv.org/api/query?id_list=2101.00001` reply — one author,
    /// a French-language paper (so the summary starts in English but the title has an accented
    /// apostrophe), an `arxiv:comment` tag this module deliberately does not read.
    const ONE_AUTHOR_REPLY: &str = include_str!("../../fixtures/arxiv-one-author/reply.xml");

    /// Recorded from `id_list=1706.03762` — eight authors, the case that proves author joining
    /// and not just author *presence*.
    const EIGHT_AUTHORS_REPLY: &str = include_str!("../../fixtures/arxiv-eight-authors/reply.xml");

    /// Recorded from `id_list=9999.99999` — a well-formed id arXiv has never heard of:
    /// `totalResults` is `0` and there is no `<entry>` at all, unlike the malformed-id case
    /// below, which is a 400 with an error `<entry>`.
    const NOT_FOUND_REPLY: &str = include_str!("../../fixtures/arxiv-not-found/reply.xml");

    /// Recorded from `id_list=nosuchid9999` — a 400 whose body is itself a feed, with one
    /// `<entry>` whose `id` names `api/errors`.
    const MALFORMED_ID_REPLY: &str = include_str!("../../fixtures/arxiv-malformed-id/reply.xml");

    struct FixedReply(Result<(u16, String), String>);

    impl Transport for FixedReply {
        fn get(&self, _url: &str) -> Result<(u16, String), String> {
            self.0.clone()
        }
    }

    #[test]
    fn an_abs_url_normalizes_to_the_bare_id() {
        assert_eq!(
            normalize_arxiv_id("https://arxiv.org/abs/1706.03762"),
            "1706.03762"
        );
    }

    #[test]
    fn a_pdf_url_normalizes_to_the_bare_id() {
        assert_eq!(
            normalize_arxiv_id("https://arxiv.org/pdf/1706.03762.pdf"),
            "1706.03762"
        );
    }

    #[test]
    fn an_arxiv_scheme_normalizes_the_same_way() {
        assert_eq!(normalize_arxiv_id("arXiv:1706.03762"), "1706.03762");
    }

    #[test]
    fn a_version_suffix_survives_normalization() {
        assert_eq!(normalize_arxiv_id("arXiv:1706.03762v7"), "1706.03762v7");
    }

    #[test]
    fn an_already_bare_id_passes_through_unchanged() {
        assert_eq!(normalize_arxiv_id("hep-th/9901001"), "hep-th/9901001");
    }

    #[test]
    fn a_single_author_entry_produces_the_expected_entry() {
        let transport = FixedReply(Ok((200, ONE_AUTHOR_REPLY.to_string())));
        let entry = fetch_arxiv_with("2101.00001", &transport).expect("the recorded reply parses");
        assert!(entry.is_type("online"));
        assert_eq!(
            entry.field("title").unwrap().value.parts,
            vec![ValuePart::Braced(
                "Etat de l'art sur l'application des bandits multi-bras".into()
            )]
        );
        assert_eq!(
            entry.field("author").unwrap().value.parts,
            vec![ValuePart::Braced("Djallel Bouneffouf".into())]
        );
        assert_eq!(
            entry.field("year").unwrap().value.parts,
            vec![ValuePart::Braced("2021".into())]
        );
        assert_eq!(
            entry.field("eprint").unwrap().value.parts,
            vec![ValuePart::Braced("2101.00001".into())]
        );
        assert_eq!(
            entry.field("eprinttype").unwrap().value.parts,
            vec![ValuePart::Braced("arxiv".into())]
        );
        assert_eq!(entry.key, "bouneffouf2021etat");
    }

    #[test]
    fn eight_authors_are_joined_with_and() {
        let transport = FixedReply(Ok((200, EIGHT_AUTHORS_REPLY.to_string())));
        let entry = fetch_arxiv_with("1706.03762", &transport).expect("the recorded reply parses");
        let author_field = entry.field("author").unwrap();
        let ValuePart::Braced(joined) = &author_field.value.parts[0] else {
            panic!("expected a braced value")
        };
        assert_eq!(joined.matches(" and ").count(), 7, "{joined}");
        assert!(joined.starts_with("Ashish Vaswani"), "{joined}");
        assert_eq!(entry.key, "vaswani2017attention");
    }

    #[test]
    fn a_pasted_id_with_no_version_still_gets_a_version_free_eprint_from_the_feeds_own_id() {
        // Regression: an earlier version of this function derived `bare_id` from the *pasted*
        // id (`normalized_id.split('v').next()`), which only "worked" for a versionless paste
        // because there was no `v` to split on — the wrong source of truth in general, since
        // arXiv's own `<entry><id>` always carries a version (confirmed live: EIGHT_AUTHORS_REPLY
        // was captured from `id_list=1706.03762`, with no version requested, and still answers
        // `.../abs/1706.03762v7`). Deriving `bare_id` from `entry_id` instead makes this pass for
        // the reason that matters, not by the coincidence of there being no `v` to trip over.
        let transport = FixedReply(Ok((200, EIGHT_AUTHORS_REPLY.to_string())));
        let entry = fetch_arxiv_with("1706.03762", &transport).expect("the recorded reply parses");
        assert_eq!(
            entry.field("eprint").unwrap().value.parts,
            vec![ValuePart::Braced("1706.03762".into())]
        );
        assert_eq!(
            entry.field("url").unwrap().value.parts,
            vec![ValuePart::Braced("https://arxiv.org/abs/1706.03762".into())]
        );
    }

    #[test]
    fn a_version_suffix_is_stripped_from_the_eprint_and_key_fields_but_not_from_the_lookup() {
        let transport = FixedReply(Ok((200, EIGHT_AUTHORS_REPLY.to_string())));
        let entry = fetch_arxiv_with("1706.03762v7", &transport).expect("the recorded reply parses");
        assert_eq!(
            entry.field("eprint").unwrap().value.parts,
            vec![ValuePart::Braced("1706.03762".into())]
        );
        assert_eq!(
            entry.field("url").unwrap().value.parts,
            vec![ValuePart::Braced("https://arxiv.org/abs/1706.03762".into())]
        );
    }

    #[test]
    fn a_well_formed_but_unknown_id_is_not_found() {
        let transport = FixedReply(Ok((200, NOT_FOUND_REPLY.to_string())));
        assert_eq!(
            fetch_arxiv_with("9999.99999", &transport),
            Err(ArxivError::NotFound("9999.99999".to_string()))
        );
    }

    #[test]
    fn a_malformed_id_is_reported_as_such_not_as_not_found() {
        let transport = FixedReply(Ok((400, MALFORMED_ID_REPLY.to_string())));
        assert_eq!(
            fetch_arxiv_with("nosuchid9999", &transport),
            Err(ArxivError::NotAnArxivId("nosuchid9999".to_string()))
        );
    }

    #[test]
    fn an_unexpected_status_becomes_a_network_error() {
        let transport = FixedReply(Ok((503, String::new())));
        assert_eq!(
            fetch_arxiv_with("1706.03762", &transport),
            Err(ArxivError::Network("unexpected status 503".to_string()))
        );
    }

    #[test]
    fn a_transport_failure_becomes_a_network_error() {
        let transport = FixedReply(Err("connection refused".to_string()));
        assert_eq!(
            fetch_arxiv_with("1706.03762", &transport),
            Err(ArxivError::Network("connection refused".to_string()))
        );
    }

    #[test]
    fn html_entities_in_a_title_are_decoded() {
        let reply = ONE_AUTHOR_REPLY.replace("l'art", "l&apos;art &amp; plus");
        let transport = FixedReply(Ok((200, reply)));
        let entry = fetch_arxiv_with("2101.00001", &transport).expect("the recorded reply parses");
        let ValuePart::Braced(title) = &entry.field("title").unwrap().value.parts[0] else {
            panic!("expected a braced value")
        };
        assert!(title.contains("l'art & plus"), "{title}");
    }

    /// The one test in this file that reaches the real network. Run by hand with
    /// `cargo test -p texbib --features acquire -- --ignored`.
    #[test]
    #[ignore]
    fn the_real_arxiv_resolves_a_known_paper() {
        let entry = fetch_arxiv("1706.03762").expect("a real, stable arXiv id");
        assert!(entry.field("title").is_some());
        assert!(entry.field("author").is_some());
    }
}
