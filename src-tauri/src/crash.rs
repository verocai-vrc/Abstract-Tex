//! Crash reports (S16.3, design interview F8): when the app panics it writes a *local* report, and
//! the next launch offers to open a prefilled GitHub issue that the author reads and submits
//! themselves.
//!
//! **What it must never do:**
//!
//! - Never send anything. There is no network code here: the report is a text file in the app's
//!   own config folder, and "reporting" is the frontend opening a URL in the person's browser —
//!   a page they see, edit and choose to submit. No telemetry, by default or otherwise (§1.3).
//! - Never write into a project folder. Reports live beside the app's other per-machine files
//!   (`consent.rs`), outside any source tree.
//! - Never put a document in a report. It holds the app version, the platform, the panic's own
//!   message and location, and a backtrace. A panic message can still quote a path or a string,
//!   so the home folder is replaced with `~` and the author reads the whole text before it goes
//!   anywhere — that review, not the scrubbing, is the privacy guarantee.
//! - Never make a crash worse. The hook runs while the program is already panicking, so every
//!   failure inside it (no disk space, a missing folder) is swallowed: a second panic there would
//!   abort the process and lose the original message.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;

/// Where "Report on GitHub" points. The repository named in `Cargo.toml`, so a fork changes it in
/// one place.
const ISSUES_URL: &str = concat!(env!("CARGO_PKG_REPOSITORY"), "/issues/new");

/// A browser or GitHub refuses a very long URL; this is a comfortable margin below the point where
/// they do. The report is cut to fit, and the full text is still on disk and in the window.
const MAX_ISSUE_URL_BYTES: usize = 6_000;

/// The text of a report, from the pieces the panic hook gathers. Pure — no clock, no disk — so a
/// test can check exactly what a report holds.
pub fn build_report(
    version: &str,
    platform: &str,
    thread: &str,
    location: &str,
    message: &str,
    backtrace: &str,
    home: Option<&str>,
) -> String {
    let text = format!(
        "Abstract-Tex {version} ({platform})\n\
         Panic in thread '{thread}' at {location}:\n\
         {message}\n\
         \n\
         Backtrace:\n\
         {backtrace}\n"
    );
    scrub_home(&text, home)
}

/// Replace the home folder with `~` wherever it appears, so a report does not carry the person's
/// account name. An empty or missing home changes nothing (replacing "" would insert `~` between
/// every character).
pub fn scrub_home(text: &str, home: Option<&str>) -> String {
    match home {
        Some(home) if !home.is_empty() && home != "/" => text.replace(home, "~"),
        _ => text.to_string(),
    }
}

/// Percent-encode for a URL query value: everything but the unreserved characters becomes `%XX`
/// per byte (so a multi-byte character becomes several escapes, which is what URLs require).
fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(byte as char),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// A new-issue URL with the title and body filled in. The report goes in a fenced block under a
/// line asking what the author was doing, and is cut line by line from the end (the deep frames of
/// a backtrace matter least) until the whole URL fits [`MAX_ISSUE_URL_BYTES`].
pub fn issue_url(report: &str) -> String {
    let first_message_line = report
        .lines()
        .nth(2)
        .unwrap_or("the app crashed")
        .chars()
        .take(80)
        .collect::<String>();
    let title = percent_encode(&format!("Crash: {first_message_line}"));

    let mut lines: Vec<&str> = report.lines().collect();
    loop {
        let shown = lines.join("\n");
        let note = if lines.len() < report.lines().count() {
            "\n… (cut to fit; the full report is in the app's crash-reports folder)"
        } else {
            ""
        };
        let body = format!("**What were you doing when it crashed?**\n\n\n\n```text\n{shown}{note}\n```\n");
        let url = format!("{ISSUES_URL}?title={title}&body={}", percent_encode(&body));
        if url.len() <= MAX_ISSUE_URL_BYTES || lines.len() <= 3 {
            return url;
        }
        lines.pop();
    }
}

/// Install the panic hook, writing reports into `directory`.
///
/// `set_hook` replaces the process-wide handler, so the previous one (the default, which prints to
/// stderr, or a test harness's) is taken first and called afterwards: this adds a report without
/// removing the message a developer running from a terminal expects to see. `move` gives the
/// closure its own copy of `directory`; it must be `'static` because the hook outlives this call.
pub fn install(directory: PathBuf) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        let location = info
            .location()
            .map(|place| format!("{}:{}:{}", place.file(), place.line(), place.column()))
            .unwrap_or_else(|| "an unknown place".to_string());
        // The payload is a `&str` for `panic!("literal")` and a `String` for a formatted one; any
        // other type is rare and has no text to show.
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .map(|text| text.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "(no message)".to_string());
        let backtrace = std::backtrace::Backtrace::force_capture().to_string();
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .ok();
        let report = build_report(
            env!("CARGO_PKG_VERSION"),
            &format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
            thread.name().unwrap_or("unnamed"),
            &location,
            &message,
            &backtrace,
            home.as_deref(),
        );
        let _ = write_report(&directory, &report); // see the module doc: never fail inside the hook
        previous(info);
    }));
}

/// Write `text` as a new report in `directory`, creating the folder. The name is the time in
/// seconds plus a counter, so two panics in the same second do not overwrite each other.
pub fn write_report(directory: &Path, text: &str) -> io::Result<PathBuf> {
    fs::create_dir_all(directory)?;
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    for counter in 0..1000 {
        let path = directory.join(format!("crash-{seconds}-{counter}.txt"));
        // `create_new` fails if the file exists, which is what makes the counter safe.
        match fs::OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                io::Write::write_all(&mut file, text.as_bytes())?;
                return Ok(path);
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::other("too many crash reports in one second"))
}

