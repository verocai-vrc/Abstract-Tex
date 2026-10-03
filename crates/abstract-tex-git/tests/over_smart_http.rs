//! S11.8: clone, push, fetch and sync over *real smart HTTP*, which no other test here touches.
//!
//! Every unit test in `lib.rs` talks to a bare repository on the local disk, and libgit2's local
//! transport shares almost nothing with its HTTP one: no credential callback is ever consulted,
//! no request is ever made. DESIGN.md §9's mitigation is "test against a bare remote in CI so the
//! generic path never rots"; this is that test, with the transport that GitHub, GitLab and every
//! self-hosted server actually use.
//!
//! The server is `git http-backend` — Git's own CGI, the same code a real server runs — behind
//! forty lines of `TcpListener` that turn an HTTP request into a CGI call. It can also be told to
//! demand a password, which is how the second half of this file proves what the credential
//! callback does and does not do.
//!
//! `#[ignore]`d, like every test in this workspace that needs a binary we do not ship: it wants
//! `git` (and so `git-http-backend`) installed. CI runs it on Linux:
//! `cargo test -p abstract-tex-git --test over_smart_http -- --ignored`.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use abstract_tex_git::{branch_state, clone, push, set_origin, stage, sync, GitError, Repository, SyncOutcome};

/// A running fake server and what it has seen.
struct Server {
    /// `http://127.0.0.1:PORT`, with no trailing slash.
    origin: String,
    /// Every `Authorization` header any request carried, in order.
    authorization_seen: Arc<Mutex<Vec<String>>>,
    /// The folder that holds `project.git`; kept alive for as long as the server is.
    root: tempfile::TempDir,
}

impl Server {
    fn url(&self) -> String {
        format!("{}/project.git", self.origin)
    }

    fn bare(&self) -> PathBuf {
        self.root.path().join("project.git")
    }
}

/// Serve a bare repository, seeded with one commit holding `main.tex`, over HTTP.
///
/// `needs_password`: answer `401` with a Basic challenge to any request that has no
/// `Authorization` header, as a private repository on a real server does.
fn serve(needs_password: bool) -> Server {
    let root = tempfile::tempdir().unwrap();
    let bare = root.path().join("project.git");
    Repository::init_bare(&bare).unwrap();

    // Seed it by pushing from an ordinary working repository over the *local* path, so what the
    // tests below exercise is HTTP and not seeding.
    let seed = tempfile::tempdir().unwrap();
    let repository = Repository::init(seed.path()).unwrap();
    std::fs::write(seed.path().join("main.tex"), "the manuscript\n").unwrap();
    stage(&repository, "main.tex").unwrap();
    commit(&repository, "first");
    set_origin(&repository, bare.to_str().unwrap()).unwrap();
    push(&repository, None).unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let authorization_seen = Arc::new(Mutex::new(Vec::new()));

    let project_root = root.path().to_path_buf();
    let seen = Arc::clone(&authorization_seen);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let project_root = project_root.clone();
            let seen = Arc::clone(&seen);
            // One thread per connection: libgit2 may hold one open while it opens the next.
            std::thread::spawn(move || {
                let _ = handle(stream, &project_root, needs_password, &seen);
            });
        }
    });

    Server { origin, authorization_seen, root }
}

/// A commit of whatever is staged, authored by nobody in particular.
fn commit(repository: &Repository, message: &str) {
    let who = git2::Signature::now("Ada", "ada@example.invalid").unwrap();
    let mut index = repository.index().unwrap();
    let tree = repository.find_tree(index.write_tree().unwrap()).unwrap();
    let parent = repository.head().ok().and_then(|head| head.peel_to_commit().ok());
    let parents: Vec<&git2::Commit<'_>> = parent.iter().collect();
    repository.commit(Some("HEAD"), &who, &who, message, &tree, &parents).unwrap();
}

