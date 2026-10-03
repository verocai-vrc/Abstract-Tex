//! The project-wide bibliography index (DESIGN.md §5.4): which `.bib` files the document uses,
//! which keys they define, and which `.tex` files cite what.
//!
//! Owns three things, all built fresh from disk by [`build_index`]:
//!
//! 1. The list of `.bib` files the document names — `\bibliography{a,b}` (BibTeX, `.bib`
//!    implied) and `\addbibresource{a.bib}` (BibLaTeX, extension written out) — found by
//!    scanning every `.tex` file in the include graph (`abstract-tex-includes`, S4.1). A file the
//!    document names but that is not on disk is still listed, with `exists: false`, so the UI
//!    can say so; a silently dropped file would look like a bibliography with no entries.
//! 2. One [`EntrySummary`] per entry in those files — type, author, year, title, which file,
//!    and the entry's byte span in it — which is what `\cite` completion (S7.3) shows and
//!    paste-to-cite (S7.6) deduplicates against.
//! 3. Every `\cite`-family key in the document's `.tex` files, with file and line, which is
//!    what "undefined citation" and "never cited" health checks (S8.3) read.
//!
//! It must never write a file, and it must never keep the `.bib` file's text hostage: the
//! `.bib` is the author's (DESIGN.md §2, rule 1), so this module reads it through `texbib`,
//! summarises, and forgets. It deliberately does not import Tauri either, so it can be tested
//! on a machine that cannot link the app crate (see the S4.1 note in `SPRINTS.md`).
//!
//! **S8.3: five health checks, two crates.** `texbib::health` covers the two checks that need
//! only one parsed `.bib` file — missing required field, wrong dash in a page range — and are
//! published with that crate. The other three need data only this module has: `undefined
//! citation` and `never cited` compare `entries` against `citations` (a `.tex`-side scan,
//! outside `texbib`'s remit), and `duplicate DOI` compares entries *across* every `.bib` file the
//! project has open, not just one. [`BibliographyIndex::health`] runs all five and merges them.
//!
//! **S8.6: a sixth, ahead of the five.** A `.bib` the index lists with `exists: false` was
//! computed but never shown anywhere, so a document naming a file that is not there — or a linked
//! Zotero export Better BibTeX never wrote — looked like a bibliography with nothing in it, and
//! every citation into it reported as undefined with no word about why. `missing_bib_files` says
//! why, once per file, before the findings it causes.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use abstract_tex_includes::build_graph;
use serde::Serialize;
use texbib::{Bibliography, Entry, Span};
use texlog::Fix;

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
    /// Whether any `.tex` file has a `\nocite{*}` — BibTeX/Biber's "treat every entry as cited"
    /// command. `scan_citations` already drops the `*` itself rather than turning it into a
    /// `Citation` naming a literal key `"*"`, so this is the one bit of that command's meaning
    /// the index still needs to carry, for the never-cited check (`health`, below) to skip
    /// entirely rather than report every single entry in the bibliography.
    pub has_nocite_star: bool,
    /// The root file's folder, project-relative: what `\bibliography` arguments resolve against.
    /// Only S8.8's fix needs it, to write a linked export's path the way the document would.
    /// `#[serde(skip)]` because the frontend has no use for it and should not grow a field for it.
    #[serde(skip)]
    pub base_dir: PathBuf,
}

impl BibliographyIndex {
    /// The first entry with exactly this key. BibTeX compares keys case-insensitively and
    /// Biber exactly; exact is the comparison that is never wrong about an existing entry.
    pub fn entry(&self, key: &str) -> Option<&EntrySummary> {
        self.entries.iter().find(|entry| entry.key == key)
    }

    /// All six health checks, in a fixed order: missing `.bib` files first (S8.6: the cause of
    /// any undefined citations that follow), then DESIGN.md §5.4's five — undefined citations,
    /// never cited, duplicate DOIs, the three that need this whole index — then this module
    /// reparses each existing file to run `texbib::health::check` on it (missing field,
    /// page-range dash). A sentence per finding, never a raw anything, matching DESIGN.md §2
    /// rule 3.
    pub fn health(&self, project_dir: &Path) -> Vec<Finding> {
        let mut findings = Vec::new();
        findings.extend(missing_bib_files(self));
        findings.extend(linked_but_not_named(self, project_dir));
        findings.extend(undefined_citations(self));
        findings.extend(never_cited(self));
        findings.extend(duplicate_dois(self));
        findings.extend(entry_level_findings(self, project_dir));
        findings
    }
}

/// One health-check result, ready for a panel: a sentence, a severity, and a place to click.
/// The frontend's counterpart to `texlog::rules::Diagnostic` — that crate explains a compile
/// failure, this explains a bibliography, and neither ever reaches the screen unexplained
/// (DESIGN.md §2 rule 3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    /// Which check found this: `"missing-bib-file"`, `"undefined-citation"`, `"never-cited"`,
    /// `"duplicate-doi"`, or one of `texbib::health`'s two rule ids.
    pub rule: &'static str,
    pub severity: HealthSeverity,
    /// One complete sentence.
    pub message: String,
    /// Where to jump to: a `.tex` line for an undefined citation or a missing named `.bib`, a
    /// `.bib` entry's span for an entry-level finding, or only a file path for a linked export
    /// that is not on disk.
    pub jump: Jump,
    /// A one-click edit to the `.tex` line `jump` names, in exactly the shape a compile
    /// diagnostic's fix has (S6.2), so the frontend applies both the same way. Only S8.8's
    /// linked-but-not-named finding offers one: every other finding either has no edit that
    /// cannot be wrong, or points into a `.bib`, which no fix touches.
    pub fix: Option<Fix>,
}

