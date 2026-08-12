//! Persisted application settings (local-first).
//!
//! Settings live in `<app_data_dir>/settings.json` and mirror the `AppSettings`
//! interface in `src/lib/bridge.ts`. When the user is logged in, the frontend
//! may additionally sync these to Firebase, but the local file is the source of
//! truth for the native app.

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    /// Global shortcut as `KeyboardEvent.code` values, e.g. ["ControlLeft","Space"].
    pub shortcut: Vec<String>,
    pub push_to_talk: bool,
    pub language: String,
    pub active_model_id: String,
    pub microphone_id: String,
    pub mute_while_recording: bool,
    pub audio_feedback: bool,
    pub paste_method: String,        // "direct" | "clipboard"
    pub clipboard_handling: String,  // "preserve" | "overwrite"
    pub append_trailing_space: bool,
    pub auto_submit: bool,
    pub custom_words: Vec<String>,
    pub overlay_position: String,    // "top" | "bottom" | "none"
    pub launch_on_startup: bool,
    pub start_hidden: bool,
    pub show_tray_icon: bool,
    pub unload_model_minutes: u32,   // 0 = never unload
    pub history_limit: u32,
    pub theme: String,               // "light" | "dark" | "system"
    // AI enhancement layer (Phase 4). Off by default → transcription stays offline.
    pub ai_enhance_enabled: bool,
    pub gemini_api_key: String,
    #[serde(default = "default_true")]
    pub ai_fix_punctuation: bool,
    #[serde(default = "default_true")]
    pub ai_remove_fillers: bool,
    #[serde(default = "default_true")]
    pub ai_remove_repetitions: bool,
    #[serde(default = "default_clean")]
    pub ai_style_preset: String, // "clean" | "polished" | "concise" | "casual"
    #[serde(default)]
    pub ai_custom_instructions: String,
}

fn default_true() -> bool {
    true
}

fn default_clean() -> String {
    "clean".to_string()
}

/// Default active model for this target. Intel macOS runs the Whisper engine
/// (Parakeet/ONNX has no Intel-mac build), so it defaults to a GGML model.
pub fn default_model_id() -> String {
    if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        "whisper-small".into()
    } else {
        "parakeet-tdt-0.6b-v3".into()
    }
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            shortcut: vec!["ControlLeft".into(), "Space".into()],
            push_to_talk: true,
            language: "Auto-detect".into(),
            active_model_id: default_model_id(),
            microphone_id: "default".into(),
            mute_while_recording: false,
            audio_feedback: false,
            paste_method: "direct".into(),
            clipboard_handling: "preserve".into(),
            append_trailing_space: false,
            auto_submit: false,
            custom_words: Vec::new(),
            overlay_position: "bottom".into(),
            launch_on_startup: false,
            start_hidden: false,
            show_tray_icon: true,
            unload_model_minutes: 5,
            history_limit: 50,
            theme: "light".into(),
            ai_enhance_enabled: false,
            gemini_api_key: String::new(),
            ai_fix_punctuation: true,
            ai_remove_fillers: true,
            ai_remove_repetitions: true,
            ai_style_preset: "clean".into(),
            ai_custom_instructions: String::new(),
        }
    }
}

/// Thread-safe settings store backed by a JSON file on disk.
pub struct SettingsStore {
    pub path: PathBuf,
    inner: Mutex<AppSettings>,
}

impl SettingsStore {
    /// Load from `path`, falling back to defaults (and writing them) if missing
    /// or corrupt.
    pub fn load(path: PathBuf) -> Self {
        let settings = fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str::<AppSettings>(&s).ok())
            .unwrap_or_default();
        let store = Self { path, inner: Mutex::new(settings) };
        let _ = store.persist();
        store
    }

    pub fn get(&self) -> AppSettings {
        self.inner.lock().unwrap().clone()
    }

    /// Merge a partial JSON patch (camelCase keys) into the current settings.
    pub fn update(&self, patch: Value) -> Result<AppSettings, String> {
        let mut guard = self.inner.lock().unwrap();
        let mut current =
            serde_json::to_value(&*guard).map_err(|e| e.to_string())?;
        if let (Some(obj), Some(patch_obj)) = (current.as_object_mut(), patch.as_object()) {
            for (k, v) in patch_obj {
                obj.insert(k.clone(), v.clone());
            }
        }
        let merged: AppSettings =
            serde_json::from_value(current).map_err(|e| e.to_string())?;
        *guard = merged.clone();
        drop(guard);
        self.persist_value(&merged)?;
        Ok(merged)
    }

    fn persist(&self) -> Result<(), String> {
        let snapshot = self.inner.lock().unwrap().clone();
        self.persist_value(&snapshot)
    }

    fn persist_value(&self, settings: &AppSettings) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
        fs::write(&self.path, json).map_err(|e| e.to_string())
    }
}