/// One request: parse it, run `git http-backend` on it, send back what it said.
fn handle(
    mut stream: TcpStream,
    project_root: &Path,
    needs_password: bool,
    seen: &Mutex<Vec<String>>,
) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let mut parts = request_line.split_whitespace();
    let (method, target) = (parts.next().unwrap_or("").to_string(), parts.next().unwrap_or("/").to_string());

    let mut content_length = 0usize;
    let mut content_type = String::new();
    let mut chunked = false;
    let mut authorization = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 || line == "\r\n" {
            break;
        }
        let lower = line.to_ascii_lowercase();
        if let Some(value) = lower.strip_prefix("content-length:") {
            content_length = value.trim().parse().unwrap_or(0);
        } else if let Some(value) = line.get("content-type:".len()..).filter(|_| lower.starts_with("content-type:")) {
            content_type = value.trim().to_string();
        } else if lower.starts_with("transfer-encoding:") && lower.contains("chunked") {
            chunked = true;
        } else if lower.starts_with("authorization:") {
            authorization = Some(line["authorization:".len()..].trim().to_string());
        }
    }

    let mut body = Vec::new();
    if chunked {
        loop {
            let mut size_line = String::new();
            reader.read_line(&mut size_line)?;
            let size = usize::from_str_radix(size_line.trim(), 16).unwrap_or(0);
            if size == 0 {
                let mut trailer = String::new();
                reader.read_line(&mut trailer)?;
                break;
            }
            let mut chunk = vec![0u8; size];
            reader.read_exact(&mut chunk)?;
            body.extend_from_slice(&chunk);
            let mut crlf = [0u8; 2];
            reader.read_exact(&mut crlf)?;
        }
    } else if content_length > 0 {
        body = vec![0u8; content_length];
        reader.read_exact(&mut body)?;
    }

    if let Some(header) = &authorization {
        seen.lock().unwrap().push(header.clone());
    }
    if needs_password && authorization.is_none() {
        let reply = "HTTP/1.1 401 Unauthorized\r\nWWW-Authenticate: Basic realm=\"git\"\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        return stream.write_all(reply.as_bytes());
    }

    let (path, query) = target.split_once('?').unwrap_or((target.as_str(), ""));
    let mut backend = Command::new("git")
        .arg("http-backend")
        .env("GIT_PROJECT_ROOT", project_root)
        .env("GIT_HTTP_EXPORT_ALL", "1")
        // Without a REMOTE_USER, `http-backend` refuses `receive-pack` (a push) unless the
        // repository opts in; a server that has authenticated somebody is exactly the case where
        // pushing is on by default.
        .env("REMOTE_USER", "tester")
        .env("REQUEST_METHOD", &method)
        .env("PATH_INFO", path)
        .env("QUERY_STRING", query)
        .env("CONTENT_TYPE", &content_type)
        .env("CONTENT_LENGTH", body.len().to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    backend.stdin.take().unwrap().write_all(&body)?;
    let output = backend.wait_with_output()?;

    // CGI output: headers, a blank line, the body. `Status:` carries the HTTP status.
    let split = output.stdout.windows(4).position(|window| window == b"\r\n\r\n");
    let (headers, payload) = match split {
        Some(at) => (&output.stdout[..at], &output.stdout[at + 4..]),
        None => (&output.stdout[..0], &output.stdout[..]),
    };
    let headers = String::from_utf8_lossy(headers);
    let status = headers
        .lines()
        .find_map(|line| line.strip_prefix("Status:"))
        .map(|status| status.trim().to_string())
        .unwrap_or_else(|| "200 OK".to_string());
    let mut reply = format!("HTTP/1.1 {status}\r\n");
    for line in headers.lines().filter(|line| !line.starts_with("Status:") && !line.is_empty()) {
        reply.push_str(line);
        reply.push_str("\r\n");
    }
    reply.push_str(&format!("Content-Length: {}\r\nConnection: close\r\n\r\n", payload.len()));
    stream.write_all(reply.as_bytes())?;
    stream.write_all(payload)
}

