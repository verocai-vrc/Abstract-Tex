//! Which opening of a document is the current one, and whether the person stopped it (S17.2).
//!
//! Opening reads a whole folder, on a disk that may never answer, so it runs on a thread of its own
//! while the window stays alive. Two things can then happen that could not before: the person presses
//! *Stop*, or a second opening starts. This module owns the one question both come down to: may this
//! opening still become the open document? It must never read a disk, and it must never wait while it
//! holds its lock.

use std::sync::{Arc, Mutex};

use tokio_util::sync::CancellationToken;

/// How far one opening has got. It moves once, from `Reading` to one of the other two, and never back.
#[derive(Clone, Copy, PartialEq)]
enum Stage {
    Reading,
    /// It is now the open document. Too late to stop.
    Adopted,
    Stopped,
}

/// One attempt to open a folder.
pub struct Opening {
    stage: Mutex<Stage>,
    /// Cancelled when the stage becomes `Stopped`, and at no other time. A `Mutex` alone can say *that*
    /// it was stopped; a `CancellationToken` can also be awaited, which is what lets the command answer
    /// at once while the stuck read is left behind on its own thread.
    stop_signal: CancellationToken,
}

impl Opening {
    fn new() -> Self {
        Self {
            stage: Mutex::new(Stage::Reading),
            stop_signal: CancellationToken::new(),
        }
    }

    /// Claim the right to become the open document. `false` means it was stopped, and whoever asked
    /// must change nothing.
    pub fn adopt(&self) -> bool {
        let mut stage = self.stage.lock().unwrap();
        if *stage == Stage::Reading {
            *stage = Stage::Adopted;
        }
        *stage == Stage::Adopted
    }

    /// Returns when this opening is stopped. Never returns for one that is adopted.
    pub async fn stopped(&self) {
        self.stop_signal.cancelled().await;
    }

    /// Whether it was stopped. For work that looks now and then, such as unpacking an archive.
    pub fn is_stopped(&self) -> bool {
        *self.stage.lock().unwrap() == Stage::Stopped
    }

    /// Whether this call is the one that stopped it.
    fn stop(&self) -> bool {
        let mut stage = self.stage.lock().unwrap();
        if *stage != Stage::Reading {
            return false;
        }
        *stage = Stage::Stopped;
        self.stop_signal.cancel();
        true
    }
}

/// The app's openings: at most one is being read at a time.
///
/// `Arc` (a reference-counted pointer) because two owners need the same opening: the thread that
/// reads the folder, and this list, through which *Stop* finds it.
#[derive(Default)]
pub struct Openings {
    current: Mutex<Option<Arc<Opening>>>,
}

impl Openings {
    /// Start an opening. The one before it, if it is still reading, is stopped: the person asked for
    /// something else since.
    pub fn begin(&self) -> Arc<Opening> {
        let opening = Arc::new(Opening::new());
        let previous = self.current.lock().unwrap().replace(Arc::clone(&opening));
        if let Some(previous) = previous {
            previous.stop();
        }
        opening
    }

    /// Stop the opening that is being read. `false` when there is none: nothing was ever opened, or
    /// the last one has already finished or been stopped.
    pub fn stop(&self) -> bool {
        match self.current.lock().unwrap().as_ref() {
            Some(opening) => opening.stop(),
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_newer_opening_stops_the_one_before_it() {
        let openings = Openings::default();
        let (first, second) = (openings.begin(), openings.begin());
        assert!(!first.adopt());
        assert!(second.adopt());
    }

    #[test]
    fn what_was_adopted_can_no_longer_be_stopped() {
        let openings = Openings::default();
        let opening = openings.begin();
        assert!(opening.adopt());
        assert!(!openings.stop());
    }

    #[test]
    fn stopping_when_nothing_is_being_opened_answers_no() {
        let openings = Openings::default();
        assert!(!openings.stop());
    }

    #[test]
    fn a_stopped_opening_is_told_so_and_is_not_adopted() {
        let openings = Openings::default();
        let opening = openings.begin();
        assert!(openings.stop());
        tauri::async_runtime::block_on(opening.stopped()); // returns, or the test never ends
        assert!(!opening.adopt());
        assert!(!openings.stop(), "it can be stopped once");
    }

    #[test]
    fn an_opening_knows_whether_it_was_stopped() {
        let openings = Openings::default();
        let (older, newer) = (openings.begin(), openings.begin());
        assert!(older.is_stopped());
        assert!(!newer.is_stopped());
        newer.adopt();
        assert!(!newer.is_stopped());
    }
}
