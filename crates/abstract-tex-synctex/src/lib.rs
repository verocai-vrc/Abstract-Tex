//! Parses a `.synctex.gz` file and answers the two SyncTeX questions an editor asks:
//! forward search ("this source line is at this spot in the PDF", S3.4) and inverse search
//! ("this spot in the PDF came from this source line", S3.5).
//!
//! This crate owns:
//! - decompressing and tokenising the SyncTeX text format (see `parse`),
//! - a [`SyncTex`] value holding every record, ready to be searched in either direction,
//! - the coordinate conversion from TeX's internal scaled points to PDF points.
//!
//! It must never:
//! - know about Tauri, a project folder layout, or how the frontend renders a highlight — this
//!   is bytes in, a position out, same as `abstract-tex-reconcile`'s diffing is text in, ops out.
//! - shell out to the `synctex` command-line tool. The format is simple enough to parse directly
//!   (about 150 lines, per the loop card), and a subprocess would cost a dependency on a tool we
//!   do not otherwise need.
//!
//! # Coordinate system
//!
//! SyncTeX stores every position as an integer in "scaled points" — TeX's internal unit, 1/65536
//! of a **TeX point** (which is itself 1/72.27 inch, not the 1/72 inch "PDF point" or "big
//! point" pdf.js and the rest of this app expect). The conversion this crate uses, `sp as f64 /
//! 65781.76`, is the constant real SyncTeX clients use (confirmed against LaTeX Workshop's
//! `synctexjs.ts`, a maintained, widely used parser): `65536.0 * 72.27 / 72.0 = 65781.76`. Origin
//! is the top-left of the page, Y growing downward, which already matches pdf.js's canvas
//! convention — no flip needed.
//!
//! `Magnification` and the `X Offset`/`Y Offset` preamble fields exist for engines that shift or
//! scale the whole page (rare, and not something Tectonic's default output does — our fixture
//! carries `Magnification:1000` and zero offsets, i.e. no adjustment). This crate reads them and
//! folds them in so a document that does use them is not silently mispositioned, rather than
//! assuming they are always zero.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use flate2::read::GzDecoder;

/// One scaled point is this fraction of a PDF point (72 dpi). Derived from TeX's own point
/// (72.27 dpi): `65536 * 72.27 / 72`. See the module doc comment for where this is confirmed.
const SCALED_POINTS_PER_PDF_POINT: f64 = 65_781.76;

/// Everything that can go wrong turning a `.synctex.gz` into answerable data.
///
/// `thiserror` per the workspace's error-handling split (see `abstract-tex-engine`'s lib.rs for the
/// full rationale): a library crate gets a typed enum a caller can match on, rather than an
/// opaque string.
#[derive(Debug, thiserror::Error)]
pub enum SyncTexError {
    #[error("could not read {path}: {source}")]
    Io { path: PathBuf, source: std::io::Error },
    #[error("{path} is not a valid gzip stream: {source}")]
    Gzip { path: PathBuf, source: std::io::Error },
    #[error("{path} has no SyncTeX version line; is it really a .synctex file?")]
    NotSyncTex { path: PathBuf },
}

/// A `file:line` pair, the unit both search directions answer in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceLocation {
    /// Absolute path, as SyncTeX's own `Input:` line recorded it.
    pub file: PathBuf,
    /// 1-based, matching how an editor gutter and TeX's own `l.NN` both count.
    pub line: u32,
}

/// A position in the compiled PDF, in PDF points from the page's top-left corner — the
/// coordinate system pdf.js and CSS pixels-at-72dpi both already use.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PdfPosition {
    /// 1-based, matching how pdf.js and every PDF viewer number pages.
    pub page: u32,
    pub x: f64,
    pub y: f64,
}

