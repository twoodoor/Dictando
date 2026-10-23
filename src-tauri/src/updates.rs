//! Background update watcher + "new version" dot.
//!
//! Checks the updater endpoint shortly after launch and then every few hours.
//! When a newer release exists it:
//!   * puts a small red dot on the taskbar button (Windows overlay icon) /
//!     Dock badge (macOS) / launcher badge (Linux),
//!   * adds the same dot to the tray icon and an "Install Update" tray item,
//!   * emits `update-available` so the UI can show its own indicator.
//! Installing still happens from inside the app (Settings → App updates, or
//! the tray item) — nothing is downloaded until the user asks.

use std::sync::Mutex;
use std::time::Duration;

use tauri::image::Image;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::UpdaterExt;

const FIRST_CHECK_DELAY: Duration = Duration::from_secs(8);
const CHECK_INTERVAL: Duration = Duration::from_secs(4 * 60 * 60);

/// Version string of a pending update, if one was found.
#[derive(Default)]
pub struct UpdateState(pub Mutex<Option<String>>);

pub fn pending_version(app: &AppHandle) -> Option<String> {
    app.try_state::<UpdateState>()
        .and_then(|s| s.0.lock().unwrap().clone())
}

/// Start the periodic background check on its own thread.
pub fn spawn_watcher(app: AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(FIRST_CHECK_DELAY);
        loop {
            tauri::async_runtime::block_on(check_now(&app));
            std::thread::sleep(CHECK_INTERVAL);
        }
    });
}

/// Run one update check and refresh every indicator. Returns the new version.
pub async fn check_now(app: &AppHandle) -> Option<String> {
    let found = match app.updater() {
        Ok(updater) => match updater.check().await {
            Ok(Some(u)) => Some(u.version.clone()),
            Ok(None) => None,
            Err(e) => {
                log::warn!("background update check failed: {e}");
                // Keep whatever we knew before on a network error.
                return pending_version(app);
            }
        },
        Err(e) => {
            log::warn!("updater unavailable: {e}");
            return None;
        }
    };
    set_pending(app, found.clone());
    found
}

/// Record the pending version and update the taskbar/tray/UI indicators.
pub fn set_pending(app: &AppHandle, version: Option<String>) {
    let changed = {
        let state = app.state::<UpdateState>();
        let mut cur = state.0.lock().unwrap();
        let changed = *cur != version;
        *cur = version.clone();
        changed
    };
    apply_indicators(app);
    if changed {
        match &version {
            Some(v) => log::info!("update available: v{v}"),
            None => log::info!("no update available"),
        }
        let _ = app.emit("update-available", version);
        crate::rebuild_tray_menu(app);
    }
}

/// (Re)apply the dot. Called after a check and whenever the main window is
/// shown, because Windows drops overlay icons when a taskbar button is
/// recreated.
pub fn apply_indicators(app: &AppHandle) {
    let version = pending_version(app);
    let on = version.is_some();

    if let Some(w) = app.get_webview_window("main") {
        #[cfg(target_os = "windows")]
        {
            let _ = w.set_overlay_icon(if on { Some(dot_icon(32)) } else { None });
        }
        #[cfg(target_os = "macos")]
        {
            let _ = w.set_badge_label(if on { Some("•".to_string()) } else { None });
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            let _ = w.set_badge_count(if on { Some(1) } else { None });
        }
        let _ = &w;
    }

    if let Some(tray) = app.tray_by_id("main-tray") {
        if let Some(base) = app.default_window_icon() {
            let icon = if on { with_dot(base) } else { base.clone() };
            let _ = tray.set_icon(Some(icon));
        }
        let tip = match &version {
            Some(v) => format!("Mumblr — update v{v} available"),
            None => "Mumblr".to_string(),
        };
        let _ = tray.set_tooltip(Some(tip));
    }
}

// ---------------------------------------------------------------------------
// Dot rendering (no image assets — drawn into an RGBA buffer)
// ---------------------------------------------------------------------------

const DOT: [u8; 3] = [0xEF, 0x44, 0x44]; // red-500
const RING: [u8; 3] = [0xFF, 0xFF, 0xFF];

/// Blend an anti-aliased dot (white ring + red fill) into `rgba`.
fn paint_dot(rgba: &mut [u8], w: u32, h: u32, cx: f32, cy: f32, r: f32, ring: f32) {
    for y in 0..h {
        for x in 0..w {
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            let d = (dx * dx + dy * dy).sqrt();
            let outer = (r - d + 0.5).clamp(0.0, 1.0);
            if outer <= 0.0 {
                continue;
            }
            let inner = (r - ring - d + 0.5).clamp(0.0, 1.0);
            let i = ((y * w + x) * 4) as usize;
            // Colour: red inside, white ring outside.
            let col: [f32; 3] = std::array::from_fn(|k| {
                DOT[k] as f32 * inner + RING[k] as f32 * (1.0 - inner)
            });
            let a_src = outer;
            let a_dst = rgba[i + 3] as f32 / 255.0;
            let a_out = a_src + a_dst * (1.0 - a_src);
            for k in 0..3 {
                let c = (col[k] * a_src + rgba[i + k] as f32 * a_dst * (1.0 - a_src))
                    / a_out.max(1e-6);
                rgba[i + k] = c.round().clamp(0.0, 255.0) as u8;
            }
            rgba[i + 3] = (a_out * 255.0).round() as u8;
        }
    }
}

/// A standalone dot for the Windows taskbar overlay slot (bottom-right corner
/// of the button). Kept smaller than the slot so it reads as a "dot".
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn dot_icon(size: u32) -> Image<'static> {
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    let s = size as f32;
    paint_dot(&mut rgba, size, size, s * 0.62, s * 0.62, s * 0.36, s * 0.07);
    Image::new_owned(rgba, size, size)
}

/// The app icon with a dot in its top-right corner (tray).
fn with_dot(base: &Image<'_>) -> Image<'static> {
    let (w, h) = (base.width(), base.height());
    let mut rgba = base.rgba().to_vec();
    let s = w.min(h) as f32;
    let r = s * 0.22;
    paint_dot(&mut rgba, w, h, w as f32 - r - s * 0.02, r + s * 0.02, r, s * 0.05);
    Image::new_owned(rgba, w, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dot_is_red_in_centre_and_clear_in_corner() {
        let img = dot_icon(32);
        let px = |x: u32, y: u32| {
            let i = ((y * 32 + x) * 4) as usize;
            img.rgba()[i..i + 4].to_vec()
        };
        assert_eq!(px(20, 20), vec![0xEF, 0x44, 0x44, 255]);
        assert_eq!(px(0, 0)[3], 0);
    }
}
