//! Local speech-to-text via `transcribe-rs`.
//!
//! Phase 1 wires the default model, **Parakeet TDT 0.6B v3 (int8 ONNX)** — 25
//! languages, auto language detection, fast on CPU. The model is loaded lazily
//! on first use and can be unloaded after an idle timeout (see `unload_idle`).
//!
//! Audio in: 16 kHz mono `f32` samples (produced by `audio.rs`).

use std::path::Path;
use std::sync::Mutex;
use std::time::Instant;

use transcribe_rs::onnx::parakeet::{ParakeetModel, ParakeetParams};
use transcribe_rs::onnx::Quantization;

/// Whisper hallucination phrases emitted on silence/noise (ported from the
/// legacy Python app's `FW_HALLUCINATIONS`). Used to suppress junk output.
const HALLUCINATIONS: &[&str] = &[
    "thank you",
    "thanks for watching",
    "thanks for watching!",
    "thank you for watching",
    "thank you for watching!",
    "you",
    ".",
    "..",
    "...",
    "thanks.",
    "thank you.",
    "[music]",
    "[applause]",
    "[laughter]",
    "[ music ]",
    "subtitles by",
    "amara.org",
];

/// Returns true if `text` is empty or a known hallucination phrase.
pub fn is_hallucination(text: &str) -> bool {
    let cleaned = text.trim().to_lowercase();
    let cleaned = cleaned.trim_matches(|c| c == '.' || c == ',' || c == '!' || c == '?');
    cleaned.is_empty() || HALLUCINATIONS.contains(&cleaned)
}

struct Loaded {
    model: ParakeetModel,
    model_id: String,
    last_used: Instant,
}

/// Holds the currently loaded model (if any) behind a mutex so it can live in
/// Tauri managed state and be used from the recording thread.
pub struct Transcriber {
    loaded: Mutex<Option<Loaded>>,
}

impl Transcriber {
    pub fn new() -> Self {
        Self { loaded: Mutex::new(None) }
    }

    /// True when a model is currently loaded in memory.
    pub fn is_loaded(&self) -> bool {
        self.loaded.lock().unwrap().is_some()
    }

    pub fn loaded_model_id(&self) -> Option<String> {
        self.loaded.lock().unwrap().as_ref().map(|l| l.model_id.clone())
    }

    /// Ensure the model identified by `model_id` (stored at `model_dir`) is
    /// loaded, replacing any previously loaded model.
    pub fn ensure_loaded(&self, model_id: &str, model_dir: &Path) -> Result<(), String> {
        let mut guard = self.loaded.lock().unwrap();
        if guard.as_ref().map(|l| l.model_id.as_str()) == Some(model_id) {
            return Ok(());
        }
        if !model_dir.exists() {
            return Err(format!(
                "model '{model_id}' is not installed at {}",
                model_dir.display()
            ));
        }
        log::info!("Loading model '{model_id}' from {}", model_dir.display());
        let model = ParakeetModel::load(model_dir, &Quantization::Int8)
            .map_err(|e| format!("failed to load model '{model_id}': {e}"))?;
        *guard = Some(Loaded { model, model_id: model_id.to_string(), last_used: Instant::now() });
        Ok(())
    }

    /// Transcribe 16 kHz mono f32 `samples`. Returns the cleaned text, or an
    /// empty string if the result is a hallucination.
    pub fn transcribe(&self, samples: &[f32]) -> Result<String, String> {
        let mut guard = self.loaded.lock().unwrap();
        let loaded = guard.as_mut().ok_or("no model loaded")?;
        let started = Instant::now();
        let result = loaded
            .model
            .transcribe_with(samples, &ParakeetParams::default())
            .map_err(|e| format!("transcription failed: {e}"))?;
        loaded.last_used = Instant::now();
        let text = result.text.trim().to_string();
        log::info!(
            "Transcribed {:.1}s audio in {} ms -> {} chars",
            samples.len() as f32 / 16_000.0,
            started.elapsed().as_millis(),
            text.len()
        );
        if is_hallucination(&text) {
            Ok(String::new())
        } else {
            Ok(text)
        }
    }

    /// Unload the model if it has been idle longer than `idle_minutes`
    /// (0 disables unloading). Call periodically from a background task.
    pub fn unload_idle(&self, idle_minutes: u32) {
        if idle_minutes == 0 {
            return;
        }
        let mut guard = self.loaded.lock().unwrap();
        let should_unload = guard
            .as_ref()
            .map(|l| l.last_used.elapsed().as_secs() >= (idle_minutes as u64) * 60)
            .unwrap_or(false);
        if should_unload {
            log::info!("Unloading idle model");
            *guard = None;
        }
    }
}

impl Default for Transcriber {
    fn default() -> Self {
        Self::new()
    }
}
