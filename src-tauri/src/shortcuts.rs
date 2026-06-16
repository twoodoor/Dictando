//! Global shortcut parsing and (re)registration.
//!
//! Settings store the shortcut as `KeyboardEvent.code` values (e.g.
//! `["ControlLeft","Space"]`), matching how the web UI captures keys. This
//! module converts that into a `tauri_plugin_global_shortcut::Shortcut`
//! (modifiers + a single key code) and (un)registers it.

use std::str::FromStr;

use tauri::AppHandle;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};

/// Convert a list of `KeyboardEvent.code` strings into a `Shortcut`.
/// Modifier codes accumulate into `Modifiers`; the last non-modifier code is
/// the trigger key. Returns `None` if no trigger key is present.
pub fn parse_shortcut(codes: &[String]) -> Option<Shortcut> {
    let mut modifiers = Modifiers::empty();
    let mut key: Option<Code> = None;

    for code in codes {
        match code.as_str() {
            "ControlLeft" | "ControlRight" => modifiers |= Modifiers::CONTROL,
            "AltLeft" | "AltRight" => modifiers |= Modifiers::ALT,
            "ShiftLeft" | "ShiftRight" => modifiers |= Modifiers::SHIFT,
            "MetaLeft" | "MetaRight" | "OSLeft" | "OSRight" => modifiers |= Modifiers::META,
            other => {
                // `Code` parses the standard W3C code names ("Space", "KeyA"…).
                if let Ok(c) = Code::from_str(other) {
                    key = Some(c);
                }
            }
        }
    }

    key.map(|k| {
        let mods = if modifiers.is_empty() { None } else { Some(modifiers) };
        Shortcut::new(mods, k)
    })
}

/// Unregister everything, then register `shortcut`. Logs and ignores errors so
/// a bad shortcut never crashes the app.
pub fn reregister(app: &AppHandle, shortcut: &Shortcut) {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    if let Err(e) = gs.register(shortcut.clone()) {
        log::error!("failed to register global shortcut: {e}");
    }
}
