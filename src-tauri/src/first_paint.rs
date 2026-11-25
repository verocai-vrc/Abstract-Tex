//! The window as it is before its page has been drawn (S17.12).
//!
//! A webview is white until its page has loaded, which to someone who works in the dark is a flash of
//! light at every start. So the window is made here instead of by Tauri from `tauri.conf.json` (where it
//! says `"create": false`): by then the colours that were chosen can be read, and the empty window and the
//! page both start out in them. It must never keep the window from opening, so what cannot be read in time
//! is simply the usual colours.

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use tauri::utils::config::Color;
use tauri::{App, Runtime, Theme, WebviewWindowBuilder};

use crate::preferences::{Colours, PreferencesFile};

/// How long a config folder may take to answer before the window opens without it.
const LIMIT: Duration = Duration::from_millis(500);

/// `--bg` of each palette in `src/app.css`; a test below compares them.
fn background(colours: Colours, system_is_dark: bool) -> Color {
    match colours {
        Colours::Match if system_is_dark => background(Colours::Dark, system_is_dark),
        Colours::Match | Colours::Light => Color(0xf7, 0xf5, 0xf0, 0xff),
        Colours::Dark => Color(0x1b, 0x1d, 0x21, 0xff),
        Colours::ContrastLight => Color(0xff, 0xff, 0xff, 0xff),
        Colours::ContrastDark => Color(0x00, 0x00, 0x00, 0xff),
    }
}

/// Run in the page before anything of its own: says which palette to use, as `look.svelte.ts` does later.
/// Nothing for `Match`, which the page can ask the system about by itself.
fn script(colours: Colours) -> Option<String> {
    if colours == Colours::Match {
        return None;
    }
    let name = serde_json::to_string(&colours).ok()?; // in quotes, so it is JavaScript as it stands
                                                      // Some webviews run this before the page has even its `<html>`; then it waits for that.
    Some(format!(
        "(() => {{
           const paint = () => {{
             if (!document.documentElement) return false;
             document.documentElement.dataset.look ??= {name};
             return true;
           }};
           if (!paint()) new MutationObserver((_, observer) => paint() && observer.disconnect()).observe(document, {{ childList: true }});
         }})()"
    ))
}

/// What was chosen, if it can be read in time. The reading is left to a thread of its own, because
/// nothing can interrupt a disk that does not answer.
fn chosen(read: impl FnOnce() -> Colours + Send + 'static) -> Colours {
    let (answer, answered) = mpsc::channel();
    thread::spawn(move || answer.send(read()));
    answered.recv_timeout(LIMIT).unwrap_or_default()
}

/// Make the window that `tauri.conf.json` describes.
pub fn open_window<R: Runtime>(app: &App<R>, preferences: &PreferencesFile) -> tauri::Result<()> {
    let preferences = preferences.clone();
    let colours = chosen(move || preferences.read().colours);
    for described in &app.config().app.windows {
        let mut builder = WebviewWindowBuilder::from_config(app, described)?;
        if colours != Colours::Match {
            builder = builder.background_color(background(colours, false));
        }
        if let Some(script) = script(colours) {
            builder = builder.initialization_script(script);
        }
        let window = builder.build()?;
        if colours == Colours::Match {
            // Only a window that exists knows what the system asks for.
            let system_is_dark = window.theme().is_ok_and(|theme| theme == Theme::Dark);
            if let Err(error) = window.set_background_color(Some(background(colours, system_is_dark))) {
                tracing::debug!(%error, "the window starts in the webview's own colour");
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;

    #[test]
    fn the_empty_window_has_the_background_the_page_will_have() {
        let css = include_str!("../../src/app.css");
        for (colours, selector) in [
            (Colours::Light, ":root {"),
            (Colours::Dark, ":root[data-look='dark'] {"),
            (Colours::ContrastLight, ":root[data-look='contrast-light'] {"),
            (Colours::ContrastDark, ":root[data-look='contrast-dark'] {"),
        ] {
            let rule = css.split(selector).nth(1).unwrap().split('}').next().unwrap();
            let Color(red, green, blue, alpha) = background(colours, false);
            let expected = format!("--bg: #{red:02x}{green:02x}{blue:02x};");
            assert!(rule.contains(&expected), "{selector} has no {expected}");
            assert_eq!(alpha, 0xff);
        }
        assert_eq!(background(Colours::Match, true), background(Colours::Dark, true));
        assert_eq!(
            background(Colours::Match, false),
            background(Colours::Light, false)
        );
    }

    #[test]
    fn the_page_starts_in_the_palette_that_was_chosen() {
        let script = script(Colours::ContrastDark).unwrap();
        assert!(script.contains(r#"dataset.look ??= "contrast-dark""#), "{script}");
        assert_eq!(super::script(Colours::Match), None);
    }

    #[test]
    fn a_disk_that_does_not_answer_does_not_keep_the_window_shut() {
        assert_eq!(chosen(|| Colours::Dark), Colours::Dark);
        let asked = Instant::now();
        let colours = chosen(|| {
            thread::sleep(Duration::from_secs(20));
            Colours::Dark
        });
        assert_eq!(colours, Colours::Match);
        assert!(asked.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn the_window_is_left_for_this_to_make() {
        let config: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert_eq!(config["app"]["windows"][0]["create"], false);
        assert!(include_str!("lib.rs").contains("first_paint::open_window("));
    }
}