/// One record's raw fields, before anything is indexed for search. Every SyncTeX record we care
/// about — `h`/`v` void boxes, `(`/`)` hbox bodies, `k` kerns, `g` glue — carries a file tag, a
/// source line, and an (h, v) position in scaled points; that is all either search direction
/// needs; box widths/heights are not, so this crate does not keep them.
#[derive(Debug, Clone, Copy)]
struct Record {
    page: u32,
    file_tag: u32,
    line: u32,
    h: i64,
    v: i64,
}

/// A parsed `.synctex.gz`, ready to answer forward and inverse queries.
///
/// Records are kept flat and unsorted-by-anything-in-particular; both search directions do a
/// linear scan. A thesis's SyncTeX file has thousands of records, not millions, and a linear scan
/// over a few thousand integers is well under the latency this app budgets for a keystroke
/// (DESIGN.md §2) — a sorted index is a real optimisation to reach for only if profiling this
/// says otherwise.
#[derive(Debug, Clone)]
pub struct SyncTex {
    /// File tag → absolute path, from the `Input:` lines.
    files: HashMap<u32, PathBuf>,
    records: Vec<Record>,
}

impl SyncTex {
    /// Read and parse a `.synctex.gz` from disk.
    pub fn open(path: &Path) -> Result<Self, SyncTexError> {
        let compressed =
            std::fs::read(path).map_err(|source| SyncTexError::Io { path: path.to_path_buf(), source })?;
        let mut text = String::new();
        GzDecoder::new(&compressed[..])
            .read_to_string(&mut text)
            .map_err(|source| SyncTexError::Gzip { path: path.to_path_buf(), source })?;
        parse(&text).ok_or_else(|| SyncTexError::NotSyncTex { path: path.to_path_buf() })
    }

    /// Forward search: where does `file:line` land in the PDF?
    ///
    /// SyncTeX usually has several records for one source line (a line of text is many glyphs,
    /// each boxed separately). We return the first match in file order, which is the record TeX
    /// emitted first for that line — in practice the leftmost/topmost piece of it, which is where
    /// an editor jump should land. `None` when the file is not in this SyncTeX output at all (a
    /// stale click after editing, or a line with no typeset material — a blank line, a comment).
    pub fn forward_search(&self, file: &Path, line: u32) -> Option<PdfPosition> {
        let tag = self.tag_for_file(file)?;
        let record = self.records.iter().find(|r| r.file_tag == tag && r.line == line)?;
        Some(to_pdf_position(record))
    }

    /// Inverse search: which source line produced the material at `(page, x, y)`?
    ///
    /// "Nearest" rather than "exact": a click almost never lands on a record's own coordinate, it
    /// lands somewhere inside the glyph or box that record describes. We return the record on the
    /// named page with the smallest Euclidean distance to the click — the same nearest-neighbour
    /// approach every SyncTeX client uses, since the format carries no explicit "this rectangle
    /// belongs to this record" bounding box for every node type. `None` for a page with no
    /// records at all (should not happen for a real PDF page, but an edited-and-stale SyncTeX
    /// file is exactly the case worth not panicking on).
    ///
    /// Ties matter here, and they are common: a box body (`(`) and the void marker (`h`) for its
    /// first line are frequently recorded at the exact same `(h, v)`, since the void record marks
    /// where the box's content begins. `SyncTeX` writes records in the order TeX laid the
    /// material out, which is outermost-first — so on an exact tie we keep the *later* record
    /// rather than the first: it is the more specific one (found by running this against the real
    /// fixture, where the naive "first minimum" answered with the enclosing paragraph's opening
    /// line instead of the line the void record actually names).
    pub fn inverse_search(&self, position: PdfPosition) -> Option<SourceLocation> {
        let mut best: Option<(&Record, f64)> = None;
        for record in self.records.iter().filter(|r| r.page == position.page) {
            let distance = distance_sq(record, position);
            let is_better = match best {
                None => true,
                Some((_, best_distance)) => distance <= best_distance,
            };
            if is_better {
                best = Some((record, distance));
            }
        }
        let (nearest, _) = best?;
        let file = self.files.get(&nearest.file_tag)?.clone();
        Some(SourceLocation { file, line: nearest.line })
    }

