//! The finished PDF: the copy of it that is kept, and saving it where the person wants it (S17.7).
//!
//! Every good build's PDF is copied to `.abstract-tex/latest.pdf`. The one the engine writes is not
//! enough: a failed build may leave a broken file in its place, and a build that is running has it half
//! written. What is saved is always the copy.
//!
//! **What it must never do:** hand out a PDF of another document, or of one chapter (a draft), or open
//! and show any file but the one just saved.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime, State};
use tokio::sync::watch;

use crate::commands::{to_message, CommandResult};
use crate::compile::CompileEvent;
use crate::project::write_bytes_atomically;
use crate::AppState;

/// Which PDF there is once the build that was waited for has ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kept {
    /// That build's own, or a newer one's.
    Newest,
    /// One from before it: the build failed.
    Older,
    /// That build was good, but the copy of its PDF could not be made.
    Lost,
    /// No build of this run of the app has made one.
    None,
}

#[derive(Debug, Clone, Copy, Default)]
struct Builds {
    /// The newest build that has ended, its PDF already copied. Builds are numbered from 1.
    last_ended: u64,
    /// The newest whose PDF was kept.
    last_kept: Option<u64>,
    /// The newest that was good and whose PDF could not be copied.
    last_lost: Option<u64>,
}

/// How far the builds have got. One per app, in `AppState`.
pub struct FinishedPdf {
    /// A `watch` channel: one value that changes, and that a command can wait on without asking again
    /// and again.
    builds: watch::Sender<Builds>,
    /// The file the window may open and show, and no other.
    saved: Mutex<Option<PathBuf>>,
}

impl Default for FinishedPdf {
    fn default() -> Self {
        Self {
            builds: watch::Sender::new(Builds::default()),
            saved: Mutex::new(None),
        }
    }
}

impl FinishedPdf {
    /// Hear what a build said, and keep its PDF in `latest` if it has a good one. Called from the
    /// build's own task, which the next build waits for, so the file copied is never half written.
    pub fn note(&self, event: &CompileEvent, latest: &Path) {
        match event {
            CompileEvent::Finished {
                generation,
                success: true,
                pdf_path: Some(pdf),
                ..
            } => {
                let copied = std::fs::read(pdf).and_then(|bytes| write_bytes_atomically(latest, &bytes));
                if let Err(error) = &copied {
                    tracing::warn!(%error, "the finished PDF could not be kept");
                    self.builds
                        .send_modify(|builds| builds.last_lost = builds.last_lost.max(Some(*generation)));
                }
                self.ended(*generation, copied.is_ok());
            }
            CompileEvent::Finished { generation, .. } | CompileEvent::Failed { generation, .. } => {
                self.ended(*generation, false);
            }
            CompileEvent::Started { .. } | CompileEvent::Progress { .. } | CompileEvent::Draft { .. } => {}
        }
    }

    /// Build number `generation` is over, and `kept` says whether its PDF is now the kept one.
    pub fn ended(&self, generation: u64, kept: bool) {
        self.builds.send_modify(|builds| {
            builds.last_ended = builds.last_ended.max(generation);
            if kept {
                builds.last_kept = builds.last_kept.max(Some(generation));
            }
        });
    }

    /// Wait until build `generation`, or one that replaced it, has ended and its PDF is copied; at most
    /// `limit`.
    pub async fn wait_for(&self, generation: u64, limit: Duration) -> Kept {
        let mut builds = self.builds.subscribe();
        // Both errors mean the same: what there is now is all there is.
        let _ = tokio::time::timeout(limit, builds.wait_for(|builds| builds.last_ended >= generation)).await;
        // A copy, so that the channel is not held.
        let builds = *builds.borrow();
        match (builds.last_kept, builds.last_lost) {
            (Some(kept), _) if kept >= generation => Kept::Newest,
            (_, Some(lost)) if lost >= generation => Kept::Lost,
            (Some(_), _) => Kept::Older,
            (None, _) => Kept::None,
        }
    }
}

