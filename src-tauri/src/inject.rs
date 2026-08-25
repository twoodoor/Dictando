//! Inject transcribed text into the currently focused application.
//!
//! Two strategies (selected by `paste_method` in settings):
//!   - **direct**: type the text via simulated keystrokes (no clipboard touch).
//!   - **clipboard**: place text on the clipboard and send Ctrl/Cmd+V, then
//!     optionally restore the previous clipboard contents.
//!
//! Two-phase paste support:
//!   - Phase 1: inject the local-cleaned text immediately.
//!   - Phase 2: if cloud AI produces a different result, call
//!     `replace_pasted(n_chars, new_text)` which deletes the N chars that
//!     were pasted and re-injects the polished version.

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

/// Replace the last `n_chars` that were pasted with `new_text`.
///
/// Used by two-phase paste: after the cloud AI returns a polished version,
/// this erases what was already injected and re-injects the better version.
///
/// Strategy: send `BackSpace` × n_chars to delete, then inject new_text.
/// This works in all editors/browsers regardless of clipboard state.
/// A brief pause is inserted before deleting to ensure the target window
/// has processed the first paste before we start backspacing.
pub fn replace_pasted(n_chars: usize, new_text: &str, paste_method: &str) -> Result<(), String> {
    if n_chars == 0 && new_text.is_empty() {
        return Ok(());
    }
    // Short pause: the user may have started typing after the paste. We only
    // do the replacement within a tight window (caller enforces this).
    thread::sleep(Duration::from_millis(80));

    // Delete what was pasted.
    if n_chars > 0 {
        let mut enigo = Enigo::new(&Settings::default()).map_err(|e| e.to_string())?;
        for _ in 0..n_chars {
            enigo.key(Key::Backspace, Direction::Click).map_err(|e| e.to_string())?;
        }
    }

    if !new_text.is_empty() {
        inject_text(new_text, paste_method, false, false)?;
    }
    Ok(())
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
