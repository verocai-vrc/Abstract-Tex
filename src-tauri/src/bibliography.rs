//! The project-wide bibliography index (DESIGN.md §5.4): which `.bib` files the document uses,
//! which keys they define, and which `.tex` files cite what.
//!
//! Owns three things, all built fresh from disk by [`build_index`]:
//!
//! 1. The list of `.bib` files the document names — `\bibliography{a,b}` (BibTeX, `.bib`
//!    implied) and `\addbibresource{a.bib}` (BibLaTeX, extension written out) — found by
//!    scanning every `.tex` file in the include graph (`preamble-includes`, S4.1). A file the
//!    document names but that is not on disk is still listed, with `exists: false`, so the UI
//!    can say so; a silently dropped file would look like a bibliography with no entries.
//! 2. One [`EntrySummary`] per entry in those files — type, author, year, title, which file,
//!    and the entry's byte span in it — which is what `\cite` completion (S7.3) shows and
//!    paste-to-cite (S7.6) deduplicates against.
//! 3. Every `\cite`-family key in the document's `.tex` files, with file and line, which is
//!    what "undefined citation" and "never cited" health checks (S8.3) will read.
//!
//! It must never write a file, and it must never keep the `.bib` file's text hostage: the
//! `.bib` is the author's (DESIGN.md §2, rule 1), so this module reads it through `texbib`,
//! summarises, and forgets. It deliberately does not import Tauri either, so it can be tested
//! on a machine that cannot link the app crate (see the S4.1 note in `SPRINTS.md`).

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use preamble_includes::build_graph;
use serde::Serialize;
use texbib::{Bibliography, Entry, Span};

/// Everything the frontend needs about the project's bibliography, sent whole as the payload of
/// the `bibliography:changed` event and as the answer to the `bibliography_index` command.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BibliographyIndex {
    /// Every `.bib` the document names, in the order it names them, each listed once.
    pub files: Vec<BibFile>,
    /// Every entry in every existing file, in file order. Keys are not deduplicated here: a
    /// key defined twice is a health-check finding (S8.3), and hiding one copy would hide it.
    pub entries: Vec<EntrySummary>,
    /// Every citation in the document's `.tex` files, in include-graph order then line order.
    pub citations: Vec<Citation>,
}

impl BibliographyIndex {
    /// The first entry with exactly this key. BibTeX compares keys case-insensitively and
    /// Biber exactly; exact is the comparison that is never wrong about an existing entry.
    pub fn entry(&self, key: &str) -> Option<&EntrySummary> {
        self.entries.iter().find(|entry| entry.key == key)
    }
}

/// One `.bib` file the document names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BibFile {
    /// Project-relative, forward slashes — the same spelling `ProjectInfo.documentFiles` uses.
    /// For a resource that resolves outside the project this is the argument as written.
    pub path: String,
    /// `false` when the document names a file that is not on disk, or one outside the project
    /// (which this app never reads — the same rule `Project::resolve` applies to every path).
    pub exists: bool,
    /// How many entries parsed. Zero for a missing file.
    pub entry_count: usize,
    /// One sentence per item `texbib` could not parse, with the byte offset it gave up at.
    pub problems: Vec<Problem>,
}

/// A malformed item in a `.bib` file, as `texbib` reported it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    pub message: String,
    /// Byte offset into the file.
    pub at: usize,
}

/// What `\cite` completion needs to know about one entry, and no more. Field values are
/// resolved (`@string`s and month macros substituted, whitespace collapsed) but otherwise as
/// written: `{DNA}` keeps its braces, `Smith, Jane and Doe, John` is one string. Splitting
/// names is S7.3's `texbib::names`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntrySummary {
    pub key: String,
    /// As written (`article`, `Article`); compare ignoring case.
    pub entry_type: String,
    /// `author`, or `editor` when there is no author, or the same from a `crossref` parent.
    pub author: Option<String>,
    /// `year`, or the first four digits of a BibLaTeX `date`, or the same from a `crossref`
    /// parent.
    pub year: Option<String>,
    pub title: Option<String>,
    /// The `doi` field, normalised through `texbib::acquire::doi::normalize_doi` so a paste of
    /// `https://doi.org/10.…` compares equal to an entry whose field already reads bare
    /// `10.…` — S7.6's dedup key, not shown anywhere.
    pub doi: Option<String>,
    /// The `eprint` field when `eprinttype` is (case-insensitively) `arxiv`, normalised through
    /// `texbib::acquire::arxiv::normalize_arxiv_id` — S7.6's dedup key for a pasted arXiv id.
    pub eprint: Option<String>,
    /// The `isbn` field, normalised through `texbib::acquire::isbn::normalize_isbn` (digits and
    /// a possible trailing check digit only) — S7.6's dedup key for a pasted ISBN.
    pub isbn: Option<String>,
    /// Project-relative path of the `.bib` this entry is in.
    pub file: String,
    /// Byte span of the whole entry in that file (`texbib::Span`), so S7.6 can append after the
    /// last one and S8.3 can point at one. Bytes, not UTF-16 units: converting is the job of
    /// whoever opens the file in an editor.
    pub span: Span,
}