/// Whether there is a kept PDF at `latest`. A link there is not one: a folder from somebody else can
/// carry one that points at any file.
pub fn is_there(latest: &Path) -> bool {
    std::fs::symlink_metadata(latest).is_ok_and(|found| found.is_file())
}

/// The longest file name on offer, before `.pdf`.
const LONGEST_NAME: usize = 120;

/// A document's title as a file name that every system takes.
pub fn file_name_for(title: &str) -> String {
    let allowed: String = title
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                ' '
            } else {
                c
            }
        })
        .collect();
    let words: Vec<&str> = allowed.split_whitespace().collect();
    let mut name: String = words.join(" ").chars().take(LONGEST_NAME).collect();
    // A name may be 255 *bytes* long, and a letter is up to four. It is written beside itself first, under
    // a name that may be longer (`write_bytes_atomically`), and ".pdf" and a word may be added below.
    while name.len() > 225 {
        name.pop();
    }
    let mut name = name.trim_matches(|c| c == '.' || c == ' ').to_string();
    if name.is_empty() {
        name = "Document".to_string();
    }
    if is_device_name(&name) {
        name.insert_str(0, "Document ");
    }
    name + ".pdf"
}

/// Names Windows keeps for itself, whatever follows their first dot.
fn is_device_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name).trim().to_uppercase();
    let numbered = |device: &str| {
        stem.strip_prefix(device)
            .is_some_and(|rest| matches!(rest.as_bytes(), [b'1'..=b'9']))
    };
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL") || numbered("COM") || numbered("LPT")
}

/// The file to write for the path a save dialog answered: some dialogs do not add `.pdf`. Then nobody
/// was asked about replacing the file that name with `.pdf` may already be, so that is refused.
fn destination_of(chosen: &Path) -> CommandResult<PathBuf> {
    if !chosen.is_absolute() {
        return Err("That is not a place a file can be saved in.".into());
    }
    let has_ending = chosen
        .extension()
        .is_some_and(|ending| ending.eq_ignore_ascii_case("pdf"));
    if has_ending {
        return Ok(chosen.to_path_buf());
    }
    let mut name = chosen.file_name().unwrap_or_default().to_os_string();
    name.push(".pdf");
    let destination = chosen.with_file_name(&name);
    if std::fs::symlink_metadata(&destination).is_ok() {
        return Err(format!(
            "There is already a file called \"{}\" there. Choose another name.",
            name.to_string_lossy()
        ));
    }
    Ok(destination)
}

/// What was saved, as the window says it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SavedPdf {
    name: String,
    place: Vec<String>,
    is_newest: bool,
}

/// What became of saving the PDF. The window has the words for each.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PdfSaving {
    Saved(SavedPdf),
    /// The document builds, but the copy of its PDF is gone or could not be made. Another build makes it.
    NotKept,
    /// There is none, and the last build made none.
    NoPdf,
}

/// Where the save dialog starts: the document's title, in Documents.
#[tauri::command]
pub fn suggested_pdf_path<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
) -> CommandResult<String> {
    let open = state.project.lock().unwrap();
    let project = open.as_ref().ok_or("No document is open.")?;
    let folders = app.path();
    let folder = folders
        .document_dir()
        .or_else(|_| folders.home_dir())
        .unwrap_or_default();
    Ok(folder
        .join(file_name_for(&project.title))
        .to_string_lossy()
        .into_owned())
}