/// Mirrors `texbib::health::Severity` so both crates' findings render with the same two words;
/// kept as its own type (not a re-export) because this one also has to describe `undefined
/// citation` and friends, which `texbib` has never heard of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum HealthSeverity {
    Error,
    Warning,
}

impl From<texbib::health::Severity> for HealthSeverity {
    fn from(severity: texbib::health::Severity) -> Self {
        match severity {
            texbib::health::Severity::Error => HealthSeverity::Error,
            texbib::health::Severity::Warning => HealthSeverity::Warning,
        }
    }
}

/// Where a [`Finding`] points: a line in a `.tex` file, a byte span in a `.bib` file, or a file
/// with nothing in it to land on. Variants rather than optional fields on `Finding` itself, so a
/// caller cannot forget to check which one is set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Jump {
    /// A `\cite` or `\bibliography` command: project-relative `.tex` path and 1-based line.
    TexLine { file: String, line: u32 },
    /// A `.bib` entry: project-relative path and its byte span in that file.
    BibEntry { file: String, span: Span },
    /// A file that is not on disk (S8.6): a linked export Better BibTeX has not written. There is
    /// nothing to open, but the path still says which file the finding is about, and the panel
    /// groups by it the way it groups every other finding.
    MissingFile { file: String },
}

/// A `.bib` the index lists but cannot read — one finding per file, not per citation into it,
/// since every one of those citations already shows up as undefined and the author needs to
/// know the one cause, not count its symptoms. What to say depends on where the file came from:
/// a file the document names and that is simply absent is an error (BibTeX will stop on it); one
/// outside the project folder may compile fine, but this app never reads outside the project, so
/// it is a warning that explains the undefined citations that follow; a linked export not yet on
/// disk is a warning pointing at Better BibTeX, which is what writes it.
fn missing_bib_files(index: &BibliographyIndex) -> Vec<Finding> {
    index
        .files
        .iter()
        .filter(|file| !file.exists)
        .map(|file| {
            let path = &file.path;
            match &file.origin {
                BibOrigin::Named { file: tex_file, line } => Finding {
                    rule: "missing-bib-file",
                    severity: HealthSeverity::Error,
                    message: format!("'{path}' is named here but is not in the project folder."),
                    jump: Jump::TexLine { file: tex_file.clone(), line: *line },
                    fix: None,
                },
                BibOrigin::Outside { file: tex_file, line } => Finding {
                    rule: "missing-bib-file",
                    severity: HealthSeverity::Warning,
                    // A `\` at the end of a string-literal line continues the string and skips the
                    // next line's leading spaces, so a long sentence can wrap in the source only.
                    message: format!(
                        "'{path}' is outside the project folder, so its entries are not read \
                         and citations to them show as undefined."
                    ),
                    jump: Jump::TexLine { file: tex_file.clone(), line: *line },
                    fix: None,
                },
                BibOrigin::Linked => Finding {
                    rule: "missing-bib-file",
                    severity: HealthSeverity::Warning,
                    message: format!(
                        "The linked collection export '{path}' is not on disk yet. Better BibTeX \
                         writes it while Zotero is running with the collection's automatic export on."
                    ),
                    jump: Jump::MissingFile { file: path.clone() },
                    fix: None,
                },
            }
        })
        .collect()
}

/// A linked export (S8.2) that the document does not name (S8.8). The index reads it, so
/// completion offers its keys and the undefined-citation check counts them as defined — but
/// BibTeX and Biber read only the files the document names, so in the PDF every citation that
/// only this file defines prints as `[?]`. That disagreement between the panel and the engine is
/// what this finding exists to remove: an error when such citations exist, a warning otherwise
/// (the file's entries are still missing from the reference list).
///
/// The fix adds the file to the document's own last resource command, the one place that makes
/// the engine and the index agree without the build doing anything the `.tex` does not say
/// (DESIGN.md §2 rule 1). A document with no resource command gets no fix: where a new one
/// belongs is the author's call.
fn linked_but_not_named(index: &BibliographyIndex, project_dir: &Path) -> Vec<Finding> {
    // Where the document names its bibliography, if it does: the last command, so a new
    // resource lands after every existing one and changes no existing entry's precedence.
    let last_named = index.files.iter().rev().find_map(|file| match &file.origin {
        BibOrigin::Named { file, line } => Some((file.clone(), *line)),
        _ => None,
    });
    let keys_the_engine_sees: std::collections::HashSet<&str> = index
        .entries
        .iter()
        .filter(|entry| index.files.iter().any(|f| f.path == entry.file && !matches!(f.origin, BibOrigin::Linked)))
        .map(|entry| entry.key.as_str())
        .collect();

    let mut findings = Vec::new();
    // A linked export that is not on disk is S8.6's finding already; one cause, one finding.
    for linked in index.files.iter().filter(|f| f.exists && matches!(f.origin, BibOrigin::Linked)) {
        let only_here: std::collections::HashSet<&str> = index
            .entries
            .iter()
            .filter(|entry| entry.file == linked.path && !keys_the_engine_sees.contains(entry.key.as_str()))
            .map(|entry| entry.key.as_str())
            .collect();
        let cited_only_here =
            only_here.iter().filter(|key| index.citations.iter().any(|c| c.key == **key)).count();

        let path = &linked.path;
        let (severity, message) = if cited_only_here > 0 {
            let entries = if cited_only_here == 1 { "entry" } else { "entries" };
            (
                HealthSeverity::Error,
                format!(
                    "'{path}' is linked but the document does not name it, so BibTeX never reads it: \
                     {cited_only_here} cited {entries} from it will print as [?]."
                ),
            )
        } else {
            (
                HealthSeverity::Warning,
                format!(
                    "'{path}' is linked but the document does not name it, so its entries will not \
                     appear in the PDF."
                ),
            )
        };

        let argument = relative_to(&index.base_dir, path);
        let (jump, fix) = match &last_named {
            Some((tex_file, line)) => {
                let line_text = std::fs::read_to_string(project_dir.join(tex_file))
                    .ok()
                    .and_then(|text| text.lines().nth(*line as usize - 1).map(str::to_string));
                let fix = line_text.and_then(|text| fix_adding_bib_resource(&text, &argument));
                (Jump::TexLine { file: tex_file.clone(), line: *line }, fix)
            }
            // Nothing in the document to point at, so open the linked file itself.
            None => (Jump::BibEntry { file: path.clone(), span: Span { start: 0, end: 0 } }, None),
        };
        findings.push(Finding { rule: "linked-not-named", severity, message, jump, fix });
    }
    findings
}