/// One key inside a `\cite{...}` in one `.tex` file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Citation {
    pub key: String,
    /// Project-relative path of the `.tex` file.
    pub file: String,
    /// 1-based line of the `\cite` command.
    pub line: u32,
}

/// Whether a change to `path` can change the index: any `.bib`, and any `.tex` (a `.tex` can
/// add a `\cite` or name a new `.bib`). Everything else — a `.sty`, a PDF — cannot.
pub fn affects_index(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("bib") || ext.eq_ignore_ascii_case("tex"))
}

/// Build the index for the document rooted at `root_file` (project-relative) in `project_dir`.
///
/// Walks the include graph, scans each existing `.tex` for bibliography resources and
/// citations, then parses each `.bib` through `texbib`. Never fails: a file that cannot be
/// read is simply one with no citations, and a malformed `.bib` item is a [`Problem`] on its
/// file — the same "never fail, report what you saw" rule the parser itself follows.
pub fn build_index(project_dir: &Path, root_file: &Path) -> BibliographyIndex {
    let graph = build_graph(project_dir, root_file);
    // `\bibliography{refs}` resolves against the root file's directory, exactly as `\input`
    // does and for the same reason: paths are relative to where the engine runs.
    let base_dir = root_file.parent().map(Path::to_path_buf).unwrap_or_default();

    let mut files: Vec<BibFile> = Vec::new();
    let mut citations: Vec<Citation> = Vec::new();

    for node in graph.nodes.iter().filter(|node| node.exists) {
        let Ok(text) = std::fs::read_to_string(project_dir.join(&node.path)) else { continue };

        for resource in scan_bib_resources(&text) {
            let (path, inside_project) = match resolve_bib_argument(&base_dir, &resource) {
                Some(resolved) => (resolved, true),
                None => (resource.clone(), false),
            };
            if files.iter().any(|file| file.path == path) {
                continue;
            }
            let exists = inside_project && project_dir.join(&path).is_file();
            files.push(BibFile { path, exists, entry_count: 0, problems: Vec::new() });
        }

        for (key, line) in scan_citations(&text) {
            citations.push(Citation { key, file: node.path.clone(), line });
        }
    }

    let mut entries: Vec<EntrySummary> = Vec::new();
    for file in files.iter_mut().filter(|file| file.exists) {
        let Ok(text) = std::fs::read_to_string(project_dir.join(&file.path)) else {
            file.exists = false;
            continue;
        };
        let bibliography = texbib::parse(&text);
        file.entry_count = bibliography.entries().count();
        file.problems = bibliography
            .errors()
            .map(|error| Problem { message: error.message.clone(), at: error.at })
            .collect();
        entries.extend(summarise_file(&file.path, &bibliography));
    }

    BibliographyIndex { files, entries, citations }
}