/// Save the open document's newest PDF at `destination`, once build `generation` has ended. `is_retry`
/// when that build was asked for because of a [`PdfSaving::NotKept`]: then an older PDF is better than none.
#[tauri::command]
pub async fn save_pdf<R: Runtime>(
    app: AppHandle<R>,
    destination: String,
    generation: u64,
    is_retry: bool,
) -> CommandResult<PdfSaving> {
    let state = app.state::<AppState>();
    let open_document = || {
        let open = state.project.lock().unwrap();
        open.as_ref()
            .map(|project| (project.root_dir.clone(), project.latest_pdf()))
    };
    let (root_dir, latest) = open_document().ok_or("No document is open.")?;
    let kept = state
        .finished_pdf
        .wait_for(generation, abstract_tex_engine::BUILD_TIME_LIMIT)
        .await;
    if !open_document().is_some_and(|(open_now, _)| open_now == root_dir) {
        return Err("Another document was opened meanwhile, so nothing was saved.".into());
    }
    let home = app.path().home_dir().ok();
    let saved = tauri::async_runtime::spawn_blocking(move || {
        let destination = destination_of(Path::new(&destination))?;
        // Not only "none was kept": a document's own may be from another day.
        match (is_there(&latest), kept) {
            (true, Kept::Lost) if !is_retry => return Ok((PdfSaving::NotKept, None)),
            (true, _) => {}
            (false, Kept::Newest | Kept::Lost) => return Ok((PdfSaving::NotKept, None)),
            (false, Kept::Older | Kept::None) => return Ok((PdfSaving::NoPdf, None)),
        }
        let name = crate::recents::folder_name(&destination);
        std::fs::read(&latest)
            .and_then(|bytes| write_bytes_atomically(&destination, &bytes))
            .map_err(|error| {
                format!(
                    "\"{name}\" could not be saved ({}). If it is open in another program, close it there.",
                    error.kind()
                )
            })?;
        let place = crate::start::place_parts(&destination, home.as_deref());
        let saved = SavedPdf {
            name,
            place,
            is_newest: kept == Kept::Newest,
        };
        Ok::<_, String>((PdfSaving::Saved(saved), Some(destination)))
    })
    .await
    .map_err(to_message)?;
    let (answer, destination) = saved?;
    if let Some(destination) = destination {
        *state.finished_pdf.saved.lock().unwrap() = Some(destination);
    }
    Ok(answer)
}

/// Run `work` on the PDF saved last, off the async workers: it starts another program.
async fn with_saved<R: Runtime>(
    app: &AppHandle<R>,
    work: impl FnOnce(PathBuf) -> CommandResult<()> + Send + 'static,
) -> CommandResult<()> {
    let saved = app.state::<AppState>().finished_pdf.saved.lock().unwrap().clone();
    let saved = saved.ok_or("No PDF has been saved.")?;
    tauri::async_runtime::spawn_blocking(move || work(saved))
        .await
        .map_err(to_message)?
}

/// Open the PDF saved last in the program the computer opens PDFs with, which is also how to print.
#[tauri::command]
pub async fn open_saved_pdf<R: Runtime>(app: AppHandle<R>) -> CommandResult<()> {
    with_saved(&app, |saved| {
        tauri_plugin_opener::open_path(saved, None::<&str>).map_err(to_message)
    })
    .await
}

