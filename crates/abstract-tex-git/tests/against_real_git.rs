//! S10.2a: what this crate reports, checked against what the `git` binary reports.
//!
//! The unit tests in `lib.rs` use libgit2 to set a repository up and libgit2 to read it back,
//! which proves the mapping is self-consistent and not that it is *right*. The author's mental
//! model is `git status`, and the Source Control view has to match it — so this walks one
//! repository through every state the card names and compares both answers, path by path and
//! letter by letter.
//!
//! `#[ignore]`d, like every test in this workspace that needs a binary we do not ship: it wants
//! `git` on `PATH`. `cargo test -p abstract-tex-git --test against_real_git -- --ignored`.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::Command;

use abstract_tex_git::{stage, status, ChangeKind, Status};

/// One side's view of a repository: the staged set and the unstaged set, each a path and its
/// letter. Named so the two readers below — ours and the `git` binary's — return the same thing.
type Lists = (BTreeSet<(String, ChangeKind)>, BTreeSet<(String, ChangeKind)>);

/// `git status --porcelain=v1`, read into the same shape this crate returns.
///
/// The format is two columns then a path: the first is what the index has to say, the second
/// what the working tree does, and `??` is the special case for a file Git has never seen. A
/// rename is `R  old -> new`.
fn porcelain(dir: &Path) -> Lists {
    let output = Command::new("git")
        .args(["status", "--porcelain=v1", "--untracked-files=all", "--renames"])
        .current_dir(dir)
        .output()
        .expect("needs git on PATH");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));

    let mut staged = BTreeSet::new();
    let mut unstaged = BTreeSet::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let (codes, rest) = line.split_at(2);
        let path = rest.trim();
        // A rename prints both names; the row is about where the file is now.
        let path = path.rsplit(" -> ").next().unwrap_or(path).trim_matches('"').to_string();
        if path.starts_with(".abstract-tex/") {
            continue; // ours, and never a change (the crate says why)
        }
        let mut codes = codes.chars();
        let index = codes.next().unwrap_or(' ');
        let worktree = codes.next().unwrap_or(' ');

        if index == '?' {
            unstaged.insert((path, ChangeKind::Untracked));
            continue;
        }
        if let Some(kind) = kind_of(index) {
            staged.insert((path.clone(), kind));
        }
        if let Some(kind) = kind_of(worktree) {
            unstaged.insert((path, kind));
        }
    }
    (staged, unstaged)
}

fn kind_of(code: char) -> Option<ChangeKind> {
    match code {
        'M' => Some(ChangeKind::Modified),
        'A' => Some(ChangeKind::Added),
        'D' => Some(ChangeKind::Deleted),
        'R' => Some(ChangeKind::Renamed),
        _ => None,
    }
}

fn ours(status: &Status) -> Lists {
    let set = |changes: &[abstract_tex_git::FileChange]| {
        changes.iter().map(|change| (change.path.clone(), change.kind)).collect::<BTreeSet<_>>()
    };
    (set(&status.staged), set(&status.unstaged))
}

fn git(dir: &Path, args: &[&str]) {
    let output = Command::new("git").args(args).current_dir(dir).output().expect("needs git on PATH");
    assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
}

#[test]
#[ignore]
fn every_state_reads_the_same_way_as_the_git_binary() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    git(dir, &["init", "-q"]);
    git(dir, &["config", "user.name", "Ada"]);
    git(dir, &["config", "user.email", "ada@example.invalid"]);

    fs::write(dir.join("main.tex"), "the manuscript\n").unwrap();
    fs::write(dir.join("both.tex"), "first version\n").unwrap();
    fs::write(dir.join("gone.tex"), "to be deleted\n").unwrap();
    fs::write(dir.join("old-name.tex"), "a chapter that will be renamed, with enough text in it that Git's rename detection has something to match on\n").unwrap();
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-qm", "first"]);

    // Every state the card names, at once, in one repository.
    fs::write(dir.join("added.tex"), "new and staged\n").unwrap();
    fs::write(dir.join("untracked.tex"), "never seen\n").unwrap();
    fs::write(dir.join("both.tex"), "staged version\n").unwrap();
    fs::remove_file(dir.join("gone.tex")).unwrap();
    fs::rename(dir.join("old-name.tex"), dir.join("new-name.tex")).unwrap();
    fs::create_dir_all(dir.join(".abstract-tex/build")).unwrap();
    fs::write(dir.join(".abstract-tex/build/main.pdf"), "junk").unwrap();

    for path in ["added.tex", "both.tex", "gone.tex", "old-name.tex", "new-name.tex"] {
        let repository = abstract_tex_git::open(dir).unwrap();
        stage(&repository, path).unwrap();
    }
    // Staged, then edited again: the file that has to be in both lists with different letters.
    fs::write(dir.join("both.tex"), "and edited again\n").unwrap();
    fs::write(dir.join("main.tex"), "edited, never staged\n").unwrap();

    let repository = abstract_tex_git::open(dir).unwrap();
    let (our_staged, our_unstaged) = ours(&status(&repository).unwrap());
    let (git_staged, git_unstaged) = porcelain(dir);

    assert_eq!(our_staged, git_staged, "staged list disagrees with `git status`");
    assert_eq!(our_unstaged, git_unstaged, "unstaged list disagrees with `git status`");

    // And the two things the comparison alone would not catch, because `git status` agrees with
    // us about them by our own filtering above.
    assert!(git_staged.iter().any(|(path, kind)| path == "new-name.tex" && *kind == ChangeKind::Renamed), "{git_staged:?}");
    assert!(
        !String::from_utf8_lossy(&Command::new("git").args(["status", "--porcelain"]).current_dir(dir).output().unwrap().stdout)
            .is_empty(),
        "the repository should not be clean; the comparison above would pass trivially"
    );
}