/// One summary per entry of one parsed file, with `crossref` inheritance applied within it.
///
/// BibTeX's rule is that a child entry takes any field it lacks from the entry its `crossref`
/// names, and this is the first place in the app that needs to know two entries are one: an
/// `@inproceedings` that carries only its own title and a `crossref` to the `@proceedings` still
/// has a year for its completion label. One level only, as BibTeX does; parents are looked up
/// in the same file, because that is where every tool that writes `crossref` puts them.
fn summarise_file(path: &str, bibliography: &Bibliography) -> Vec<EntrySummary> {
    // BibTeX compares the `crossref` value to keys case-insensitively, so the map is keyed by
    // the lower-cased key. First definition wins, as it does for BibTeX.
    let mut by_key: HashMap<String, &Entry> = HashMap::new();
    for entry in bibliography.entries() {
        by_key.entry(entry.key.to_ascii_lowercase()).or_insert(entry);
    }

    bibliography
        .entries()
        .map(|entry| {
            let parent = entry
                .field("crossref")
                .map(|field| bibliography.resolve(&field.value).to_ascii_lowercase())
                .and_then(|parent_key| by_key.get(&parent_key).copied());

            let own_or_parent = |pick: fn(&Entry, &Bibliography) -> Option<String>| {
                pick(entry, bibliography).or_else(|| parent.and_then(|parent| pick(parent, bibliography)))
            };

            EntrySummary {
                key: entry.key.clone(),
                entry_type: entry.entry_type.clone(),
                author: own_or_parent(author_of),
                year: own_or_parent(year_of),
                title: own_or_parent(title_of),
                doi: own_or_parent(doi_of),
                eprint: own_or_parent(eprint_of),
                isbn: own_or_parent(isbn_of),
                file: path.to_string(),
                span: entry.span,
            }
        })
        .collect()
}

/// A field's resolved text, or `None` if the entry has no such field or it resolves to nothing.
fn resolved_field(entry: &Entry, bibliography: &Bibliography, name: &str) -> Option<String> {
    let field = entry.field(name)?;
    let text = bibliography.resolve(&field.value);
    if text.trim().is_empty() {
        None
    } else {
        Some(text)
    }
}

fn author_of(entry: &Entry, bibliography: &Bibliography) -> Option<String> {
    resolved_field(entry, bibliography, "author").or_else(|| resolved_field(entry, bibliography, "editor"))
}

fn title_of(entry: &Entry, bibliography: &Bibliography) -> Option<String> {
    resolved_field(entry, bibliography, "title")
}

/// The entry's `doi` field, normalised the same way a pasted DOI is (S7.6's paste-to-cite dedup
/// key), so a hand-typed `10.1109/tcbb.2019.000001` and an export's `https://doi.org/10.1109/…`
/// compare equal.
fn doi_of(entry: &Entry, bibliography: &Bibliography) -> Option<String> {
    resolved_field(entry, bibliography, "doi").map(|doi| texbib::acquire::doi::normalize_doi(&doi))
}

/// The entry's `eprint` field, but only when `eprinttype` names arXiv — `eprint` alone is
/// BibLaTeX's generic "identifier in some other archive" field, and treating every archive's
/// eprint as an arXiv id would dedup two unrelated papers that merely share a number.
fn eprint_of(entry: &Entry, bibliography: &Bibliography) -> Option<String> {
    let eprinttype = resolved_field(entry, bibliography, "eprinttype")?;
    if !eprinttype.eq_ignore_ascii_case("arxiv") {
        return None;
    }
    resolved_field(entry, bibliography, "eprint").map(|id| texbib::acquire::arxiv::normalize_arxiv_id(&id))
}

fn isbn_of(entry: &Entry, bibliography: &Bibliography) -> Option<String> {
    resolved_field(entry, bibliography, "isbn").map(|isbn| texbib::acquire::isbn::normalize_isbn(&isbn))
}