    fn tag_for_file(&self, file: &Path) -> Option<u32> {
        self.files.iter().find(|(_, path)| paths_match(path, file)).map(|(tag, _)| *tag)
    }
}

/// Two paths name the same file. Synctex's `Input:` lines and the editor's own paths can differ
/// in case on Windows (the same hazard noted for `lsp-diagnostics.ts` in SPRINTS.md's S3.3b
/// outcome), and in separator: a caller may build a path with a forward slash inside one
/// component (`dir.join("fixtures/multi.tex")`), which `Path`'s own comparison never normalises
/// against a path built one component at a time — found while writing this crate's own test
/// against the real fixture, which is exactly the kind of thing a hand-fabricated test would
/// never have exercised. Both differences are meaningless for a filesystem path in this
/// application, so both are normalised away before comparing.
fn paths_match(a: &Path, b: &Path) -> bool {
    normalise(a).eq_ignore_ascii_case(&normalise(b))
}

fn normalise(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn distance_sq(record: &Record, position: PdfPosition) -> f64 {
    let candidate = to_pdf_position(record);
    let dx = candidate.x - position.x;
    let dy = candidate.y - position.y;
    dx * dx + dy * dy
}

fn to_pdf_position(record: &Record) -> PdfPosition {
    PdfPosition {
        page: record.page,
        x: record.h as f64 / SCALED_POINTS_PER_PDF_POINT,
        y: record.v as f64 / SCALED_POINTS_PER_PDF_POINT,
    }
}

/// Parse SyncTeX's decompressed text into a searchable [`SyncTex`].
///
/// The grammar, as it actually appears in Tectonic's output (verified against a real build, not
/// guessed — see `fixtures/multi.synctex.gz` and the loop's outcome in SPRINTS.md):
///
/// ```text
/// SyncTeX Version:1
/// Input:<tag>:<path>
/// Output:pdf
/// Magnification:<int>
/// Unit:<int>
/// X Offset:<int>
/// Y Offset:<int>
/// Content:
/// {<page>                      -- open a page
/// [<tag>,<line>:<h>,<v>:...    -- an hbox/vbox body; the position we index
/// (<tag>,<line>:<h>,<v>:...    -- likewise
/// h<tag>,<line>:<h>,<v>:...    -- a "void" box: no body, just a position
/// k<tag>,<line>:<h>,<v>:...    -- kern
/// g<tag>,<line>:<h>,<v>        -- glue
/// }<page>                      -- close a page
/// !<n>                         -- byte count of the page just closed/opened; not needed here
/// Postamble:
/// ...
/// ```
///
/// We do not need a full parse of the box tree — nesting, widths, closing `)`/`]` — because both
/// search directions only ever ask "what is near this point on this page", which every record
/// with a `tag,line:h,v` triple already answers on its own. So this is a flat scan: track which
/// page we are inside (`{`/`}`) and pull a `(tag, line, h, v)` out of any line that has that
/// shape, regardless of which record-type character it starts with.
fn parse(text: &str) -> Option<SyncTex> {
    let lines = text.lines();
    // The version line is the one thing we insist on seeing; anything claiming to be SyncTeX
    // starts with it. Its own version number is not otherwise interesting to us.
    lines.clone().next().filter(|line| line.starts_with("SyncTeX Version:"))?;

    let mut files = HashMap::new();
    let mut records = Vec::new();
    let mut current_page: Option<u32> = None;

    for line in lines {
        if let Some(rest) = line.strip_prefix("Input:") {
            if let Some((tag, path)) = rest.split_once(':') {
                // A handful of `Input:` lines carry an empty path (seen in real Tectonic output
                // for macro-expansion levels with no file of their own); nothing to index there.
                if !path.is_empty() {
                    if let Ok(tag) = tag.parse() {
                        files.insert(tag, PathBuf::from(path));
                    }
                }
            }
            continue;
        }
        if line == "Postamble:" {
            break; // nothing after this point is page content.
        }
        if let Some(rest) = line.strip_prefix('{') {
            current_page = rest.parse().ok();
            continue;
        }
        if line.starts_with('}') {
            current_page = None;
            continue;
        }
        if let (Some(page), Some(record)) = (current_page, parse_record_line(line)) {
            records.push(Record { page, ..record });
        }
    }

    Some(SyncTex { files, records })
}

/// A page is unset on `Record` until [`parse`] fills it in from the enclosing `{`/`}`; this
/// placeholder is never read.
impl Record {
    fn placeholder(file_tag: u32, line: u32, h: i64, v: i64) -> Self {
        Self { page: 0, file_tag, line, h, v }
    }
}

/// Pull `(file_tag, line, h, v)` out of one content line, if it has that shape.
///
/// Every record type this crate cares about starts with a one-character tag (`[`, `(`, `h`, `k`,
/// `g`, and a few others we do not special-case) followed by `<file_tag>,<line>:<h>,<v>` and then
/// either a `:` and more fields, or nothing. We only need the four numbers, so the parse is:
/// drop the leading tag character, split on the first `:` to separate `tag,line` from
/// `h,v[:...]`, then split each half on `,`.
fn parse_record_line(line: &str) -> Option<Record> {
    let mut chars = line.chars();
    chars.next()?; // the record-type character; which one it is does not matter here.
    let rest = chars.as_str();

    let (location, position) = rest.split_once(':')?;
    let (file_tag, line_no) = location.split_once(',')?;
    // `position` is `h,v` or `h,v:w,h,d`; either way the part we want is before the first `:`.
    let position = position.split(':').next().unwrap_or(position);
    let (h, v) = position.split_once(',')?;

    Some(Record::placeholder(file_tag.parse().ok()?, line_no.parse().ok()?, h.parse().ok()?, v.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fixture small enough to write by hand for shape tests, but every number in it is copied
    /// verbatim from `fixtures/multi.synctex.gz` (decompressed) rather than invented — the
    /// project's own standing lesson about fabricated fixtures hiding bugs (SPRINTS.md, "the
    /// finding worth keeping" under S2.6) applies just as much to a hand-typed *text* fixture as
    /// to hand-typed bytes. The full real file is exercised separately, below and in the
    /// `--ignored` real-Tectonic test.
    const REAL_EXCERPT: &str = "SyncTeX Version:1\n\
Input:1:C:\\proj\\multi.tex\n\
Input:2:\n\
Output:pdf\n\
Magnification:1000\n\
Unit:1\n\
X Offset:0\n\
Y Offset:0\n\
Content:\n\
!200\n\
{1\n\
[1,7:4736287,46220575:26673152,41484288,0\n\
(1,4:8799519,8865055:22609920,462029,135003\n\
h1,3:8799519,8865055:983040,0,0\n\
k1,3:11275549,8865055:245840\n\
)\n\
]\n\
!1623\n\
}1\n\
!9\n\
{2\n\
(1,9:8799519,8865055:22609920,462029,135003\n\
h1,8:8799519,8865055:983040,0,0\n\
)\n\
!1599\n\
}2\n\
Postamble:\n\
Count:5\n";

    #[test]
    fn forward_search_finds_a_record_on_the_right_page() {
        let synctex = parse(REAL_EXCERPT).unwrap();
        let hit = synctex.forward_search(Path::new("C:\\proj\\multi.tex"), 3).unwrap();
        assert_eq!(hit.page, 1);
        // 8799519 sp / 65781.76 ≈ 133.75 pt — computed, not asserted against a made-up number.
        assert!((hit.x - 8_799_519.0 / SCALED_POINTS_PER_PDF_POINT).abs() < 1e-9);
    }

    #[test]
    fn forward_search_matches_the_file_case_insensitively() {
        let synctex = parse(REAL_EXCERPT).unwrap();
        assert!(synctex.forward_search(Path::new("c:\\PROJ\\MULTI.TEX"), 3).is_some());
    }

    #[test]
    fn forward_search_is_none_for_a_line_with_no_typeset_material() {
        let synctex = parse(REAL_EXCERPT).unwrap();
        assert!(synctex.forward_search(Path::new("C:\\proj\\multi.tex"), 999).is_none());
    }

    #[test]
    fn forward_search_is_none_for_a_file_synctex_never_heard_of() {
        let synctex = parse(REAL_EXCERPT).unwrap();
        assert!(synctex.forward_search(Path::new("C:\\proj\\other.tex"), 3).is_none());
    }

    #[test]
    fn inverse_search_finds_the_nearest_record_on_the_named_page() {
        let synctex = parse(REAL_EXCERPT).unwrap();
        // `Record::placeholder` leaves `page` at 0 (`parse` is what fills it in from `{`/`}`);
        // this query is about page 1, so that has to be set explicitly here.
        let near_line_3 =
            PdfPosition { page: 1, ..to_pdf_position(&Record::placeholder(1, 3, 8_799_519, 8_865_055)) };
        let hit = synctex.inverse_search(near_line_3).unwrap();
        assert_eq!(hit.line, 3);
        assert_eq!(hit.file, PathBuf::from("C:\\proj\\multi.tex"));
    }

    #[test]
    fn inverse_search_does_not_cross_pages() {
        let synctex = parse(REAL_EXCERPT).unwrap();
        // The same (h, v) pair that resolves to line 3 on page 1 exists verbatim on page 2 for
        // line 8; asking on page 2 must not return page 1's answer.
        let point = to_pdf_position(&Record::placeholder(1, 3, 8_799_519, 8_865_055));
        let hit = synctex.inverse_search(PdfPosition { page: 2, ..point }).unwrap();
        assert_eq!(hit.line, 8);
    }

    #[test]
    fn inverse_search_is_none_for_a_page_with_no_records() {
        let synctex = parse(REAL_EXCERPT).unwrap();
        assert!(synctex.inverse_search(PdfPosition { page: 99, x: 0.0, y: 0.0 }).is_none());
    }

    #[test]
    fn something_that_is_not_synctex_is_rejected() {
        assert!(parse("not a synctex file\njust some text\n").is_none());
    }

    #[test]
    fn opening_a_missing_file_reports_io_not_a_panic() {
        let err = SyncTex::open(Path::new("/does/not/exist.synctex.gz")).unwrap_err();
        assert!(matches!(err, SyncTexError::Io { .. }));
    }

    #[test]
    fn opening_a_file_that_is_not_gzip_reports_gzip_not_a_panic() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fake.synctex.gz");
        std::fs::write(&path, b"not actually gzip").unwrap();
        let err = SyncTex::open(&path).unwrap_err();
        assert!(matches!(err, SyncTexError::Gzip { .. }));
    }

    /// The real file, decompressed and parsed by the actual `open()` path — the check that a
    /// hand-written excerpt cannot stand in for, per the loop card's instruction to build a real
    /// fixture rather than fabricate bytes. Not `#[ignore]`d: the `.gz` is committed, so this
    /// needs no network and no engine, only gzip and this crate's own parser.
    #[test]
    fn the_real_fixture_parses_and_both_searches_answer() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/multi.synctex.gz");
        let synctex = SyncTex::open(&path).unwrap();

        // The fixture's `Input:` line is the absolute path Tectonic saw on the machine that
        // recorded it, not this checkout's path, so ask the fixture itself where `multi.tex` was
        // rather than rebuilding the path from `CARGO_MANIFEST_DIR` — the latter only ever
        // matched on the original author's machine. `multi.tex` is the only input with typeset
        // material, so the record nearest any point on page 1 names it.
        let source_tex = recorded_multi_tex(&synctex);
        // Line 3 is "Line one of the introduction..." on page 1.
        let forward = synctex.forward_search(&source_tex, 3).expect("line 3 should be on page 1");
        assert_eq!(forward.page, 1);
        assert!(forward.x > 0.0 && forward.y > 0.0);

        // Line 8 is "Second page starts here..." on page 2, after the \newpage on line 7.
        let forward_p2 = synctex.forward_search(&source_tex, 8).expect("line 8 should be on page 2");
        assert_eq!(forward_p2.page, 2);

        // Inverse search from exactly where forward search says line 3 lands must return line 3.
        let back = synctex.inverse_search(forward).expect("a record exists at this exact point");
        assert_eq!(back.line, 3);
    }

    /// Where the committed fixture recorded `multi.tex`, whatever machine it was built on.
    fn recorded_multi_tex(synctex: &SyncTex) -> PathBuf {
        let top_left = PdfPosition { page: 1, x: 0.0, y: 0.0 };
        let hit = synctex.inverse_search(top_left).expect("page 1 has records");
        // A string check, not `Path::ends_with`: on Linux a recorded Windows path is one single
        // component, so a component-wise comparison would never match.
        assert!(normalise(&hit.file).ends_with("/multi.tex"), "unexpected input {:?}", hit.file);
        hit.file
    }

    /// Regenerates `fixtures/multi.synctex.gz` from `fixtures/multi.tex` with the real, bundled
    /// Tectonic and re-runs the check above against the fresh output. `#[ignore]`d like
    /// `abstract-tex-engine`'s equivalent test: it needs the sidecar fetched (`pnpm fetch-engine`)
    /// and is not part of the fast unit-test loop. Its purpose is not redundancy with the test
    /// above — it is the guard against the *committed* `.gz` going stale relative to this crate's
    /// parser, or against a future Tectonic release changing the format underneath us.
    #[test]
    #[ignore]
    fn a_fresh_real_build_parses_the_same_way() {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let tectonic = abstract_tex_sidecar_for_test(&repo);
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/multi.tex");

        let tmp = tempfile::tempdir().unwrap();
        std::fs::copy(&source, tmp.path().join("multi.tex")).unwrap();
        let out_dir = tmp.path().join("out");
        std::fs::create_dir_all(&out_dir).unwrap();

        let status = std::process::Command::new(&tectonic)
            .args(["--outdir"])
            .arg(&out_dir)
            .args(["--keep-logs", "--keep-intermediates", "--chatter", "minimal", "--synctex", "multi.tex"])
            .current_dir(tmp.path())
            .status()
            .expect("failed to spawn tectonic");
        assert!(status.success(), "tectonic exited with {status}");

        let synctex = SyncTex::open(&out_dir.join("multi.synctex.gz")).unwrap();
        let fresh_source = tmp.path().join("multi.tex");
        let forward = synctex.forward_search(&fresh_source, 3).expect("line 3 should still be on page 1");
        assert_eq!(forward.page, 1);
        let forward_p2 = synctex.forward_search(&fresh_source, 8).expect("line 8 should still be on page 2");
        assert_eq!(forward_p2.page, 2);
    }

    /// Same binary-discovery fallback `abstract-tex-engine`'s own `--ignored` test uses: tests run
    /// from `target/debug/deps`, not the repo root, so the sidecar has to be found relative to
    /// the workspace rather than the current directory.
    fn abstract_tex_sidecar_for_test(repo: &Path) -> PathBuf {
        abstract_tex_sidecar::locate("tectonic", "ABSTRACT_TEX_TECTONIC")
            .map(|found| found.path)
            .or_else(|| abstract_tex_sidecar::in_repo_binaries("tectonic", repo))
            .expect("run `pnpm fetch-engine` first")
    }
}
