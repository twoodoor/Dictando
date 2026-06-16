//! Inject transcribed text into the currently focused application.
//!
//! Two strategies (selected by `paste_method` in settings):
//!   - **direct**: type the text via simulated keystrokes (no clipboard touch).
//!   - **clipboard**: place text on the clipboard and send Ctrl/Cmd+V, then
//!     optionally restore the previous clipboard contents.
//!
//! Ported from the legacy Python app's clipboard+paste path.

use std::thread;
use std::time::Duration;

use enigo::{Direction, Enigo, Key, Keyboard, Settings};

/// Inject `text` using the given paste method.
///
/// * `paste_method` — "direct" or "clipboard".
/// * `preserve_clipboard` — for the clipboard method, restore prior contents.
/// * `append_space` — append a trailing space after the text.
pub fn inject_text(
    text: &str,
    paste_method: &str,
    preserve_clipboard: bool,
    append_space: bool,
) -> Result<(), String> {
    if text.is_empty() {
        return Ok(());
    }
    let payload = if append_space {
        format!("{text} ")
    } else {
        text.to_string()
    };

    match paste_method {
        "clipboard" => paste_via_clipboard(&payload, preserve_clipboard),
        // Default to direct typing.
        _ => type_direct(&payload),
    }
}

fn type_direct(text: &str) -> Result<(), String> {
    let mut enigo = Enigo::new(&Settings::default()).map_err(|e| e.to_string())?;
    enigo.text(text).map_err(|e| e.to_string())
}

fn paste_via_clipboard(text: &str, preserve: bool) -> Result<(), String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    let previous = if preserve { clipboard.get_text().ok() } else { None };

    clipboard.set_text(text.to_string()).map_err(|e| e.to_string())?;
    // Give the OS clipboard a moment to settle before pasting.
    thread::sleep(Duration::from_millis(40));

    send_paste_shortcut()?;

    if let Some(prev) = previous {
        // Restore after the target app has had time to read the clipboard.
        thread::sleep(Duration::from_millis(120));
        let _ = clipboard.set_text(prev);
    }
    Ok(())
}

#[cfg(target_os = "macos")]
const PASTE_MODIFIER: Key = Key::Meta;
#[cfg(not(target_os = "macos"))]
const PASTE_MODIFIER: Key = Key::Control;

fn send_paste_shortcut() -> Result<(), String> {
    let mut enigo = Enigo::new(&Settings::default()).map_err(|e| e.to_string())?;
    enigo.key(PASTE_MODIFIER, Direction::Press).map_err(|e| e.to_string())?;
    enigo.key(Key::Unicode('v'), Direction::Click).map_err(|e| e.to_string())?;
    enigo.key(PASTE_MODIFIER, Direction::Release).map_err(|e| e.to_string())?;
    Ok(())
}