/// `year = 2019`, or the year of a BibLaTeX `date = {2019-05-01}` (or a range, `2019/2020`):
/// the leading run of digits, when it is four of them.
fn year_of(entry: &Entry, bibliography: &Bibliography) -> Option<String> {
    if let Some(year) = resolved_field(entry, bibliography, "year") {
        return Some(year.trim().to_string());
    }
    let date = resolved_field(entry, bibliography, "date")?;
    let digits: String = date.trim().chars().take_while(char::is_ascii_digit).collect();
    if digits.len() == 4 {
        Some(digits)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Scanning `.tex` text. Pure — string in, data out — so every case below is a unit test with a
// literal, the same split `preamble-includes` makes between `scan` and `graph`.
// ---------------------------------------------------------------------------

/// Commands that name a `.bib` file. `\bibliography{a,b}` is BibTeX's and takes a comma list of
/// stems; the three `\add…bib` forms are BibLaTeX's and take one file name, extension included.
const RESOURCE_COMMANDS: &[&str] = &["bibliography", "addbibresource", "addglobalbib", "addsectionbib"];

/// Every `.bib` file the text names, as written in the source (stem or file name, comma lists
/// split), in order. `\bibliography` is the one command whose argument is a stem, so `.bib` is
/// appended to each of its parts here; the BibLaTeX commands are passed through as written.
pub fn scan_bib_resources(source: &str) -> Vec<String> {
    let cleaned = strip_line_comments(source);
    let mut resources = Vec::new();
    for (command, arguments, _line) in find_commands(&cleaned, |name| RESOURCE_COMMANDS.contains(&name)) {
        // Every resource command takes exactly one braced argument; a second `{...}` is prose.
        let Some(argument) = arguments.first() else { continue };
        for part in argument.split(',').map(str::trim).filter(|part| !part.is_empty()) {
            if command == "bibliography" {
                resources.push(format!("{part}.bib"));
            } else {
                resources.push(part.to_string());
            }
        }
    }
    resources
}

/// Every citation key in the text with the 1-based line of its command, in order.
///
/// A citation command is any `\…cite…` control sequence — `\cite`, `\citep`, `\parencite`,
/// `\Textcite`, `\footcite`, `\citeauthor`, `\nocite`, and whatever a package adds — with any
/// number of `[…]` options and one or more `{…}` arguments, each a comma list of keys
/// (`\cites{a}{b}` is BibLaTeX's multi-cite). Matching on the substring rather than a fixed list
/// is a choice: a key this scanner misses is an "undefined citation" that is not one, while an
/// extra command matched is harmless. `\nocite{*}` cites nothing in particular and is skipped.
pub fn scan_citations(source: &str) -> Vec<(String, u32)> {
    let cleaned = strip_line_comments(source);
    let mut citations = Vec::new();
    for (_command, arguments, line) in find_commands(&cleaned, |name| name.contains("cite")) {
        for argument in arguments {
            for key in argument.split(',').map(str::trim).filter(|key| !key.is_empty() && *key != "*") {
                citations.push((key.to_string(), line));
            }
        }
    }
    citations
}

/// Find every `\name` whose letters `wanted` accepts, and read its arguments: `[…]` options
/// are skipped, `{…}` groups are collected (with nested braces kept balanced), and anything
/// else ends the command. Returns `(name, braced arguments, 1-based line of the backslash)`.
///
/// Returns a `Vec` rather than an iterator so the two scanners above stay one `for` loop
/// each; source files are small enough that collecting first costs nothing measurable.
fn find_commands(cleaned: &str, wanted: impl Fn(&str) -> bool) -> Vec<(String, Vec<String>, u32)> {
    let bytes = cleaned.as_bytes();
    let mut found = Vec::new();
    let mut line: u32 = 1;
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'\n' => {
                line += 1;
                i += 1;
            }
            b'\\' => {
                let name_start = i + 1;
                let mut name_end = name_start;
                while name_end < bytes.len() && bytes[name_end].is_ascii_alphabetic() {
                    name_end += 1;
                }
                let name = &cleaned[name_start..name_end];
                if name.is_empty() || !wanted(name) {
                    // `\\`, `\%`, or a command we do not care about: step over the backslash
                    // *and* the character after it, so `\\cite` — an escaped backslash followed
                    // by the letters — is not mistaken for `\cite`.
                    i = name_start + 1;
                    continue;
                }
                let mut cursor = name_end;
                // A starred variant (`\cite*`) is the same command for our purposes.
                if bytes.get(cursor) == Some(&b'*') {
                    cursor += 1;
                }
                let mut arguments = Vec::new();
                loop {
                    // Spaces are allowed before the first argument (`\cite {a}` is legal TeX) but
                    // not between arguments: `\cites{a}{b}` is written adjacent, and in
                    // `\cite{a} {\bf b}` the second group is prose, not a second key.
                    let open_at = if arguments.is_empty() { skip_spaces(bytes, cursor) } else { cursor };
                    match bytes.get(open_at) {
                        Some(b'[') => {
                            cursor = matching_close(bytes, open_at, b'[', b']').unwrap_or(bytes.len());
                        }
                        Some(b'{') => {
                            // `None` is an unclosed group: take everything to the end of the
                            // text as the argument, which is the reading that loses no key.
                            let (inner_end, after) = match matching_close(bytes, open_at, b'{', b'}') {
                                Some(after) => (after - 1, after),
                                None => (bytes.len(), bytes.len()),
                            };
                            arguments.push(cleaned[open_at + 1..inner_end].to_string());
                            cursor = after;
                        }
                        _ => break,
                    }
                }
                found.push((name.to_string(), arguments, line));
                // An argument almost never spans a line break, but nothing stops an author
                // writing one that does, so keep the line counter honest rather than assume.
                line += cleaned[i..cursor].matches('\n').count() as u32;
                i = cursor;
            }
            _ => i += 1,
        }
    }
    found
}