/// The edit that adds `argument` (a `.bib` path as the document would write it) to the resource
/// command on `line_text`: one more stem in a `\bibliography{…}` list, or a new
/// `\addbibresource{…}` line after an existing one. `None` when the line has neither, which is
/// what makes a stale line number (the file changed since the index was built) a no-op rather
/// than an edit in the wrong place — `locateFix` on the frontend checks the same text again.
fn fix_adding_bib_resource(line_text: &str, argument: &str) -> Option<Fix> {
    if let Some(command) = command_with_argument(line_text, "\\bibliography") {
        let stem = argument.strip_suffix(".bib").unwrap_or(argument);
        let replace = format!("{},{stem}}}", command.strip_suffix('}')?);
        return Some(Fix {
            description: format!("Add {stem} to \\bibliography"),
            find: command,
            replace,
        });
    }
    let command = command_with_argument(line_text, "\\addbibresource")?;
    Some(Fix {
        description: format!("Add \\addbibresource{{{argument}}}"),
        replace: format!("{command}\n\\addbibresource{{{argument}}}"),
        find: command,
    })
}

/// `name` and everything up to its first closing brace — `\bibliography{a,b}`, or
/// `\addbibresource[datatype=bibtex]{refs.bib}` with its options — as written on the line.
/// Resource arguments are file names, which never contain a nested brace, so the first `}` ends
/// it. `\bibliographystyle` is not `\bibliography`: the character after the name must open the
/// argument (or its options), not continue a longer command name.
fn command_with_argument(line_text: &str, name: &str) -> Option<String> {
    let mut search_from = 0;
    while let Some(found) = line_text[search_from..].find(name) {
        let start = search_from + found;
        let rest = &line_text[start + name.len()..];
        if rest.starts_with('{') || rest.starts_with('[') {
            let end = rest.find('}')?;
            return Some(line_text[start..start + name.len() + end + 1].to_string());
        }
        search_from = start + name.len();
    }
    None
}

/// `path` (project-relative) as seen from `base` (project-relative folder): how a document whose
/// root file sits in `base` must write it. `..` for each level of `base` the path does not share.
fn relative_to(base: &Path, path: &str) -> String {
    let base_parts: Vec<String> =
        base.components().map(|part| part.as_os_str().to_string_lossy().into_owned()).collect();
    let path_parts: Vec<&str> = path.split('/').collect();
    let shared = base_parts.iter().zip(&path_parts).take_while(|(a, b)| a.as_str() == **b).count();
    let mut parts: Vec<&str> = vec![".."; base_parts.len() - shared];
    parts.extend(&path_parts[shared..]);
    parts.join("/")
}

/// A key cited somewhere but defined nowhere — one finding per *key*, not per citation, so
/// citing the same missing key five times does not flood the panel; the message still says how
/// many places, since that is the number that tells the author how much work fixing it is.
fn undefined_citations(index: &BibliographyIndex) -> Vec<Finding> {
    let mut by_key: HashMap<&str, Vec<&Citation>> = HashMap::new();
    for citation in &index.citations {
        if index.entry(&citation.key).is_none() {
            by_key.entry(citation.key.as_str()).or_default().push(citation);
        }
    }
    let mut keys: Vec<&str> = by_key.keys().copied().collect();
    keys.sort_unstable();
    keys.into_iter()
        .map(|key| {
            let places = &by_key[key];
            let first = places[0];
            let message = if places.len() == 1 {
                format!("'{key}' is cited but no entry defines it.")
            } else {
                format!("'{key}' is cited in {} places but no entry defines it.", places.len())
            };
            Finding {
                rule: "undefined-citation",
                severity: HealthSeverity::Error,
                message,
                jump: Jump::TexLine { file: first.file.clone(), line: first.line },
                fix: None,
            }
        })
        .collect()
}

