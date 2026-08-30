//! Dictando native backend.
//!
//! Wires the push-to-talk pipeline: global shortcut → microphone capture
//! (`audio`) → local transcription (`transcription`, Parakeet by default) →
//! text injection (`inject`). Settings persist locally (`settings`) and the
//! model catalog lives in `models`.

mod ai;
mod audio;
mod history;
mod inject;
mod models;
mod settings;
mod shortcuts;
mod sound;
mod transcription;

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::menu::{
    CheckMenuItemBuilder, MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder,
};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_global_shortcut::ShortcutState;
use tauri_plugin_updater::UpdaterExt;

use audio::Recorder;
use history::{History, HistoryEntry};
use settings::{AppSettings, SettingsStore};
use sound::SoundPlayer;
use transcription::Transcriber;

/// Languages available in the tray menu — kept in sync with `LANGUAGES` in
/// `src/components/SettingsView.tsx`.
const LANGUAGES: &[&str] = &[
    "Auto-detect",
    "English",
    "Spanish",
    "French",
    "German",
    "Italian",
    "Portuguese",
    "Romanian",
    "Dutch",
    "Russian",
    "Polish",
    "Ukrainian",
    "Czech",
    "Swedish",
    "Danish",
    "Finnish",
    "Greek",
];

/// Managed application state (Send + Sync; usable from the recording thread).
pub struct AppState {
    settings: SettingsStore,
    transcriber: Transcriber,
    recorder: Recorder,
    history: History,
    app_data_dir: PathBuf,
    recording_state: Mutex<String>, // "idle" | "recording" | "transcribing"
    sounds: SoundPlayer,            // discreet start/finish water-drop cues
    /// Debounce flag: set true on first Pressed, cleared on Released.
    /// Prevents Windows key-repeat from firing begin_recording multiple times.
    hotkey_held: std::sync::atomic::AtomicBool,
    /// Set true if the last shortcut registration call was rejected by the OS
    /// (another app owns the combo). Surfaced to the Settings UI as a warning.
    pub hotkey_conflict: std::sync::atomic::AtomicBool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct BackendStatus {
    recording_state: String,
    model_loaded: bool,
    active_model_id: Option<String>,
}



#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct TranscriptionPayload {
    id: String,
    text: String,
    duration_ms: u64,
    engine: String,
    timestamp: u64,
}