/// Index of the first byte after the run of spaces and tabs starting at `from`. Line breaks are
/// deliberately not skipped: an argument on the next line still belongs to the command in TeX,
/// but an author who breaks a line after `\cite` is rare and a `{` starting the next line is
/// far more often a new group, so stopping at the line break is the reading that fails safe.
fn skip_spaces(bytes: &[u8], from: usize) -> usize {
    let mut i = from;
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    i
}

/// Index one past the bracket that closes the one at `open_at`, counting nesting, or `None`
/// when the group never closes — which is also what TeX would complain about.
fn matching_close(bytes: &[u8], open_at: usize, open: u8, close: u8) -> Option<usize> {
    let mut depth = 0usize;
    let mut i = open_at;
    while i < bytes.len() {
        let byte = bytes[i];
        if byte == b'\\' {
            i += 2; // an escaped bracket is text
            continue;
        }
        if byte == open {
            depth += 1;
        } else if byte == close {
            depth -= 1;
            if depth == 0 {
                return Some(i + 1);
            }
        }
        i += 1;
    }
    None
}

/// `%` starts a comment that runs to the end of the line, except `\%`, which is a literal
/// percent sign. Line breaks are kept so line numbers stay right. The same rule, and nearly the
/// same code, as `preamble_includes::scan`'s private helper: ten lines are cheaper to repeat
/// than a public API for them would be to explain.
fn strip_line_comments(source: &str) -> String {
    let mut cleaned = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    let mut in_comment = false;
    while let Some(c) = chars.next() {
        if c == '\n' {
            in_comment = false;
            cleaned.push(c);
        } else if in_comment {
            continue;
        } else if c == '\\' {
            cleaned.push(c);
            if let Some(next) = chars.next() {
                cleaned.push(next);
            }
        } else if c == '%' {
            in_comment = true;
        } else {
            cleaned.push(c);
        }
    }
    cleaned
}