/// An entry nobody cites. `\nocite{*}` (cite every entry) is already dropped by `scan_citations`
/// before it ever becomes a [`Citation`], which would make every bibliography using it report
/// every entry as never cited — so this check treats that command as an explicit "everything in
/// this file counts as cited" and skips the whole `.tex` corpus's worth of checking when any
/// `\nocite{*}` is present, rather than re-scanning raw text this module has already discarded.
fn never_cited(index: &BibliographyIndex) -> Vec<Finding> {
    if index.has_nocite_star {
        return Vec::new();
    }
    let cited: std::collections::HashSet<&str> = index.citations.iter().map(|c| c.key.as_str()).collect();
    index
        .entries
        .iter()
        .filter(|entry| !cited.contains(entry.key.as_str()))
        .map(|entry| Finding {
            rule: "never-cited",
            severity: HealthSeverity::Warning,
            message: format!("'{}' is defined but never cited.", entry.key),
            jump: Jump::BibEntry { file: entry.file.clone(), span: entry.span },
            fix: None,
        })
        .collect()
}

/// Two (or more) entries whose `doi` field normalises to the same value — the same comparison
/// S7.6's paste-to-cite already makes before appending a new entry, run here across everything
/// already in the bibliography rather than against one candidate paste.
fn duplicate_dois(index: &BibliographyIndex) -> Vec<Finding> {
    let mut by_doi: HashMap<&str, Vec<&EntrySummary>> = HashMap::new();
    for entry in &index.entries {
        if let Some(doi) = entry.doi.as_deref() {
            by_doi.entry(doi).or_default().push(entry);
        }
    }
    let mut groups: Vec<&Vec<&EntrySummary>> = by_doi.values().filter(|group| group.len() > 1).collect();
    groups.sort_unstable_by_key(|group| group[0].key.clone());
    groups
        .into_iter()
        .flat_map(|group| {
            let others: Vec<&str> = group.iter().map(|e| e.key.as_str()).collect();
            group.iter().map(move |entry| {
                let rest: Vec<&str> = others.iter().copied().filter(|key| *key != entry.key).collect();
                Finding {
                    rule: "duplicate-doi",
                    severity: HealthSeverity::Warning,
                    message: format!("'{}' has the same DOI as {}.", entry.key, rest.join(", ")),
                    jump: Jump::BibEntry { file: entry.file.clone(), span: entry.span },
                    fix: None,
                }
            })
        })
        .collect()
}

/// Reparses each existing `.bib` file to run `texbib::health::check` on it. Reparsing rather
/// than keeping a `Bibliography` around is the same choice `build_index` itself already makes —
/// `EntrySummary` is what this module keeps, not the parse tree, so a second pass over already
/// summarised entries could not answer "which field is missing" without the fields themselves.
/// The file is small and this runs once per index rebuild, not per keystroke.
fn entry_level_findings(index: &BibliographyIndex, project_dir: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();
    for file in index.files.iter().filter(|file| file.exists) {
        let Ok(text) = std::fs::read_to_string(project_dir.join(&file.path)) else { continue };
        let bibliography = texbib::parse(&text);
        for finding in texbib::health::check(&bibliography) {
            findings.push(Finding {
                rule: finding.rule,
                severity: finding.severity.into(),
                message: finding.message,
                jump: Jump::BibEntry { file: file.path.clone(), span: finding.at },
                fix: None,
            });
        }
    }
    findings
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
    /// Why the index lists this file — which decides what a missing one's finding says (S8.6).
    pub origin: BibOrigin,
}

/// Where a [`BibFile`] came from. A file named by the document *and* linked is `Named`: the
/// document's own command is the more useful place to point at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum BibOrigin {
    /// A `\bibliography`/`\addbibresource` command inside the project folder names it, first at
    /// this project-relative `.tex` file and 1-based line.
    Named { file: String, line: u32 },
    /// The same, but the argument resolves outside the project folder, which this app never
    /// reads (the rule `Project::resolve` applies to every path).
    Outside { file: String, line: u32 },
    /// `abstract-tex.toml`'s `extra_bib_files` lists it — a linked Zotero collection's export (S8.2).
    Linked,
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