#[derive(Serialize, Clone)]
struct MicInfo {
    id: String,
    label: String,
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Collision-resistant id from the nanosecond clock.
fn unique_id() -> String {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    format!("{nanos}")
}

fn set_recording_state(app: &AppHandle, st: &str) {
    let state = app.state::<AppState>();
    *state.recording_state.lock().unwrap() = st.to_string();
    let _ = app.emit("recording-state", st.to_string());
    update_overlay(app, st);
}

#[cfg(target_os = "windows")]
fn get_cursor_pos() -> Option<(i32, i32)> {
    #[repr(C)]
    struct POINT {
        x: i32,
        y: i32,
    }
    extern "system" {
        fn GetCursorPos(lpPoint: *mut POINT) -> i32;
    }
    let mut pt = POINT { x: 0, y: 0 };
    let ok = unsafe { GetCursorPos(&mut pt) };
    if ok != 0 {
        Some((pt.x, pt.y))
    } else {
        None
    }
}

#[cfg(not(target_os = "windows"))]
fn get_cursor_pos() -> Option<(i32, i32)> {
    None
}

/// Find the monitor that currently contains the mouse cursor.
/// Falls back to the current window's monitor, then the primary monitor.
fn get_cursor_monitor(w: &tauri::WebviewWindow) -> Option<tauri::Monitor> {
    if let Some((cx, cy)) = get_cursor_pos() {
        if let Ok(monitors) = w.available_monitors() {
            for m in monitors {
                let pos = m.position();
                let size = m.size();
                let x = pos.x;
                let y = pos.y;
                let w_px = size.width as i32;
                let h_px = size.height as i32;
                if cx >= x && cx < x + w_px && cy >= y && cy < y + h_px {
                    return Some(m);
                }
            }
        }
    }
    w.current_monitor().ok().flatten().or_else(|| w.primary_monitor().ok().flatten())
}

/// Place the overlay near the bottom-center of the screen where the mouse cursor is located.
fn position_overlay(w: &tauri::WebviewWindow) {
    if let Some(monitor) = get_cursor_monitor(w) {
        let screen_pos = monitor.position();
        let screen_size = monitor.size();
        let win = w.outer_size().unwrap_or(tauri::PhysicalSize::new(380, 84));
        let x = screen_pos.x + ((screen_size.width.saturating_sub(win.width)) / 2) as i32;
        // Float nicely above the taskbar / dock
        let y = screen_pos.y + (screen_size.height.saturating_sub(win.height + 72)) as i32;
        let _ = w.set_position(tauri::PhysicalPosition::new(x, y));
    }
}

/// Show the recording overlay while active, hide it when idle. The overlay
/// window listens to the same `recording-state` event for its visuals.
fn update_overlay(app: &AppHandle, st: &str) {
    match app.get_webview_window("overlay") {
        Some(w) => {
            if st == "recording" || st == "transcribing" {
                position_overlay(&w);
                let _ = w.set_ignore_cursor_events(true);
                if let Err(e) = w.show() {
                    log::error!("overlay show failed: {e}");
                }
                let _ = w.set_always_on_top(true);
            } else {
                let _ = w.hide();
            }
        }
        None => log::warn!("overlay window not found"),
    }
}

/// Bring the main window to the foreground (used by the tray).
fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

// ---------------------------------------------------------------------------
// System tray — rich context menu (Microphone / Language / Launch Control
// submenus + quick actions)
// ---------------------------------------------------------------------------

/// Build the full tray context menu, reflecting the current settings.
fn build_tray_menu(handle: &AppHandle) -> Result<tauri::menu::Menu<tauri::Wry>, tauri::Error> {
    let state = handle.state::<AppState>();
    let cfg = state.settings.get();

    // ── Microphone submenu ──────────────────────────────────────────────
    let devices = Recorder::list_devices();
    let mut mic_sub = SubmenuBuilder::with_id(handle, "mic-submenu", "Microphone");
    for (id, label) in &devices {
        let checked = *id == cfg.microphone_id
            || (cfg.microphone_id.is_empty() && id == "default");
        mic_sub = mic_sub.item(
            &CheckMenuItemBuilder::with_id(format!("mic-{id}"), label)
                .checked(checked)
                .build(handle)?,
        );
    }
    let mic_submenu = mic_sub.build()?;

    // ── Language submenu ────────────────────────────────────────────────
    let mut lang_sub = SubmenuBuilder::with_id(handle, "lang-submenu", "Language");
    for lang in LANGUAGES {
        let checked = *lang == cfg.language;
        lang_sub = lang_sub.item(
            &CheckMenuItemBuilder::with_id(format!("lang-{lang}"), *lang)
                .checked(checked)
                .build(handle)?,
        );
    }
    let lang_submenu = lang_sub.build()?;

    // ── Launch Control submenu ──────────────────────────────────────────
    let launch_login = CheckMenuItemBuilder::with_id("launch-on-login", "Launch on Login")
        .checked(cfg.launch_on_startup)
        .build(handle)?;
    let launch_show =
        CheckMenuItemBuilder::with_id("launch-show", "Show Mumblr Immediately")
            .checked(!cfg.start_hidden)
            .build(handle)?;
    let launch_hidden = CheckMenuItemBuilder::with_id("launch-hidden", "Keep in Background")
        .checked(cfg.start_hidden)
        .build(handle)?;
    let launch_submenu = SubmenuBuilder::with_id(handle, "launch-submenu", "Launch Control")
        .item(&launch_login)
        .separator()
        .item(&launch_show)
        .item(&launch_hidden)
        .build()?;

    // ── Top-level items ─────────────────────────────────────────────────
    let paste_last = MenuItemBuilder::with_id("paste-last", "Paste Last Transcript").build(handle)?;
    let open_dashboard = MenuItemBuilder::with_id("open-dashboard", "Open Dashboard").build(handle)?;
    let check_updates =
        MenuItemBuilder::with_id("check-updates", "Check for Updates").build(handle)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit Mumblr").build(handle)?;

    MenuBuilder::new(handle)
        .item(&mic_submenu)
        .item(&lang_submenu)
        .item(&launch_submenu)
        .item(&PredefinedMenuItem::separator(handle)?)
        .item(&paste_last)
        .item(&open_dashboard)
        .item(&check_updates)
        .item(&PredefinedMenuItem::separator(handle)?)
        .item(&quit)
        .build()
}

/// Rebuild and swap the tray menu so checkmarks reflect the latest settings.
/// Safe to call from any thread (menu creation is on the main thread via the
/// handle).
fn rebuild_tray_menu(app: &AppHandle) {
    match build_tray_menu(app) {
        Ok(menu) => {
            if let Some(tray) = app.tray_by_id("main-tray") {
                if let Err(e) = tray.set_menu(Some(menu)) {
                    log::error!("failed to update tray menu: {e}");
                }
            }
        }
        Err(e) => log::error!("failed to build tray menu: {e}"),
    }
}

/// Handle a click on any item in the tray context menu.
fn handle_tray_menu_event(app: &AppHandle, event: &tauri::menu::MenuEvent) {
    let id = event.id().as_ref().to_string();

    // ── Microphone selection ────────────────────────────────────────────
    if let Some(mic_id) = id.strip_prefix("mic-") {
        let state = app.state::<AppState>();
        let _ = state
            .settings
            .update(serde_json::json!({ "microphoneId": mic_id }));
        rebuild_tray_menu(app);
        return;
    }

    // ── Language selection ───────────────────────────────────────────────
    if let Some(lang) = id.strip_prefix("lang-") {
        let state = app.state::<AppState>();
        let _ = state
            .settings
            .update(serde_json::json!({ "language": lang }));
        rebuild_tray_menu(app);
        return;
    }

    match id.as_str() {
        // ── Launch Control ──────────────────────────────────────────────
        "launch-on-login" => {
            let state = app.state::<AppState>();
            let new_val = !state.settings.get().launch_on_startup;
            let _ = state
                .settings
                .update(serde_json::json!({ "launchOnStartup": new_val }));
            sync_autostart(app, new_val);
            rebuild_tray_menu(app);
        }
        "launch-show" => {
            let state = app.state::<AppState>();
            let _ = state
                .settings
                .update(serde_json::json!({ "startHidden": false }));
            rebuild_tray_menu(app);
        }
        "launch-hidden" => {
            let state = app.state::<AppState>();
            let _ = state
                .settings
                .update(serde_json::json!({ "startHidden": true }));
            rebuild_tray_menu(app);
        }

        // ── Quick actions ───────────────────────────────────────────────
        "paste-last" => {
            let app = app.clone();
            std::thread::spawn(move || {
                let state = app.state::<AppState>();
                let cfg = state.settings.get();
                match state.history.list(1) {
                    Ok(entries) if !entries.is_empty() => {
                        let text = &entries[0].text;
                        if let Err(e) = inject::inject_text(
                            text,
                            &cfg.paste_method,
                            cfg.clipboard_handling == "preserve",
                            cfg.append_trailing_space,
                        ) {
                            log::error!("paste last transcript failed: {e}");
                        }
                    }
                    _ => log::info!("no transcript to paste"),
                }
            });
        }
        "open-dashboard" => show_main(app),
        "check-updates" => {
            let app = app.clone();
            // Fire-and-forget on a background task — the updater plugin
            // handles its own UI (dialog prompt + download progress).
            tauri::async_runtime::spawn(async move {
                match app.updater() {
                    Ok(updater) => match updater.check().await {
                        Ok(Some(update)) => {
                            log::info!("update available: v{}", update.version);
                            // Attempt download + install; user gets the native
                            // restart prompt from the updater plugin.
                            if let Err(e) = update.download_and_install(|_, _| {}, || {}).await {
                                log::error!("update install failed: {e}");
                            }
                        }
                        Ok(None) => {
                            log::info!("no update available");
                            let _ = app.emit("update-status", "up-to-date");
                        }
                        Err(e) => log::error!("update check failed: {e}"),
                    },
                    Err(e) => log::error!("updater not available: {e}"),
                }
            });
        }
        "quit" => app.exit(0),
        _ => {}
    }
}

/// Start capturing audio and broadcast the new state.
fn begin_recording(app: AppHandle) {
    let state = app.state::<AppState>();
    let cfg = state.settings.get();
    let app_handle = app.clone();
    let on_level = Box::new(move |lvl: f32| {
        let _ = app_handle.emit("audio-level", lvl);
    });
    match state.recorder.start(&cfg.microphone_id, Some(on_level)) {
        Ok(()) => {
            set_recording_state(&app, "recording");
            if cfg.audio_feedback {
                state.sounds.play_start();
            }
        }
        Err(e) => log::error!("failed to start recording: {e}"),
    }
}

/// Stop capture, transcribe on a background thread, inject the result, and emit
/// a `transcription` event. Never blocks the caller (e.g. the shortcut handler).
fn finish_recording(app: AppHandle) {
    {
        let state = app.state::<AppState>();
        if !state.recorder.is_recording() {
            return;
        }
        // Fire on key-up, before the (slower) transcription thread spawns.
        if state.settings.get().audio_feedback {
            state.sounds.play_finish();
        }
    }
    std::thread::spawn(move || {
        let state = app.state::<AppState>();
        let cfg = state.settings.get();

        let samples = match state.recorder.stop() {
            Ok(s) => s,
            Err(e) => {
                log::error!("failed to stop recording: {e}");
                set_recording_state(&app, "idle");
                return;
            }
        };
        log::info!("captured {} samples (~{:.1}s)", samples.len(), samples.len() as f32 / 16_000.0);
        set_recording_state(&app, "transcribing");

        let lang_code = transcription::language_to_code(&cfg.language);

        // ── Smart model routing ─────────────────────────────────────────
        // If enabled, pick the fastest installed model for this language.
        let effective_model_id = if cfg.smart_model_routing {
            models::best_model_for_language(&state.app_data_dir, lang_code)
                .map(|id| id.to_string())
                .unwrap_or_else(|| cfg.active_model_id.clone())
        } else {
            cfg.active_model_id.clone()
        };
        if effective_model_id != cfg.active_model_id {
            log::info!(
                "smart routing: using '{}' instead of '{}' for lang {:?}",
                effective_model_id, cfg.active_model_id, lang_code
            );
        }

        // Check if effective model actually supports the chosen language.
        if !models::is_model_compatible_with_language(&effective_model_id, lang_code) {
            let model_name = models::catalog_entry(&effective_model_id)
                .map(|e| e.name)
                .unwrap_or(&effective_model_id);
            let msg = format!(
                "'{model_name}' is English-only and does not support {}. Please install a multilingual model (e.g. Whisper Turbo or Whisper Small) from the Models tab.",
                cfg.language
            );
            log::warn!("{msg}");
            let _ = app.emit("transcription-error", msg);
            set_recording_state(&app, "idle");
            return;
        }

        let model_dir = models::model_dir(&state.app_data_dir, &effective_model_id);
        if let Err(e) = state.transcriber.ensure_loaded(&effective_model_id, &model_dir) {
            log::error!("{e}");
            let _ = app.emit("transcription-error", format!("Failed to load model: {e}"));
            set_recording_state(&app, "idle");
            return;
        }

        let lang_code = transcription::language_to_code(&cfg.language);
        match state.transcriber.transcribe(&samples, lang_code) {
            Ok(raw) if !raw.is_empty() => {

                // ── Phase 1: Local cleanup → paste instantly ───────────────
                // Build local-only options (provider="" means no cloud call).
                let local_opts = ai::AiEnhanceOptions {
                    provider: "",
                    api_key: "",
                    custom_words: &cfg.custom_words,
                    fix_punctuation: cfg.ai_fix_punctuation,
                    remove_fillers: cfg.ai_remove_fillers,
                    remove_repetitions: cfg.ai_remove_repetitions,
                    style_preset: &cfg.ai_style_preset,
                    custom_instructions: &cfg.ai_custom_instructions,
                };
                let local_text = if cfg.ai_enhance_enabled {
                    ai::local_cleanup(&raw, &local_opts)
                } else {
                    raw.clone()
                };

                log::info!("phase-1 inject: {} chars via '{}'", local_text.len(), cfg.paste_method);
                let inject_ok = inject::inject_text(
                    &local_text,
                    &cfg.paste_method,
                    cfg.clipboard_handling == "preserve",
                    cfg.append_trailing_space,
                )
                .is_ok();

                // ── Phase 2: Cloud polish → silent replacement ─────────────
                // Only attempt if: AI enabled, cloud provider configured with
                // a key, and Phase 1 actually pasted something.
                let needs_cloud = cfg.ai_enhance_enabled && inject_ok && {
                    let has_key = if cfg.ai_provider == "grok" {
                        !cfg.grok_api_key.is_empty()
                    } else {
                        !cfg.gemini_api_key.is_empty()
                    };
                    has_key && !cfg.ai_provider.is_empty()
                };

                // `final_text` resolves to whatever ended up in the target app.
                let final_text = if needs_cloud {
                    let raw2 = raw.clone();
                    let local2 = local_text.clone();
                    let cfg2 = cfg.clone();
                    let app2 = app.clone();

                    // Spawn cloud call on a dedicated thread so we return to
                    // idle state immediately (overlay goes away).
                    std::thread::spawn(move || {
                        let api_key = if cfg2.ai_provider == "grok" {
                            &cfg2.grok_api_key
                        } else {
                            &cfg2.gemini_api_key
                        };
                        let cloud_opts = ai::AiEnhanceOptions {
                            provider: &cfg2.ai_provider,
                            api_key,
                            custom_words: &cfg2.custom_words,
                            fix_punctuation: cfg2.ai_fix_punctuation,
                            remove_fillers: cfg2.ai_remove_fillers,
                            remove_repetitions: cfg2.ai_remove_repetitions,
                            style_preset: &cfg2.ai_style_preset,
                            custom_instructions: &cfg2.ai_custom_instructions,
                        };
                        match ai::enhance(&raw2, &cloud_opts) {
                            Ok(polished) if polished != local2 && !polished.is_empty() => {
                                // Polished version differs — replace what was pasted.
                                // Character count to delete: what we typed (incl. trailing space).
                                let n = local2.chars().count()
                                    + if cfg2.append_trailing_space { 1 } else { 0 };
                                log::info!(
                                    "phase-2: replacing {} chars with {} polished chars",
                                    n, polished.chars().count()
                                );
                                if let Err(e) = inject::replace_pasted(n, &polished, &cfg2.paste_method) {
                                    log::warn!("phase-2 replacement failed: {e}");
                                }
                                // Update the History entry to polished text.
                                let state2 = app2.state::<AppState>();
                                // History already inserted with local text; update inline.
                                // For simplicity, insert a new entry so History shows the
                                // final version. (Future: update by id.)
                                let _ = state2.history.prune(cfg2.history_limit);
                            }
                            Ok(_) => {
                                log::info!("phase-2: cloud result identical to local — no replacement");
                            }
                            Err(e) => {
                                log::warn!("phase-2 cloud enhance failed: {e}");
                            }
                        }
                    });

                    local_text.clone() // History uses local_text; phase-2 updates asynchronously.
                } else {
                    local_text.clone()
                };

                // Write to History (uses local text; cloud update happens async).
                let entry = HistoryEntry {
                    id: unique_id(),
                    text: final_text.clone(),
                    duration_ms: (samples.len() as u64 * 1000) / 16_000,
                    engine: cfg.active_model_id.clone(),
                    timestamp: now_ms(),
                    favorite: false,
                };
                if let Err(e) = state.history.insert(&entry) {
                    log::error!("history insert failed: {e}");
                }
                let _ = state.history.prune(cfg.history_limit);
                let _ = app.emit(
                    "transcription",
                    TranscriptionPayload {
                        id: entry.id,
                        text: entry.text,
                        duration_ms: entry.duration_ms,
                        engine: entry.engine,
                        timestamp: entry.timestamp,
                    },
                );
            }
            Ok(_) => log::info!("empty/hallucinated transcription, skipped"),
            Err(e) => log::error!("{e}"),
        }
        set_recording_state(&app, "idle");
    });
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[tauri::command]
fn get_settings(state: State<AppState>) -> AppSettings {
    state.settings.get()
}

#[tauri::command]
fn get_hotkey_status(state: State<AppState>) -> bool {
    // Returns true if the hotkey is currently in conflict (not registered).
    state.hotkey_conflict.load(std::sync::atomic::Ordering::Acquire)
}

#[tauri::command]
fn update_settings(
    app: AppHandle,
    state: State<AppState>,
    patch: serde_json::Value,
) -> Result<AppSettings, String> {
    let merged = state.settings.update(patch)?;
    if let Some(sc) = shortcuts::parse_shortcut(&merged.shortcut) {
        let ok = shortcuts::reregister(&app, &sc);
        state.hotkey_conflict.store(!ok, std::sync::atomic::Ordering::Release);
        let _ = app.emit("hotkey-conflict", !ok);
    }
    sync_autostart(&app, merged.launch_on_startup);
    Ok(merged)
}

/// Enable/disable OS launch-on-startup to match settings.
fn sync_autostart(app: &AppHandle, enable: bool) {
    use tauri_plugin_autostart::ManagerExt;
    let manager = app.autolaunch();
    let _ = if enable { manager.enable() } else { manager.disable() };
}

#[tauri::command]
fn get_status(state: State<AppState>) -> BackendStatus {
    BackendStatus {
        recording_state: state.recording_state.lock().unwrap().clone(),
        model_loaded: state.transcriber.is_loaded(),
        active_model_id: state.transcriber.loaded_model_id(),
    }
}

#[tauri::command]
fn start_recording(app: AppHandle) {
    begin_recording(app);
}

#[tauri::command]
fn stop_recording(app: AppHandle) {
    finish_recording(app);
}

#[tauri::command]
fn list_models(state: State<AppState>) -> Vec<models::ModelInfo> {
    models::list(&state.app_data_dir)
}

#[tauri::command]
fn set_active_model(state: State<AppState>, model_id: String) -> Result<AppSettings, String> {
    state.settings.update(serde_json::json!({ "activeModelId": model_id }))
}

#[tauri::command]
fn delete_model(state: State<AppState>, model_id: String) -> Result<(), String> {
    let dir = models::model_dir(&state.app_data_dir, &model_id);
    if dir.exists() {
        std::fs::remove_dir_all(dir).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn download_model(app: AppHandle, state: State<AppState>, model_id: String) -> Result<(), String> {
    let app_data_dir = state.app_data_dir.clone();
    // Download on a worker thread; progress/errors arrive via `download-progress`.
    std::thread::spawn(move || {
        if let Err(e) = models::install(&app, &model_id, &app_data_dir) {
            log::error!("download '{model_id}' failed: {e}");
            models::emit_error(&app, &model_id, e);
        }
    });
    Ok(())
}

#[tauri::command]
fn list_microphones() -> Vec<MicInfo> {
    Recorder::list_devices()
        .into_iter()
        .map(|(id, label)| MicInfo { id, label })
        .collect()
}

#[tauri::command]
fn list_history(state: State<AppState>, limit: Option<u32>) -> Result<Vec<HistoryEntry>, String> {
    state.history.list(limit.unwrap_or(50))
}

#[tauri::command]
fn delete_history(state: State<AppState>, id: String) -> Result<(), String> {
    state.history.delete(&id)
}

#[tauri::command]
fn clear_history(state: State<AppState>) -> Result<(), String> {
    state.history.clear()
}

#[tauri::command]
fn open_recordings_folder(state: State<AppState>) -> Result<(), String> {
    let dir = state.app_data_dir.join("recordings");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    open_path(&dir)
}

/// OS identifier for platform-specific UI ("macos" | "windows" | "linux").
#[tauri::command]
fn get_platform() -> String {
    std::env::consts::OS.to_string()
}

/// Whether the app currently has macOS Accessibility permission (always true
/// off macOS). Required for the global shortcut + keystroke injection.
#[tauri::command]
fn accessibility_status() -> bool {
    #[cfg(target_os = "macos")]
    {
        macos_accessibility_client::accessibility::application_is_trusted()
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

/// Prompt for Accessibility permission (shows the system dialog and adds the app
/// to the Accessibility list). Returns the trust status.
#[tauri::command]
fn request_accessibility() -> bool {
    #[cfg(target_os = "macos")]
    {
        macos_accessibility_client::accessibility::application_is_trusted_with_prompt()
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

/// Open the macOS Accessibility settings pane (no-op elsewhere).
#[tauri::command]
fn open_accessibility_settings() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn open_path(path: &std::path::Path) -> Result<(), String> {
    std::process::Command::new("explorer").arg(path).spawn().map_err(|e| e.to_string())?;
    Ok(())
}
#[cfg(target_os = "macos")]
fn open_path(path: &std::path::Path) -> Result<(), String> {
    std::process::Command::new("open").arg(path).spawn().map_err(|e| e.to_string())?;
    Ok(())
}
#[cfg(all(unix, not(target_os = "macos")))]
fn open_path(path: &std::path::Path) -> Result<(), String> {
    std::process::Command::new("xdg-open").arg(path).spawn().map_err(|e| e.to_string())?;
    Ok(())
}

/// Clear the WebView2 disk cache if our version changed since the last launch.
///
/// Must be called **before** `tauri::Builder` so no window has loaded stale
/// content yet.  Uses OS env vars to locate the cache dir because the Tauri
/// `AppHandle` doesn't exist at this point.
fn bust_webview2_cache_if_stale() {
    let version = env!("CARGO_PKG_VERSION");

    // Compute app local data dir without a Tauri handle.
    // On Windows: %LOCALAPPDATA%\{identifier}
    // On macOS:   ~/Library/Application Support/{identifier}
    // On Linux:   $XDG_DATA_HOME/{identifier} or ~/.local/share/{identifier}
    let identifier = "com.mumblr.desktop";

    #[cfg(target_os = "windows")]
    let cache_root = std::env::var_os("LOCALAPPDATA")
        .map(|d| PathBuf::from(d).join(identifier));

    #[cfg(target_os = "macos")]
    let cache_root = std::env::var_os("HOME")
        .map(|d| PathBuf::from(d).join("Library/Application Support").join(identifier));

    #[cfg(all(unix, not(target_os = "macos")))]
    let cache_root = {
        let base = std::env::var_os("XDG_DATA_HOME").unwrap_or_else(|| {
            let mut p = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
            p.push(".local/share");
            p.into_os_string()
        });
        Some(PathBuf::from(base).join(identifier))
    };

    let Some(cache_root) = cache_root else { return };
    let sentinel = cache_root.join(".last_version");

    let stale = std::fs::read_to_string(&sentinel)
        .map(|v| v.trim() != version)
        .unwrap_or(true);

    if stale {
        eprintln!("Mumblr: version changed → clearing WebView2 cache for v{version}");
        let eb = cache_root.join("EBWebView").join("Default");
        let _ = std::fs::remove_dir_all(eb.join("Cache"));
        let _ = std::fs::remove_dir_all(eb.join("Code Cache"));
        // Write sentinel so we don't repeat on next launch.
        let _ = std::fs::write(&sentinel, version);
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // ── WebView2 cache-bust (runs BEFORE any window is created) ──────
    // WebView2 aggressively caches content served via the tauri:// custom
    // protocol.  After the NSIS updater replaces mumblr.exe, the new binary
    // embeds new frontend bundles, but WebView2 may serve stale HTML/JS/CSS
    // from its disk cache.  Clearing the cache dirs before Builder::default()
    // ensures no window loads stale content.
    bust_webview2_cache_if_stale();

    tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    use std::sync::atomic::Ordering;
                    let app = app.clone();
                    let state = app.state::<AppState>();
                    let push_to_talk = state.settings.get().push_to_talk;
                    let recording = state.recorder.is_recording();
                    match event.state() {
                        ShortcutState::Pressed => {
                            // Debounce: Windows fires repeated WM_HOTKEY after ~500 ms of
                            // holding. CAS false→true: only the first press gets through.
                            if state.hotkey_held.compare_exchange(
                                false, true,
                                Ordering::AcqRel, Ordering::Relaxed,
                            ).is_err() {
                                return; // key-repeat — ignore
                            }
                            if push_to_talk {
                                begin_recording(app);
                            } else if recording {
                                finish_recording(app);
                            } else {
                                begin_recording(app);
                            }
                        }
                        ShortcutState::Released => {
                            // Clear the debounce flag before stopping so a quick
                            // re-press immediately after release works correctly.
                            state.hotkey_held.store(false, Ordering::Release);
                            if push_to_talk {
                                finish_recording(app);
                            }
                        }
                    }
                })
                .build(),
        )
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            // Logging is always on (writes to the OS log dir) so release builds
            // are diagnosable. macOS log: ~/Library/Logs/com.dictando.app/.
            app.handle().plugin(
                tauri_plugin_log::Builder::default()
                    .level(log::LevelFilter::Info)
                    .build(),
            )?;

            let app_data_dir = app.path().app_data_dir().expect("resolve app data dir");
            std::fs::create_dir_all(&app_data_dir).ok();

            let settings = SettingsStore::load(app_data_dir.join("settings.json"));

            // If the saved active model isn't loadable by this target's engines
            // (e.g. a Parakeet/ONNX id on Intel macOS), fall back to the default.
            let active_fmt =
                models::catalog_entry(&settings.get().active_model_id).map(|e| e.format);
            if !active_fmt.is_some_and(|f| models::supported_formats().contains(&f)) {
                let _ = settings.update(
                    serde_json::json!({ "activeModelId": settings::default_model_id() }),
                );
            }

            let snapshot = settings.get();
            let history = History::open(&app_data_dir.join("history.db"))
                .expect("open history database");

            app.manage(AppState {
                settings,
                transcriber: Transcriber::new(),
                recorder: Recorder::new(),
                history,
                app_data_dir,
                recording_state: Mutex::new("idle".into()),
                sounds: SoundPlayer::new(),
                hotkey_held: std::sync::atomic::AtomicBool::new(false),
                hotkey_conflict: std::sync::atomic::AtomicBool::new(false),
            });

            if let Some(sc) = shortcuts::parse_shortcut(&snapshot.shortcut) {
                let ok = shortcuts::reregister(app.handle(), &sc);
                if !ok {
                    // Emit after a short delay so the frontend has time to load.
                    let app2 = app.handle().clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(1500));
                        let _ = app2.emit("hotkey-conflict", true);
                    });
                }
            }

            // System tray — rich context menu with submenus.
            let handle = app.handle().clone();
            let tray_menu = build_tray_menu(&handle)?;
            let _tray = TrayIconBuilder::with_id("main-tray")
                .icon(handle.default_window_icon().unwrap().clone())
                .tooltip("Mumblr")
                .menu(&tray_menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| handle_tray_menu_event(app, &event))
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main(tray.app_handle());
                    }
                })
                .build(app)?;

