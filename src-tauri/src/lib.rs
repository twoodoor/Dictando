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
mod transcription;

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_global_shortcut::ShortcutState;

use audio::Recorder;
use history::{History, HistoryEntry};
use settings::{AppSettings, SettingsStore};
use transcription::Transcriber;

/// Managed application state (Send + Sync; usable from the recording thread).
pub struct AppState {
    settings: SettingsStore,
    transcriber: Transcriber,
    recorder: Recorder,
    history: History,
    app_data_dir: PathBuf,
    recording_state: Mutex<String>, // "idle" | "recording" | "transcribing"
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

/// Show the recording overlay while active, hide it when idle. The overlay
/// window listens to the same `recording-state` event for its visuals.
fn update_overlay(app: &AppHandle, st: &str) {
    match app.get_webview_window("overlay") {
        Some(w) => {
            if st == "recording" || st == "transcribing" {
                position_overlay(&w);
                if let Err(e) = w.show() {
                    log::error!("overlay show failed: {e}");
                }
                let _ = w.set_always_on_top(true);
            } else {
                let _ = w.hide();
            }
        }
        None => log::warn!("overlay window 'overlay' not found"),
    }
}

/// Place the overlay near the bottom-center of the primary monitor.
fn position_overlay(w: &tauri::WebviewWindow) {
    if let Ok(Some(monitor)) = w.primary_monitor() {
        let screen = monitor.size();
        let win = w.outer_size().unwrap_or(tauri::PhysicalSize::new(200, 54));
        let x = ((screen.width.saturating_sub(win.width)) / 2) as i32;
        let y = (screen.height.saturating_sub(win.height + 90)) as i32;
        let _ = w.set_position(tauri::PhysicalPosition::new(x, y));
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

/// Start capturing audio and broadcast the new state.
fn begin_recording(app: AppHandle) {
    let state = app.state::<AppState>();
    let mic = state.settings.get().microphone_id;
    match state.recorder.start(&mic) {
        Ok(()) => set_recording_state(&app, "recording"),
        Err(e) => log::error!("failed to start recording: {e}"),
    }
}

/// Stop capture, transcribe on a background thread, inject the result, and emit
/// a `transcription` event. Never blocks the caller (e.g. the shortcut handler).
fn finish_recording(app: AppHandle) {
    if !app.state::<AppState>().recorder.is_recording() {
        return;
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

        let model_dir = models::model_dir(&state.app_data_dir, &cfg.active_model_id);
        if let Err(e) = state.transcriber.ensure_loaded(&cfg.active_model_id, &model_dir) {
            log::error!("{e}");
            set_recording_state(&app, "idle");
            return;
        }

        match state.transcriber.transcribe(&samples) {
            Ok(raw) if !raw.is_empty() => {
                // Optional AI cleanup pass (opt-in; falls back to raw on failure).
                let text = if cfg.ai_enhance_enabled && !cfg.gemini_api_key.is_empty() {
                    match ai::enhance(&raw, &cfg.gemini_api_key, &cfg.custom_words) {
                        Ok(t) => t,
                        Err(e) => {
                            log::warn!("AI enhance failed, using raw text: {e}");
                            raw
                        }
                    }
                } else {
                    raw
                };
                log::info!("injecting {} chars via '{}'", text.len(), cfg.paste_method);
                if let Err(e) = inject::inject_text(
                    &text,
                    &cfg.paste_method,
                    cfg.clipboard_handling == "preserve",
                    cfg.append_trailing_space,
                ) {
                    log::error!("inject failed: {e}");
                }
                log::info!("inject complete");
                let entry = HistoryEntry {
                    id: unique_id(),
                    text: text.clone(),
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
fn update_settings(
    app: AppHandle,
    state: State<AppState>,
    patch: serde_json::Value,
) -> Result<AppSettings, String> {
    let merged = state.settings.update(patch)?;
    if let Some(sc) = shortcuts::parse_shortcut(&merged.shortcut) {
        shortcuts::reregister(&app, &sc);
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    let app = app.clone();
                    let state = app.state::<AppState>();
                    let push_to_talk = state.settings.get().push_to_talk;
                    let recording = state.recorder.is_recording();
                    match event.state() {
                        ShortcutState::Pressed => {
                            if push_to_talk {
                                begin_recording(app);
                            } else if recording {
                                finish_recording(app);
                            } else {
                                begin_recording(app);
                            }
                        }
                        ShortcutState::Released => {
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

            // If the saved active model isn't loadable by this target's engine
            // (e.g. a Parakeet/ONNX id on Intel macOS), fall back to the default.
            if models::catalog_entry(&settings.get().active_model_id).map(|e| e.format)
                != Some(models::engine_format())
            {
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
            });

            if let Some(sc) = shortcuts::parse_shortcut(&snapshot.shortcut) {
                shortcuts::reregister(app.handle(), &sc);
            }

            // System tray — gives Show + a real Quit (the X minimizes).
            let handle = app.handle().clone();
            let show = MenuItemBuilder::with_id("show", "Show Dictando").build(&handle)?;
            let quit = MenuItemBuilder::with_id("quit", "Quit Dictando").build(&handle)?;
            let menu = MenuBuilder::new(&handle).items(&[&show, &quit]).build()?;
            let _tray = TrayIconBuilder::with_id("main-tray")
                .icon(handle.default_window_icon().unwrap().clone())
                .tooltip("Dictando")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "show" => show_main(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
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

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            update_settings,
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