/// A report the author has not yet been asked about.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingReport {
    /// The file's name inside the reports folder; what [`dismiss`] takes back.
    pub id: String,
    pub text: String,
    pub issue_url: String,
}

/// The newest report not yet dismissed, if any. Older ones stay on disk untouched: the author is
/// asked about the latest crash, not shown a queue.
pub fn newest_pending(directory: &Path) -> Option<PendingReport> {
    let mut names: Vec<String> = fs::read_dir(directory)
        .ok()?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name.starts_with("crash-") && name.ends_with(".txt"))
        .collect();
    // The seconds are not zero-padded, so a plain string sort would put 99 after 100; sort by the
    // numbers in the name instead.
    names.sort_by_key(|name| report_order(name));
    let id = names.pop()?;
    let text = fs::read_to_string(directory.join(&id)).ok()?;
    let issue_url = issue_url(&text);
    Some(PendingReport { id, text, issue_url })
}

fn report_order(name: &str) -> (u64, u64) {
    let mut numbers = name
        .trim_start_matches("crash-")
        .trim_end_matches(".txt")
        .split('-')
        .map(|part| part.parse::<u64>().unwrap_or(0));
    (numbers.next().unwrap_or(0), numbers.next().unwrap_or(0))
}

/// Mark a report handled by moving it into `handled/`, so it is kept (the author may want to look
/// again) but not offered again. Refuses any `id` that is not a plain file name: the id comes from
/// the webview, and `../../something` must not move a file out of the folder.
pub fn dismiss(directory: &Path, id: &str) -> io::Result<()> {
    let plain = Path::new(id).file_name().and_then(|name| name.to_str()) == Some(id);
    if !plain || !id.starts_with("crash-") || !id.ends_with(".txt") {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "not a crash report"));
    }
    let handled = directory.join("handled");
    fs::create_dir_all(&handled)?;
    fs::rename(directory.join(id), handled.join(id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_report_names_the_version_the_place_and_the_message_and_hides_the_home_folder() {
        let report = build_report(
            "0.1.0",
            "linux x86_64",
            "main",
            "/home/ada/src/x.rs:1:2",
            "no such file /home/ada/thesis/main.tex",
            "0: frame",
            Some("/home/ada"),
        );
        assert!(report.starts_with("Abstract-Tex 0.1.0 (linux x86_64)"));
        assert!(report.contains("~/src/x.rs:1:2"));
        assert!(report.contains("no such file ~/thesis/main.tex"));
        assert!(!report.contains("ada"), "the account name is gone: {report}");
    }

    #[test]
    fn an_empty_or_root_home_scrubs_nothing() {
        assert_eq!(scrub_home("a/b", Some("")), "a/b");
        assert_eq!(scrub_home("a/b", Some("/")), "a/b");
        assert_eq!(scrub_home("a/b", None), "a/b");
    }

    #[test]
    fn the_issue_url_is_encoded_and_a_long_report_is_cut_to_fit() {
        let short = issue_url("Abstract-Tex 0.1.0 (x)\nPanic at y:\nboom é\n\nBacktrace:\n0: f");
        assert!(short.starts_with(ISSUES_URL));
        assert!(short.contains("title=Crash%3A%20boom%20%C3%A9"));
        assert!(!short.contains(' '), "no raw spaces in a URL");

        let long: String = (0..2_000)
            .map(|n| format!("frame {n} somewhere/in/code.rs\n"))
            .collect();
        let report = format!("header\nPanic at y:\nboom\n{long}");
        let cut = issue_url(&report);
        assert!(cut.len() <= MAX_ISSUE_URL_BYTES, "{} bytes", cut.len());
        assert!(cut.contains("cut%20to%20fit"));
    }

    #[test]
    fn reports_are_listed_newest_first_dismissed_into_handled_and_never_offered_again() {
        let tmp = tempfile::tempdir().unwrap();
        write_report(tmp.path(), "Abstract-Tex\nPanic:\nfirst").unwrap();
        let second = write_report(tmp.path(), "Abstract-Tex\nPanic:\nsecond").unwrap();

        let newest = newest_pending(tmp.path()).unwrap();
        assert!(newest.text.contains("second"));
        assert_eq!(newest.id, second.file_name().unwrap().to_str().unwrap());

        dismiss(tmp.path(), &newest.id).unwrap();
        assert!(tmp.path().join("handled").join(&newest.id).exists());
        assert!(newest_pending(tmp.path()).unwrap().text.contains("first"));
    }

    #[test]
    fn numbers_in_names_sort_as_numbers() {
        assert!(report_order("crash-99-0.txt") < report_order("crash-100-0.txt"));
        assert!(report_order("crash-5-1.txt") < report_order("crash-5-2.txt"));
    }

    #[test]
    fn dismiss_refuses_anything_that_is_not_a_plain_report_name() {
        let tmp = tempfile::tempdir().unwrap();
        for bad in [
            "../x.txt",
            "crash-1-0.txt/../../x",
            "other.txt",
            "/etc/passwd",
            "",
        ] {
            assert!(dismiss(tmp.path(), bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn the_hook_writes_a_report_when_a_thread_panics() {
        let tmp = tempfile::tempdir().unwrap();
        install(tmp.path().to_path_buf());
        let _ = std::thread::Builder::new()
            .name("doomed".into())
            .spawn(|| panic!("the demo panic"))
            .unwrap()
            .join();
        let report = newest_pending(tmp.path()).expect("a report was written");
        assert!(report.text.contains("thread 'doomed'"));
        assert!(report.text.contains("the demo panic"));
    }
}