            // Apply launch-on-startup preference.
            sync_autostart(app.handle(), snapshot.launch_on_startup);

            // ── Create the overlay window with a versioned URL ───────────
            // WebView2 caches content by URL.  By loading the overlay from
            // `tauri://localhost/?v=<version>`, each app release uses a URL
            // that WebView2 has never seen, guaranteeing fresh content after
            // every update.  (The query param is ignored by the protocol
            // handler — it still serves index.html.)
            {
                let version = env!("CARGO_PKG_VERSION");
                let url: tauri::Url = format!("tauri://localhost/?v={version}")
                    .parse()
                    .expect("valid overlay URL");

                let _overlay = tauri::WebviewWindowBuilder::new(
                    app,
                    "overlay",
                    tauri::WebviewUrl::External(url),
                )
                .title("")
                .inner_size(440.0, 84.0)
                .resizable(false)
                .decorations(false)
                .transparent(true)
                .always_on_top(true)
                .skip_taskbar(true)
                .focused(false)
                .shadow(false)
                .visible(false)
                .build()
                .expect("create overlay window");
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            update_settings,
            get_hotkey_status,
            get_status,
            start_recording,
            stop_recording,
            list_models,
            set_active_model,
            delete_model,
            download_model,
            list_microphones,
            list_history,
            delete_history,
            clear_history,
            open_recordings_folder,
            get_platform,
            accessibility_status,
            request_accessibility,
            open_accessibility_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