/// A resource argument as a project-relative, forward-slash path, or `None` if it climbs out
/// of the project or is absolute. `.bib` is appended when the argument has no extension at all
/// (`\addbibresource{refs}` is wrong but common enough to mean `refs.bib`); an argument that
/// already has one is kept as written.
fn resolve_bib_argument(base_dir: &Path, argument: &str) -> Option<String> {
    let joined = base_dir.join(argument.trim());
    let mut normal: Vec<Component> = Vec::new();
    for component in joined.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if matches!(normal.last(), Some(Component::Normal(_))) {
                    normal.pop();
                } else {
                    return None;
                }
            }
            Component::Normal(part) => normal.push(Component::Normal(part)),
            Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    let mut path: PathBuf = normal.into_iter().collect();
    if path.extension().is_none() {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        path.set_file_name(format!("{name}.bib"));
    }
    Some(path.to_string_lossy().replace('\\', "/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn scaffold(files: &[(&str, &str)]) -> TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (rel, content) in files {
            let path = dir.path().join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, content).unwrap();
        }
        dir
    }

    const REFS: &str = "@article{smith2019,\n  author = {Smith, Jane},\n  title = {A Title},\n  year = 2019,\n}\n";
    const MORE: &str = "@book{doe2020,\n  editor = {Doe, John},\n  title = {A Book},\n  date = {2020-05-01},\n}\n";

    // ---- the card's done-when: two .bib files and one missing one, all reported by path ----

    #[test]
    fn two_bib_files_and_a_missing_one_are_all_reported_by_path() {
        let dir = scaffold(&[
            ("main.tex", "\\input{intro}\n\\bibliography{refs,missing}\n\\addbibresource{bib/more.bib}\n"),
            ("intro.tex", "As shown~\\cite{smith2019}.\n"),
            ("refs.bib", REFS),
            ("bib/more.bib", MORE),
        ]);
        let index = build_index(dir.path(), Path::new("main.tex"));

        let files: Vec<(&str, bool, usize)> =
            index.files.iter().map(|f| (f.path.as_str(), f.exists, f.entry_count)).collect();
        assert_eq!(files, vec![("refs.bib", true, 1), ("missing.bib", false, 0), ("bib/more.bib", true, 1)]);

        let keys: Vec<&str> = index.entries.iter().map(|e| e.key.as_str()).collect();
        assert_eq!(keys, vec!["smith2019", "doe2020"]);
        assert_eq!(index.citations, vec![Citation { key: "smith2019".into(), file: "intro.tex".into(), line: 1 }]);
    }

    #[test]
    fn a_summary_carries_author_year_title_file_and_span() {
        let dir = scaffold(&[("main.tex", "\\bibliography{refs}\n"), ("refs.bib", REFS)]);
        let index = build_index(dir.path(), Path::new("main.tex"));
        let entry = index.entry("smith2019").expect("indexed");
        assert_eq!(entry.entry_type, "article");
        assert_eq!(entry.author.as_deref(), Some("Smith, Jane"));
        assert_eq!(entry.year.as_deref(), Some("2019"));
        assert_eq!(entry.title.as_deref(), Some("A Title"));
        assert_eq!(entry.file, "refs.bib");
        assert_eq!(entry.span.text(REFS), REFS.trim_end());
    }

    #[test]
    fn editor_stands_in_for_author_and_a_biblatex_date_for_year() {
        let dir = scaffold(&[("main.tex", "\\addbibresource{more.bib}\n"), ("more.bib", MORE)]);
        let index = build_index(dir.path(), Path::new("main.tex"));
        let entry = index.entry("doe2020").unwrap();
        assert_eq!(entry.author.as_deref(), Some("Doe, John"));
        assert_eq!(entry.year.as_deref(), Some("2020"));
    }

    #[test]
    fn a_crossref_child_inherits_the_year_and_editor_it_lacks() {
        let bib = "@inproceedings{paper, title = {The Paper}, author = {A. Author}, crossref = {PROC}}\n\
                   @proceedings{proc, title = {The Proceedings}, editor = {E. Editor}, year = 2018}\n";
        let dir = scaffold(&[("main.tex", "\\bibliography{refs}\n"), ("refs.bib", bib)]);
        let index = build_index(dir.path(), Path::new("main.tex"));
        let child = index.entry("paper").unwrap();
        assert_eq!(child.author.as_deref(), Some("A. Author"), "own field wins");
        assert_eq!(child.title.as_deref(), Some("The Paper"));
        assert_eq!(child.year.as_deref(), Some("2018"), "inherited, key compared ignoring case");
    }

    #[test]
    fn a_broken_entry_is_a_problem_on_its_file_not_a_failure() {
        let bib = "@article{ok, title = {Fine}}\n@article{broken, title = \n@article{also, title = {Fine}}\n";
        let dir = scaffold(&[("main.tex", "\\bibliography{refs}\n"), ("refs.bib", bib)]);
        let index = build_index(dir.path(), Path::new("main.tex"));
        assert_eq!(index.files[0].entry_count, 2);
        assert_eq!(index.files[0].problems.len(), 1);
        assert!(index.entry("ok").is_some() && index.entry("also").is_some());
    }

    #[test]
    fn strings_resolve_inside_their_own_file() {
        let bib = "@string{jmlr = {Journal of Machine Learning Research}}\n\
                   @article{k, author = {X}, title = {T}, year = 2001, journal = jmlr, month = jan}\n";
        let dir = scaffold(&[("main.tex", "\\bibliography{refs}\n"), ("refs.bib", bib)]);
        let index = build_index(dir.path(), Path::new("main.tex"));
        assert_eq!(index.entry("k").unwrap().year.as_deref(), Some("2001"));
    }

    #[test]
    fn a_resource_outside_the_project_is_listed_but_never_read() {
        let dir = scaffold(&[("main.tex", "\\bibliography{../shared/refs}\n")]);
        let index = build_index(dir.path(), Path::new("main.tex"));
        assert_eq!(index.files.len(), 1);
        assert_eq!(index.files[0].path, "../shared/refs.bib");
        assert!(!index.files[0].exists);
    }

    #[test]
    fn resources_resolve_against_the_root_files_directory() {
        let dir = scaffold(&[("paper/main.tex", "\\bibliography{refs}\n"), ("paper/refs.bib", REFS)]);
        let index = build_index(dir.path(), Path::new("paper/main.tex"));
        assert_eq!(index.files[0].path, "paper/refs.bib");
        assert!(index.files[0].exists);
    }

    #[test]
    fn a_file_named_twice_is_listed_once() {
        let dir = scaffold(&[
            ("main.tex", "\\input{a}\n\\bibliography{refs}\n"),
            ("a.tex", "\\addbibresource{refs.bib}\n"),
            ("refs.bib", REFS),
        ]);
        let index = build_index(dir.path(), Path::new("main.tex"));
        assert_eq!(index.files.len(), 1);
        assert_eq!(index.entries.len(), 1);
    }

    #[test]
    fn a_missing_root_yields_an_empty_index() {
        let dir = scaffold(&[]);
        let index = build_index(dir.path(), Path::new("main.tex"));
        assert_eq!(index, BibliographyIndex::default());
    }

    // ---- scanners ----

    #[test]
    fn scan_bib_resources_reads_both_syntaxes() {
        let source = "\\bibliography{refs, more}\n\\addbibresource[datatype=bibtex]{lib.bib}\n\\addbibresource{noext}\n";
        assert_eq!(scan_bib_resources(source), vec!["refs.bib", "more.bib", "lib.bib", "noext"]);
        assert_eq!(resolve_bib_argument(Path::new(""), "noext"), Some("noext.bib".into()));
    }

    #[test]
    fn scan_bib_resources_ignores_comments_and_bibliographystyle() {
        let source = "% \\bibliography{old}\n\\bibliographystyle{plain}\n\\bibliography{refs} % trailing\n";
        assert_eq!(scan_bib_resources(source), vec!["refs.bib"]);
    }

    #[test]
    fn scan_citations_covers_the_cite_family_with_options_and_lines() {
        let source = "\\cite{a}\n\\citep[p.~3]{b, c}\n\\parencite[see][12]{d}\n\\Textcite*{e}\n\\cites{f}{g}\n\\nocite{*}\n\\footcite{h}";
        let expected: Vec<(String, u32)> = [("a", 1), ("b", 2), ("c", 2), ("d", 3), ("e", 4), ("f", 5), ("g", 5), ("h", 7)]
            .into_iter()
            .map(|(k, l)| (k.to_string(), l))
            .collect();
        assert_eq!(scan_citations(source), expected);
    }

    #[test]
    fn scan_citations_is_not_fooled_by_commented_or_escaped_text() {
        let source = "100\\% sure \\cite{a} % \\cite{b}\n\\\\cite{c}\n\\excite{d}\n";
        // `\\cite{c}`: the backslashes pair off as a line break, and `cite{c}` is text.
        // `\excite`: contains "cite" — accepted on purpose, see `scan_citations`.
        assert_eq!(scan_citations(source), vec![("a".to_string(), 1), ("d".to_string(), 3)]);
    }

    #[test]
    fn scan_citations_keeps_nested_braces_balanced_and_survives_an_unclosed_one() {
        assert_eq!(scan_citations("\\cite{a} {\\bf x} \\cite{b}"), vec![("a".to_string(), 1), ("b".to_string(), 1)]);
        assert_eq!(scan_citations("\\cite{a"), vec![("a".to_string(), 1)]);
        assert_eq!(scan_citations("\\cite"), Vec::<(String, u32)>::new());
    }

    #[test]
    fn affects_index_is_bib_or_tex_only() {
        assert!(affects_index(Path::new("/p/refs.bib")));
        assert!(affects_index(Path::new("/p/REFS.BIB")));
        assert!(affects_index(Path::new("/p/sections/intro.tex")));
        assert!(!affects_index(Path::new("/p/main.pdf")));
        assert!(!affects_index(Path::new("/p/style.sty")));
    }
}