/// `Sync Changes ↑n ↓m` is drawn straight from these two numbers, so they are checked against
/// `git rev-list --left-right --count`, which is where `git status`'s own "ahead 2, behind 1"
/// sentence comes from. Needs a real remote, which means a real bare repository and a real clone.
#[test]
#[ignore]
fn ahead_and_behind_agree_with_git_rev_list() {
    let tmp = tempfile::tempdir().unwrap();
    let origin = tmp.path().join("origin.git");
    let ours = tmp.path().join("ours");
    let theirs = tmp.path().join("theirs");

    git(tmp.path(), &["init", "--bare", "-q", "origin.git"]);
    for clone in [&ours, &theirs] {
        git(tmp.path(), &["clone", "-q", origin.to_str().unwrap(), clone.to_str().unwrap()]);
        git(clone, &["config", "user.name", "Ada"]);
        git(clone, &["config", "user.email", "ada@example.invalid"]);
    }

    // One commit pushed, so both clones share a base and have an upstream.
    fs::write(ours.join("main.tex"), "shared base\n").unwrap();
    git(&ours, &["add", "-A"]);
    git(&ours, &["commit", "-qm", "base"]);
    // Name the branch rather than inheriting this machine's `init.defaultBranch`, which is
    // `master` here and `main` elsewhere — the test should not depend on which.
    git(&ours, &["branch", "-M", "main"]);
    git(&ours, &["push", "-q", "origin", "HEAD:refs/heads/main"]);
    git(&ours, &["branch", "--set-upstream-to=origin/main"]);
    git(&theirs, &["fetch", "-q"]);
    git(&theirs, &["checkout", "-q", "-B", "main", "origin/main"]);
    git(&theirs, &["branch", "--set-upstream-to=origin/main"]);

    // They push one commit; we make two of our own and fetch without merging. Now we are two
    // ahead and one behind — the state the Sync button exists for.
    fs::write(theirs.join("theirs.tex"), "their work\n").unwrap();
    git(&theirs, &["add", "-A"]);
    git(&theirs, &["commit", "-qm", "theirs"]);
    git(&theirs, &["push", "-q"]);

    for n in 1..=2 {
        fs::write(ours.join(format!("ours-{n}.tex")), "our work\n").unwrap();
        git(&ours, &["add", "-A"]);
        git(&ours, &["commit", "-qm", &format!("ours {n}")]);
    }
    git(&ours, &["fetch", "-q"]);

    let repository = abstract_tex_git::open(&ours).unwrap();
    let state = abstract_tex_git::branch_state(&repository).unwrap();
    assert_eq!(state.name.as_deref(), Some("main"));
    assert!(!state.unborn);

    let counts = Command::new("git")
        .args(["rev-list", "--left-right", "--count", "HEAD...@{upstream}"])
        .current_dir(&ours)
        .output()
        .expect("needs git on PATH");
    let counts = String::from_utf8_lossy(&counts.stdout);
    let mut numbers = counts.split_whitespace().map(|n| n.parse::<usize>().unwrap());
    let (ahead, behind) = (numbers.next().unwrap(), numbers.next().unwrap());

    assert_eq!(state.ahead_behind, Some((ahead, behind)), "git says {ahead} ahead, {behind} behind");
    assert_eq!(state.ahead_behind, Some((2, 1)), "and the state this test set up is 2 ahead, 1 behind");
}