/// Build the index for the document rooted at `root_file` (project-relative) in `project_dir`,
/// plus whatever `extra_bib_files` names (S8.2: `abstract-tex.toml`'s `extra_bib_files`, most often a
/// linked Zotero collection's auto-export path) — a `.bib` the document's own `\bibliography`/
/// `\addbibresource` commands never mention, but the author still wants indexed.
///
/// Walks the include graph, scans each existing `.tex` for bibliography resources and
/// citations, then parses each `.bib` through `texbib`. Never fails: a file that cannot be
/// read is simply one with no citations, and a malformed `.bib` item is a [`Problem`] on its
/// file — the same "never fail, report what you saw" rule the parser itself follows.
pub fn build_index(project_dir: &Path, root_file: &Path, extra_bib_files: &[String]) -> BibliographyIndex {
    let graph = build_graph(project_dir, root_file);
    // `\bibliography{refs}` resolves against the root file's directory, exactly as `\input`
    // does and for the same reason: paths are relative to where the engine runs.
    let base_dir = root_file.parent().map(Path::to_path_buf).unwrap_or_default();

    let mut files: Vec<BibFile> = Vec::new();
    let mut citations: Vec<Citation> = Vec::new();
    let mut has_nocite_star = false;

    for node in graph.nodes.iter().filter(|node| node.exists) {
        let Ok(text) = std::fs::read_to_string(project_dir.join(&node.path)) else { continue };

        for (resource, line) in scan_bib_resources(&text) {
            let named_at = node.path.clone();
            let (path, origin) = match resolve_bib_argument(&base_dir, &resource) {
                Some(resolved) => (resolved, BibOrigin::Named { file: named_at, line }),
                None => (resource.clone(), BibOrigin::Outside { file: named_at, line }),
            };
            if files.iter().any(|file| file.path == path) {
                continue;
            }
            let exists = matches!(origin, BibOrigin::Named { .. }) && project_dir.join(&path).is_file();
            files.push(BibFile { path, exists, entry_count: 0, problems: Vec::new(), origin });
        }

        for (key, line) in scan_citations(&text) {
            citations.push(Citation { key, file: node.path.clone(), line });
        }
        has_nocite_star |= scan_has_nocite_star(&text);
    }

    // Already project-relative (that is what `abstract-tex.toml` stores), so no `resolve_bib_argument`
    // step: a linked collection's export path is not written relative to the root file's folder.
    for extra in extra_bib_files {
        if files.iter().any(|file| &file.path == extra) {
            continue;
        }
        let exists = project_dir.join(extra).is_file();
        files.push(BibFile { path: extra.clone(), exists, entry_count: 0, problems: Vec::new(), origin: BibOrigin::Linked });
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

    BibliographyIndex { files, entries, citations, has_nocite_star, base_dir }
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
// literal, the same split `abstract-tex-includes` makes between `scan` and `graph`.
// ---------------------------------------------------------------------------

/// Commands that name a `.bib` file. `\bibliography{a,b}` is BibTeX's and takes a comma list of
/// stems; the three `\add…bib` forms are BibLaTeX's and take one file name, extension included.
const RESOURCE_COMMANDS: &[&str] = &["bibliography", "addbibresource", "addglobalbib", "addsectionbib"];

/// Every `.bib` file the text names, as written in the source (stem or file name, comma lists
/// split), in order, each with the 1-based line of the command that names it — the place a
/// missing file's finding points at (S8.6). `\bibliography` is the one command whose argument is
/// a stem, so `.bib` is appended to each of its parts here; the BibLaTeX commands are passed
/// through as written.
pub fn scan_bib_resources(source: &str) -> Vec<(String, u32)> {
    let cleaned = strip_line_comments(source);
    let mut resources = Vec::new();
    for (command, arguments, line) in find_commands(&cleaned, |name| RESOURCE_COMMANDS.contains(&name)) {
        // Every resource command takes exactly one braced argument; a second `{...}` is prose.
        let Some(argument) = arguments.first() else { continue };
        for part in argument.split(',').map(str::trim).filter(|part| !part.is_empty()) {
            if command == "bibliography" {
                resources.push((format!("{part}.bib"), line));
            } else {
                resources.push((part.to_string(), line));
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
    for (_command, arguments, line) in find_commands(&cleaned, texbib::is_citation_command) {
        for argument in arguments {
            for key in argument.split(',').map(str::trim).filter(|key| !key.is_empty() && *key != "*") {
                citations.push((key.to_string(), line));
            }
        }
    }
    citations
}

/// Whether the text has a `\nocite{*}` (any spacing, any argument list containing a bare `*`) —
/// BibTeX/Biber's "cite every entry in the bibliography" command. `scan_citations` above already
/// walks the same commands and drops the `*` as a non-key, which is correct for the citation list
/// but throws away the one piece of information [`never_cited`] needs, so this is a second, much
/// smaller pass over the same `find_commands` output rather than a change to what `scan_citations`
/// returns.
fn scan_has_nocite_star(source: &str) -> bool {
    let cleaned = strip_line_comments(source);
    find_commands(&cleaned, texbib::is_citation_command)
        .into_iter()
        .any(|(_command, arguments, _line)| arguments.iter().any(|argument| argument.split(',').any(|key| key.trim() == "*")))
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
/// same code, as `abstract_tex_includes::scan`'s private helper: ten lines are cheaper to repeat
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
        let index = build_index(dir.path(), Path::new("main.tex"), &[]);

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
        let index = build_index(dir.path(), Path::new("main.tex"), &[]);
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
        let index = build_index(dir.path(), Path::new("main.tex"), &[]);
        let entry = index.entry("doe2020").unwrap();
        assert_eq!(entry.author.as_deref(), Some("Doe, John"));
        assert_eq!(entry.year.as_deref(), Some("2020"));
    }

    #[test]
    fn a_crossref_child_inherits_the_year_and_editor_it_lacks() {
        let bib = "@inproceedings{paper, title = {The Paper}, author = {A. Author}, crossref = {PROC}}\n\
                   @proceedings{proc, title = {The Proceedings}, editor = {E. Editor}, year = 2018}\n";
        let dir = scaffold(&[("main.tex", "\\bibliography{refs}\n"), ("refs.bib", bib)]);
        let index = build_index(dir.path(), Path::new("main.tex"), &[]);
        let child = index.entry("paper").unwrap();
        assert_eq!(child.author.as_deref(), Some("A. Author"), "own field wins");
        assert_eq!(child.title.as_deref(), Some("The Paper"));
        assert_eq!(child.year.as_deref(), Some("2018"), "inherited, key compared ignoring case");
    }

    #[test]
    fn a_broken_entry_is_a_problem_on_its_file_not_a_failure() {
        let bib = "@article{ok, title = {Fine}}\n@article{broken, title = \n@article{also, title = {Fine}}\n";
        let dir = scaffold(&[("main.tex", "\\bibliography{refs}\n"), ("refs.bib", bib)]);
        let index = build_index(dir.path(), Path::new("main.tex"), &[]);
        assert_eq!(index.files[0].entry_count, 2);
        assert_eq!(index.files[0].problems.len(), 1);
        assert!(index.entry("ok").is_some() && index.entry("also").is_some());
    }

    #[test]
    fn strings_resolve_inside_their_own_file() {
        let bib = "@string{jmlr = {Journal of Machine Learning Research}}\n\
                   @article{k, author = {X}, title = {T}, year = 2001, journal = jmlr, month = jan}\n";
        let dir = scaffold(&[("main.tex", "\\bibliography{refs}\n"), ("refs.bib", bib)]);
        let index = build_index(dir.path(), Path::new("main.tex"), &[]);
        assert_eq!(index.entry("k").unwrap().year.as_deref(), Some("2001"));
    }

    #[test]
    fn a_resource_outside_the_project_is_listed_but_never_read() {
        let dir = scaffold(&[("main.tex", "\\bibliography{../shared/refs}\n")]);
        let index = build_index(dir.path(), Path::new("main.tex"), &[]);
        assert_eq!(index.files.len(), 1);
        assert_eq!(index.files[0].path, "../shared/refs.bib");
        assert!(!index.files[0].exists);
    }

    #[test]
    fn resources_resolve_against_the_root_files_directory() {
        let dir = scaffold(&[("paper/main.tex", "\\bibliography{refs}\n"), ("paper/refs.bib", REFS)]);
        let index = build_index(dir.path(), Path::new("paper/main.tex"), &[]);
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
        let index = build_index(dir.path(), Path::new("main.tex"), &[]);
        assert_eq!(index.files.len(), 1);
        assert_eq!(index.entries.len(), 1);
    }

    #[test]
    fn a_missing_root_yields_an_empty_index() {
        let dir = scaffold(&[]);
        let index = build_index(dir.path(), Path::new("main.tex"), &[]);
        assert_eq!(index, BibliographyIndex::default());
    }

    // ---- extra_bib_files (S8.2: a linked Zotero collection's export path) ----

    #[test]
    fn an_extra_bib_file_is_indexed_though_no_tex_file_names_it() {
        let dir = scaffold(&[("main.tex", "As shown~\\cite{smith2019}.\n"), ("zotero/reading.bib", REFS)]);
        let index = build_index(dir.path(), Path::new("main.tex"), &["zotero/reading.bib".to_string()]);
        assert_eq!(index.files, vec![BibFile {
            path: "zotero/reading.bib".to_string(),
            exists: true,
            entry_count: 1,
            problems: Vec::new(),
            origin: BibOrigin::Linked,
        }]);
        assert!(index.entry("smith2019").is_some());
    }

    #[test]
    fn an_extra_bib_file_the_document_also_names_is_listed_once() {
        let dir = scaffold(&[("main.tex", "\\bibliography{refs}\n"), ("refs.bib", REFS)]);
        let index = build_index(dir.path(), Path::new("main.tex"), &["refs.bib".to_string()]);
        assert_eq!(index.files.len(), 1);
        assert_eq!(index.entries.len(), 1);
    }

    #[test]
    fn a_missing_extra_bib_file_is_listed_with_exists_false() {
        let dir = scaffold(&[("main.tex", "")]);
        let index = build_index(dir.path(), Path::new("main.tex"), &["zotero/gone.bib".to_string()]);
        assert_eq!(index.files.len(), 1);
        assert!(!index.files[0].exists);
    }

    // ---- health checks (S8.3) ----

    /// The card's done-when: one instance of each of the five problems, five findings.
    ///
    /// - `undefined` is cited but has no entry.
    /// - `nocited` is a real entry that nothing cites.
    /// - `dupe_a`/`dupe_b` share a DOI (one written as a bare DOI, the other as a full URL, to
    ///   also prove normalisation runs before the comparison).
    /// - `incomplete` is an `@article` with no `journal`.
    /// - `dashed` has `pages = {12-15}`, a hyphen.
    ///
    /// `clean` has none of the five problems and exists only to prove it produces no finding of
    /// its own — the second half of the card's done-when.
    fn health_fixture() -> (&'static str, &'static str) {
        let tex = "As shown~\\cite{undefined, dupe_a, dupe_b, incomplete, dashed, clean}.\n";
        let bib = "@article{clean, author = {A}, title = {T0}, journal = {J}, year = 2019, pages = {1--2}}\n\
                   @article{nocited, author = {B}, title = {T1}, journal = {J}, year = 2019}\n\
                   @article{dupe_a, author = {C}, title = {T2}, journal = {J}, year = 2019, doi = {10.1/x}}\n\
                   @article{dupe_b, author = {D}, title = {T3}, journal = {J}, year = 2019, doi = {https://doi.org/10.1/x}}\n\
                   @article{incomplete, author = {E}, title = {T4}, year = 2019}\n\
                   @article{dashed, author = {F}, title = {T5}, journal = {J}, year = 2019, pages = {12-15}}\n";
        (tex, bib)
    }

    #[test]
    fn one_of_each_of_the_five_problems_yields_exactly_five_findings() {
        let (tex, bib) = health_fixture();
        let dir = scaffold(&[("main.tex", &format!("\\bibliography{{refs}}\n{tex}")), ("refs.bib", bib)]);
        let index = build_index(dir.path(), Path::new("main.tex"), &[]);
        let findings = index.health(dir.path());

        let mut rules: Vec<&str> = findings.iter().map(|f| f.rule).collect();
        rules.sort_unstable();
        assert_eq!(
            rules,
            vec!["duplicate-doi", "duplicate-doi", "missing-field", "never-cited", "page-range-dash", "undefined-citation"],
            "{findings:#?}"
        );
    }

    #[test]
    fn a_clean_bibliography_has_no_findings() {
        let dir = scaffold(&[
            ("main.tex", "As shown~\\cite{clean}.\n\\bibliography{refs}\n"),
            ("refs.bib", "@article{clean, author = {A}, title = {T}, journal = {J}, year = 2019, pages = {1--2}}\n"),
        ]);
        let index = build_index(dir.path(), Path::new("main.tex"), &[]);
        assert_eq!(index.health(dir.path()), Vec::new());
    }

    #[test]
    fn an_undefined_citation_points_at_its_tex_line() {
        let dir = scaffold(&[("main.tex", "line one\n\\cite{missing}\n"), ("refs.bib", "")]);
        let index = build_index(dir.path(), Path::new("main.tex"), &[]);
        let findings = index.health(dir.path());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule, "undefined-citation");
        assert_eq!(findings[0].severity, HealthSeverity::Error);
        assert_eq!(findings[0].jump, Jump::TexLine { file: "main.tex".to_string(), line: 2 });
    }

    #[test]
    fn nocite_star_suppresses_the_never_cited_check() {
        let dir = scaffold(&[
            ("main.tex", "\\nocite{*}\n\\bibliography{refs}\n"),
            ("refs.bib", "@misc{k, title = {T}}\n"),
        ]);
        let index = build_index(dir.path(), Path::new("main.tex"), &[]);
        assert!(index.has_nocite_star);
        assert!(index.health(dir.path()).is_empty(), "{:#?}", index.health(dir.path()));
    }

    // ---- missing .bib files (S8.6) ----

    #[test]
    fn a_named_bib_that_is_not_on_disk_is_an_error_at_its_command_and_comes_first() {
        let dir = scaffold(&[("main.tex", "Text~\\cite{a}.\n\n\\bibliography{refs}\n")]);
        let index = build_index(dir.path(), Path::new("main.tex"), &[]);
        let findings = index.health(dir.path());

        // The cause before its symptom: the missing file, then the citation it leaves undefined.
        let rules: Vec<&str> = findings.iter().map(|f| f.rule).collect();
        assert_eq!(rules, vec!["missing-bib-file", "undefined-citation"]);
        assert_eq!(findings[0].severity, HealthSeverity::Error);
        assert_eq!(findings[0].message, "'refs.bib' is named here but is not in the project folder.");
        assert_eq!(findings[0].jump, Jump::TexLine { file: "main.tex".to_string(), line: 3 });
    }

    #[test]
    fn a_bib_outside_the_project_is_a_warning_that_explains_the_undefined_citations() {
        let dir = scaffold(&[("main.tex", "\\addbibresource{../shared/refs.bib}\n")]);
        let index = build_index(dir.path(), Path::new("main.tex"), &[]);
        let findings = index.health(dir.path());
        assert_eq!(findings.len(), 1, "{findings:#?}");
        assert_eq!(findings[0].severity, HealthSeverity::Warning);
        assert!(findings[0].message.starts_with("'../shared/refs.bib' is outside the project folder"));
        assert_eq!(findings[0].jump, Jump::TexLine { file: "main.tex".to_string(), line: 1 });
    }

    #[test]
    fn a_linked_export_not_yet_written_points_at_better_bibtex_and_names_the_file() {
        let dir = scaffold(&[("main.tex", "")]);
        let index = build_index(dir.path(), Path::new("main.tex"), &["zotero/Thesis.bib".to_string()]);
        let findings = index.health(dir.path());
        assert_eq!(findings.len(), 1, "{findings:#?}");
        assert_eq!(findings[0].severity, HealthSeverity::Warning);
        assert!(findings[0].message.contains("Better BibTeX"), "{}", findings[0].message);
        assert_eq!(findings[0].jump, Jump::MissingFile { file: "zotero/Thesis.bib".to_string() });
    }

    #[test]
    fn a_file_both_named_and_linked_is_reported_at_the_documents_command() {
        let dir = scaffold(&[("main.tex", "\\bibliography{refs}\n")]);
        let index = build_index(dir.path(), Path::new("main.tex"), &["refs.bib".to_string()]);
        assert_eq!(index.files[0].origin, BibOrigin::Named { file: "main.tex".to_string(), line: 1 });
    }

    // ---- linked but not named (S8.8) ----

    const LINKED: &str = "@article{zot2020, author = {Z}, title = {T}, journal = {J}, year = 2020}\n";

    fn linked_findings(tex: &str) -> (TempDir, Vec<Finding>) {
        let dir = scaffold(&[("main.tex", tex), ("references.bib", ""), ("zotero/Thesis.bib", LINKED)]);
        let index = build_index(dir.path(), Path::new("main.tex"), &["zotero/Thesis.bib".to_string()]);
        let findings: Vec<Finding> =
            index.health(dir.path()).into_iter().filter(|f| f.rule == "linked-not-named").collect();
        (dir, findings)
    }

    #[test]
    fn a_cited_linked_export_the_document_does_not_name_is_an_error_with_a_bibliography_fix() {
        let (_dir, findings) = linked_findings("Text~\\cite{zot2020}.\n\n\\bibliography{references}\n");
        assert_eq!(findings.len(), 1, "{findings:#?}");
        assert_eq!(findings[0].severity, HealthSeverity::Error);
        assert!(findings[0].message.contains("1 cited entry from it will print as [?]"), "{}", findings[0].message);
        assert_eq!(findings[0].jump, Jump::TexLine { file: "main.tex".to_string(), line: 3 });
        assert_eq!(
            findings[0].fix,
            Some(Fix {
                description: "Add zotero/Thesis to \\bibliography".to_string(),
                find: "\\bibliography{references}".to_string(),
                replace: "\\bibliography{references,zotero/Thesis}".to_string(),
            })
        );
    }

    #[test]
    fn with_addbibresource_the_fix_adds_a_line_after_the_existing_one() {
        let (_dir, findings) = linked_findings("\\addbibresource[datatype=bibtex]{references.bib}\n");
        // Nothing cites it yet: the entries are still missing from the PDF, so a warning.
        assert_eq!(findings[0].severity, HealthSeverity::Warning);
        let fix = findings[0].fix.as_ref().expect("an addbibresource line has a fix");
        assert_eq!(fix.find, "\\addbibresource[datatype=bibtex]{references.bib}");
        assert_eq!(fix.replace, "\\addbibresource[datatype=bibtex]{references.bib}\n\\addbibresource{zotero/Thesis.bib}");
    }

    #[test]
    fn without_any_resource_command_there_is_no_fix_and_the_jump_opens_the_export() {
        let (_dir, findings) = linked_findings("No bibliography yet.\n");
        assert_eq!(findings[0].fix, None);
        assert_eq!(findings[0].jump, Jump::BibEntry { file: "zotero/Thesis.bib".to_string(), span: Span { start: 0, end: 0 } });
    }

    #[test]
    fn applying_the_fix_makes_the_export_named_and_the_finding_goes_away() {
        let (dir, findings) = linked_findings("\\cite{zot2020}\n\\bibliography{references}\n");
        let fix = findings[0].fix.clone().unwrap();
        let fixed = std::fs::read_to_string(dir.path().join("main.tex")).unwrap().replace(&fix.find, &fix.replace);
        std::fs::write(dir.path().join("main.tex"), fixed).unwrap();

        let index = build_index(dir.path(), Path::new("main.tex"), &["zotero/Thesis.bib".to_string()]);
        let rules: Vec<&str> = index.health(dir.path()).iter().map(|f| f.rule).collect();
        assert!(!rules.contains(&"linked-not-named"), "{rules:?}");
        assert!(!rules.contains(&"undefined-citation"), "{rules:?}");
    }

    #[test]
    fn a_root_file_in_a_subfolder_gets_a_path_relative_to_its_own_folder() {
        assert_eq!(relative_to(Path::new(""), "zotero/Thesis.bib"), "zotero/Thesis.bib");
        assert_eq!(relative_to(Path::new("paper"), "zotero/Thesis.bib"), "../zotero/Thesis.bib");
        assert_eq!(relative_to(Path::new("paper"), "paper/zotero/Thesis.bib"), "zotero/Thesis.bib");
        // And the argument the fix writes resolves back to the linked file, the round trip that
        // makes the finding go away.
        assert_eq!(resolve_bib_argument(Path::new("paper"), "../zotero/Thesis.bib"), Some("zotero/Thesis.bib".to_string()));
    }

    #[test]
    fn bibliographystyle_is_not_mistaken_for_bibliography() {
        assert_eq!(command_with_argument("\\bibliographystyle{plain}", "\\bibliography"), None);
        assert_eq!(
            command_with_argument("\\bibliographystyle{plain} \\bibliography{refs}", "\\bibliography").as_deref(),
            Some("\\bibliography{refs}")
        );
    }

    #[test]
    fn duplicate_dois_name_each_other_and_ignore_entries_with_no_doi() {
        let dir = scaffold(&[
            ("main.tex", "\\cite{a, b, c}\n\\bibliography{refs}\n"),
            (
                "refs.bib",
                "@misc{a, title={A}, doi={10.1/x}}\n@misc{b, title={B}, doi={10.1/x}}\n@misc{c, title={C}}\n",
            ),
        ]);
        let index = build_index(dir.path(), Path::new("main.tex"), &[]);
        let findings: Vec<Finding> = index.health(dir.path()).into_iter().filter(|f| f.rule == "duplicate-doi").collect();
        assert_eq!(findings.len(), 2);
        assert!(findings[0].message.contains('b') || findings[0].message.contains('a'), "{findings:#?}");
    }

    // ---- scanners ----

    #[test]
    fn scan_bib_resources_reads_both_syntaxes() {
        let source = "\\bibliography{refs, more}\n\\addbibresource[datatype=bibtex]{lib.bib}\n\\addbibresource{noext}\n";
        let expected: Vec<(String, u32)> = [("refs.bib", 1), ("more.bib", 1), ("lib.bib", 2), ("noext", 3)]
            .into_iter()
            .map(|(resource, line)| (resource.to_string(), line))
            .collect();
        assert_eq!(scan_bib_resources(source), expected);
        assert_eq!(resolve_bib_argument(Path::new(""), "noext"), Some("noext.bib".into()));
    }

    #[test]
    fn scan_bib_resources_ignores_comments_and_bibliographystyle() {
        let source = "% \\bibliography{old}\n\\bibliographystyle{plain}\n\\bibliography{refs} % trailing\n";
        assert_eq!(scan_bib_resources(source), vec![("refs.bib".to_string(), 3)]);
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

    /// natbib's `\Citep` and biblatex's `\Cite` start with a capital and contain no lower-case
    /// "cite"; the scan used to miss them, so an undefined key in one was never reported.
    #[test]
    fn scan_citations_reads_capitalised_cite_commands() {
        let source = "\\Citep{a}\n\\Cite[p.~3]{b}\n\\Citeauthor{c}";
        let keys: Vec<String> = scan_citations(source).into_iter().map(|(key, _)| key).collect();
        assert_eq!(keys, vec!["a", "b", "c"]);
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