/// The commit a repository's current branch points at.
fn head_id(repository: &Repository) -> git2::Oid {
    repository.head().unwrap().peel_to_commit().unwrap().id()
}

#[test]
#[ignore = "needs git and git-http-backend installed"]
fn clone_push_fetch_and_sync_work_over_smart_http() {
    let server = serve(false);
    let workspace = tempfile::tempdir().unwrap();

    // Machine two clones what machine one pushed.
    let second = workspace.path().join("second");
    let cloned = clone(&server.url(), &second, None).expect("a clone over HTTP");
    assert_eq!(std::fs::read_to_string(second.join("main.tex")).unwrap(), "the manuscript\n");
    assert_eq!(branch_state(&cloned).unwrap().ahead_behind, Some((0, 0)));

    // It writes something and pushes, over HTTP.
    std::fs::write(second.join("main.tex"), "the manuscript\nand a new line\n").unwrap();
    stage(&cloned, "main.tex").unwrap();
    commit(&cloned, "second machine");
    push(&cloned, None).expect("a push over HTTP");
    let served = Repository::open_bare(server.bare()).unwrap();
    assert_eq!(head_id(&served), head_id(&cloned), "the server has the pushed commit");

    // A third clone starts from the pushed state, and sync reports being up to date.
    let third = workspace.path().join("third");
    let third_repository = clone(&server.url(), &third, None).unwrap();
    assert_eq!(std::fs::read_to_string(third.join("main.tex")).unwrap(), "the manuscript\nand a new line\n");
    assert_eq!(sync(&third_repository, None).unwrap(), SyncOutcome::UpToDate);

    // The second machine writes again and pushes; the third's *Sync* fast-forwards it in.
    std::fs::write(second.join("main.tex"), "the manuscript\nand a new line\nand another\n").unwrap();
    stage(&cloned, "main.tex").unwrap();
    commit(&cloned, "second machine again");
    push(&cloned, None).unwrap();
    assert_eq!(sync(&third_repository, None).unwrap(), SyncOutcome::FastForwarded { behind: 1 });
    assert_eq!(std::fs::read_to_string(third.join("main.tex")).unwrap(), "the manuscript\nand a new line\nand another\n");
}

/// The thing the credential callback must never do, now tested against a server that asks:
/// hand the GitHub token to a host that is not GitHub. The server demands a password; the clone
/// has a token to give; the clone must refuse to give it, fail, and not hang.
///
/// What this does *not* settle: the ledger's suspicion that libgit2 re-offers a *rejected* token
/// for ever. That needs a server the callback is willing to give the token to, and the callback
/// only ever gives it to `github.com`. Here the callback answers "no credential" at once, so the
/// loop could not start. A real GitHub with a revoked token is the only test of that.
#[test]
#[ignore = "needs git and git-http-backend installed"]
fn a_server_that_asks_for_a_password_never_receives_the_github_token_and_the_clone_fails() {
    let server = serve(true);
    let workspace = tempfile::tempdir().unwrap();
    let destination = workspace.path().join("private");

    // On a thread, so a hang is a failed test and not a stuck CI run.
    let (done, finished) = mpsc::channel();
    let url = server.url();
    let target = destination.clone();
    std::thread::spawn(move || {
        let outcome = clone(&url, &target, Some("gho_a_real_looking_token")).map(|_| ());
        let _ = done.send(outcome);
    });
    let outcome = finished.recv_timeout(Duration::from_secs(30)).expect("the clone hung instead of failing");

    assert!(matches!(outcome, Err(GitError::Git(_))), "{outcome:?}");
    assert!(
        server.authorization_seen.lock().unwrap().is_empty(),
        "the token was sent to a host that is not github.com: {:?}",
        server.authorization_seen.lock().unwrap()
    );
    assert!(!destination.exists(), "a failed clone left its folder behind");
}