#[tauri::command]
pub async fn show_saved_pdf<R: Runtime>(app: AppHandle<R>) -> CommandResult<()> {
    with_saved(&app, |saved| {
        tauri_plugin_opener::reveal_item_in_dir(saved).map_err(to_message)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::Project;

    const SOON: Duration = Duration::from_millis(200);

    #[test]
    fn a_title_becomes_a_file_name_every_system_takes() {
        for (title, name) in [
            ("Thesis draft", "Thesis draft.pdf"),
            ("My Essay: v2", "My Essay v2.pdf"),
            ("Q1/Q2 <now>?", "Q1 Q2 now.pdf"),
            ("São Paulo", "São Paulo.pdf"),
            ("CON", "Document CON.pdf"),
            ("Con. Law outline", "Document Con. Law outline.pdf"),
            ("Common sense", "Common sense.pdf"),
            ("  ... ", "Document.pdf"),
        ] {
            assert_eq!(file_name_for(title), name);
        }
        assert_eq!(file_name_for(&"a".repeat(300)).chars().count(), 124); // 120 and ".pdf"
    }

    #[test]
    fn the_name_on_offer_for_a_long_title_can_be_saved_under() {
        let (folder, out) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let app = app_with(folder.path());
        std::fs::write(folder.path().join(".abstract-tex/latest.pdf"), "pages").unwrap();
        for letter in ["語", "Ж", "𝒜", "a"] {
            save(&app, &out.path().join(file_name_for(&letter.repeat(300))), 0).unwrap();
        }
    }

    /// Windows looks at what stands before the first dot, so a word at the end does not help.
    #[test]
    fn no_name_on_offer_is_one_windows_keeps_for_itself() {
        for title in [
            "CON",
            "Con. Law outline",
            "com1.notes",
            "nul . x",
            "LPT9.1.2",
            "aux",
        ] {
            let name = file_name_for(title);
            assert!(!is_device_name(&name), "{name}");
        }
    }

    fn finished(generation: u64, success: bool, pdf: &Path) -> CompileEvent {
        CompileEvent::Finished {
            generation,
            success,
            pdf_path: Some(pdf.to_string_lossy().into_owned()),
            log_path: None,
            diagnostics: Vec::new(),
            duration_ms: 1,
            stderr: String::new(),
        }
    }

    #[test]
    fn waits_for_the_copy_and_not_only_for_the_event() {
        let pdf = std::sync::Arc::new(FinishedPdf::default());
        pdf.ended(4, true);
        tauri::async_runtime::block_on(async {
            let waiting = {
                let pdf = pdf.clone();
                tauri::async_runtime::spawn(async move { pdf.wait_for(5, Duration::from_secs(60)).await })
            };
            tokio::time::sleep(Duration::from_millis(100)).await;
            assert!(!waiting.inner().is_finished(), "build 5 has not ended");
            pdf.ended(5, true);
            assert_eq!(waiting.await.unwrap(), Kept::Newest);
        });
    }

    #[test]
    fn a_failed_build_answers_older() {
        let pdf = FinishedPdf::default();
        pdf.ended(4, true);
        pdf.ended(5, false);
        assert_eq!(tauri::async_runtime::block_on(pdf.wait_for(5, SOON)), Kept::Older);
    }

    #[test]
    fn a_build_that_was_replaced_answers_for_the_one_that_replaced_it() {
        let pdf = FinishedPdf::default();
        pdf.ended(6, true);
        pdf.ended(5, true); // heard late
        let asked = std::time::Instant::now();
        assert_eq!(
            tauri::async_runtime::block_on(pdf.wait_for(5, SOON)),
            Kept::Newest
        );
        assert_eq!(
            tauri::async_runtime::block_on(pdf.wait_for(6, SOON)),
            Kept::Newest
        );
        assert!(asked.elapsed() < SOON, "nothing is left to wait for");
    }

    #[test]
    fn a_build_that_never_ends_is_not_waited_for_for_ever() {
        let pdf = FinishedPdf::default();
        pdf.ended(4, true);
        assert_eq!(tauri::async_runtime::block_on(pdf.wait_for(5, SOON)), Kept::Older);
    }

    #[test]
    fn nothing_ever_kept_answers_none() {
        let pdf = FinishedPdf::default();
        pdf.ended(1, false);
        assert_eq!(tauri::async_runtime::block_on(pdf.wait_for(1, SOON)), Kept::None);
    }

    #[test]
    fn the_last_good_pdf_survives_a_broken_one_and_a_draft_never_becomes_it() {
        let folder = tempfile::tempdir().unwrap();
        let (built, latest) = (folder.path().join("main.pdf"), folder.path().join("latest.pdf"));
        let pdf = FinishedPdf::default();
        pdf.note(
            &CompileEvent::Draft {
                generation: 1,
                chapter: "a".into(),
                pdf_path: built.to_string_lossy().into_owned(),
                duration_ms: 1,
            },
            &latest,
        );
        assert!(!latest.exists());

        std::fs::write(&built, "good").unwrap();
        pdf.note(&finished(1, true, &built), &latest);
        std::fs::write(&built, "bro").unwrap();
        pdf.note(&finished(2, false, &built), &latest);
        assert_eq!(std::fs::read_to_string(&latest).unwrap(), "good");
        assert_eq!(tauri::async_runtime::block_on(pdf.wait_for(2, SOON)), Kept::Older);
    }

    #[test]
    fn a_pdf_that_could_not_be_copied_is_not_the_newest() {
        let folder = tempfile::tempdir().unwrap();
        let pdf = FinishedPdf::default();
        pdf.note(
            &finished(1, true, &folder.path().join("gone.pdf")),
            &folder.path().join("latest.pdf"),
        );
        assert_eq!(tauri::async_runtime::block_on(pdf.wait_for(1, SOON)), Kept::Lost);
        // And after a build that failed, it is the failure that counts.
        pdf.ended(2, false);
        assert_eq!(tauri::async_runtime::block_on(pdf.wait_for(2, SOON)), Kept::None);
    }

    /// An app with a document open in `folder`.
    fn app_with(folder: &Path) -> tauri::App<tauri::test::MockRuntime> {
        let app = tauri::test::mock_builder()
            .manage(AppState::new())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        *app.state::<AppState>().project.lock().unwrap() = Some(Project::open(folder).unwrap());
        app
    }

    fn answer(
        app: &tauri::App<tauri::test::MockRuntime>,
        destination: &Path,
        generation: u64,
        is_retry: bool,
    ) -> CommandResult<PdfSaving> {
        let destination = destination.to_string_lossy().into_owned();
        tauri::async_runtime::block_on(save_pdf(app.handle().clone(), destination, generation, is_retry))
    }

    /// What was saved, where the answer must be that something was.
    fn save(
        app: &tauri::App<tauri::test::MockRuntime>,
        destination: &Path,
        generation: u64,
    ) -> CommandResult<SavedPdf> {
        answer(app, destination, generation, false).map(|answer| match answer {
            PdfSaving::Saved(saved) => saved,
            other => panic!("{other:?}"),
        })
    }

    #[test]
    fn a_kept_pdf_that_is_gone_or_could_not_be_made_is_not_called_a_failed_build() {
        let (folder, out) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let app = app_with(folder.path());
        let builds = &app.state::<AppState>().finished_pdf;
        let latest = folder.path().join(".abstract-tex/latest.pdf");
        let destination = out.path().join("a.pdf");

        // Deleted while the document is open.
        builds.ended(1, true);
        assert_eq!(answer(&app, &destination, 1, false), Ok(PdfSaving::NotKept));
        assert_eq!(answer(&app, &destination, 1, true), Ok(PdfSaving::NotKept));

        // Never copied, and an older one is there: that is what there is, but only after another try.
        builds.note(&finished(2, true, &folder.path().join("gone.pdf")), &latest);
        std::fs::write(&latest, "older pages").unwrap();
        assert_eq!(answer(&app, &destination, 2, false), Ok(PdfSaving::NotKept));
        assert!(!destination.exists());
        assert!(
            matches!(answer(&app, &destination, 2, true), Ok(PdfSaving::Saved(saved)) if !saved.is_newest)
        );

        std::fs::remove_file(&latest).unwrap();
        builds.ended(3, false);
        assert_eq!(answer(&app, &destination, 3, false), Ok(PdfSaving::NoPdf));
    }

    #[test]
    fn what_is_saved_is_the_kept_pdf_and_it_says_whether_it_is_the_newest() {
        let (folder, out) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let app = app_with(folder.path());
        let state = app.state::<AppState>();
        let destination = out.path().join("Thesis.pdf");

        state.finished_pdf.ended(1, false);
        assert_eq!(
            answer(&app, &destination, 1, false),
            Ok(PdfSaving::NoPdf),
            "there is none yet"
        );
        assert!(!destination.exists());
        assert!(tauri::async_runtime::block_on(open_saved_pdf(app.handle().clone())).is_err());

        // From another day: there is one, and it is not this text's.
        std::fs::write(folder.path().join(".abstract-tex/latest.pdf"), "pages").unwrap();
        let saved = save(&app, &destination, 1).unwrap();
        assert_eq!((saved.name.as_str(), saved.is_newest), ("Thesis.pdf", false));

        state.finished_pdf.ended(2, true);
        std::fs::write(folder.path().join(".abstract-tex/latest.pdf"), "new pages").unwrap();
        assert!(save(&app, &destination, 2).unwrap().is_newest);
        assert_eq!(std::fs::read_to_string(&destination).unwrap(), "new pages");
        assert_eq!(
            state.finished_pdf.saved.lock().unwrap().as_deref(),
            Some(destination.as_path())
        );
    }

    #[test]
    fn another_document_opened_meanwhile_is_refused() {
        let (folder, other, out) = (
            tempfile::tempdir().unwrap(),
            tempfile::tempdir().unwrap(),
            tempfile::tempdir().unwrap(),
        );
        let app = app_with(folder.path());
        std::fs::write(folder.path().join(".abstract-tex/latest.pdf"), "pages").unwrap();
        let destination = out.path().join("Thesis.pdf");
        let answer = tauri::async_runtime::block_on(async {
            let saving = tauri::async_runtime::spawn(save_pdf(
                app.handle().clone(),
                destination.to_string_lossy().into_owned(),
                5,
                false,
            ));
            tokio::time::sleep(Duration::from_millis(100)).await;
            let state = app.state::<AppState>();
            *state.project.lock().unwrap() = Some(Project::open(other.path()).unwrap());
            state.finished_pdf.ended(5, true);
            saving.await.unwrap()
        });
        assert!(answer.unwrap_err().contains("Another document"));
        assert!(!destination.exists());
    }

    #[test]
    fn a_name_with_no_ending_gets_pdf_but_never_over_another_file() {
        let (folder, out) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let app = app_with(folder.path());
        std::fs::write(folder.path().join(".abstract-tex/latest.pdf"), "pages").unwrap();

        assert_eq!(
            save(&app, &out.path().join("Thesis"), 0).unwrap().name,
            "Thesis.pdf"
        );
        std::fs::write(out.path().join("Other.pdf"), "somebody's").unwrap();
        let refused = save(&app, &out.path().join("Other"), 0).unwrap_err();
        assert!(refused.contains("Other.pdf"), "{refused}");
        assert_eq!(
            std::fs::read_to_string(out.path().join("Other.pdf")).unwrap(),
            "somebody's"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_link_in_the_place_of_the_kept_pdf_is_not_one() {
        let (folder, out) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let app = app_with(folder.path());
        std::fs::write(out.path().join("secret"), "secret").unwrap();
        let latest = folder.path().join(".abstract-tex/latest.pdf");
        std::os::unix::fs::symlink(out.path().join("secret"), &latest).unwrap();

        assert!(!is_there(&latest));
        assert_eq!(
            answer(&app, &out.path().join("a.pdf"), 0, false),
            Ok(PdfSaving::NoPdf)
        );
        // And a build's copy goes in its place, not through it.
        std::fs::write(folder.path().join("main.pdf"), "pages").unwrap();
        let state = app.state::<AppState>();
        state
            .finished_pdf
            .note(&finished(1, true, &folder.path().join("main.pdf")), &latest);
        assert_eq!(
            std::fs::read_to_string(out.path().join("secret")).unwrap(),
            "secret"
        );
        assert!(is_there(&latest));
    }
}
