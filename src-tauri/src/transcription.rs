//! Local speech-to-text via `transcribe-rs`.
//!
//! Engine is chosen at compile time per target (see `Cargo.toml`):
//! - **Intel macOS (x86_64):** Whisper (`whisper.cpp`) only — ONNX ships no
//!   Intel-mac prebuilt binary. Loads a single GGML `.bin`.
//! - **Everything else (Windows, Apple-Silicon macOS, Linux):** both ONNX
//!   (Parakeet TDT 0.6B v3 int8 — 25 languages, fast — plus Moonshine/
//!   SenseVoice) *and* whisper.cpp, so the OpenAI Whisper GGML models load too.
//!   ONNX loads an extracted model directory; Whisper loads a single `.bin`.
//!
//! Audio in: 16 kHz mono `f32` samples (from `audio.rs`).

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

// --- ONNX engines (all targets except Intel macOS) ---
// Dispatched through the unified `SpeechModel` trait, so several engines share
// one transcribe path; only construction differs per model id.
#[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
use transcribe_rs::onnx::moonshine::{MoonshineModel, MoonshineVariant};
#[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
use transcribe_rs::onnx::parakeet::ParakeetModel;
#[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
use transcribe_rs::onnx::sense_voice::SenseVoiceModel;
#[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
use transcribe_rs::onnx::Quantization;
#[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
use transcribe_rs::{SpeechModel, TranscribeOptions};
// whisper.cpp is also compiled on these targets so the OpenAI Whisper GGML
// models load; `WhisperEngine` implements `SpeechModel`, so it boxes into the
// same dispatch as the ONNX engines below.
#[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
use transcribe_rs::whisper_cpp::WhisperEngine;
#[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
type Engine = Box<dyn SpeechModel + Send>;

// --- Whisper (whisper.cpp) on Intel macOS: the only compiled engine ---
#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
use transcribe_rs::whisper_cpp::{WhisperEngine, WhisperInferenceParams};
#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
type Engine = WhisperEngine;

/// Whisper hallucination phrases emitted on silence/noise (ported from the
/// legacy Python app). Used to suppress junk output.
const HALLUCINATIONS: &[&str] = &[
    "thank you", "thanks for watching", "thanks for watching!",
    "thank you for watching", "thank you for watching!",
    "you", ".", "..", "...", "thanks.", "thank you.",
    "[music]", "[applause]", "[laughter]", "[ music ]",
    "subtitles by", "amara.org",
];

pub fn is_hallucination(text: &str) -> bool {
    let cleaned = text.trim().to_lowercase();
    let cleaned = cleaned.trim_matches(|c| c == '.' || c == ',' || c == '!' || c == '?');
    cleaned.is_empty() || HALLUCINATIONS.contains(&cleaned)
}

// --- Engine-specific load + transcribe (the only parts that differ) ---

/// Locate the single GGML `.bin` inside a Whisper model directory.
fn find_ggml_bin(model_dir: &Path) -> Result<PathBuf, String> {
    std::fs::read_dir(model_dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .find(|p| p.extension().map(|x| x == "bin").unwrap_or(false))
        .ok_or_else(|| format!("no .bin model file in {}", model_dir.display()))
}

#[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
fn load_engine(model_id: &str, model_dir: &Path) -> Result<Engine, String> {
    let model: Engine = if model_id.starts_with("parakeet") {
        Box::new(ParakeetModel::load(model_dir, &Quantization::Int8).map_err(|e| e.to_string())?)
    } else if model_id.starts_with("moonshine") {
        Box::new(
            MoonshineModel::load(model_dir, MoonshineVariant::Base, &Quantization::default())
                .map_err(|e| e.to_string())?,
        )
    } else if model_id.starts_with("sense-voice") {
        Box::new(SenseVoiceModel::load(model_dir, &Quantization::Int8).map_err(|e| e.to_string())?)
    } else if model_id.starts_with("whisper") {
        // OpenAI Whisper via whisper.cpp — loads a single GGML .bin.
        Box::new(WhisperEngine::load(&find_ggml_bin(model_dir)?).map_err(|e| e.to_string())?)
    } else {
        return Err(format!("model '{model_id}' is not supported by this build"));
    };
    Ok(model)
}
#[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
fn run_transcribe(model: &mut Engine, samples: &[f32]) -> Result<String, String> {
    model
        .transcribe(samples, &TranscribeOptions::default())
        .map(|r| r.text)
        .map_err(|e| e.to_string())
}

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
fn load_engine(_model_id: &str, model_dir: &Path) -> Result<Engine, String> {
    // Whisper loads a single GGML .bin from inside the model directory.
    WhisperEngine::load(&find_ggml_bin(model_dir)?).map_err(|e| e.to_string())
}
#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
fn run_transcribe(model: &mut Engine, samples: &[f32]) -> Result<String, String> {
    model
        .transcribe_with(samples, &WhisperInferenceParams::default())
        .map(|r| r.text)
        .map_err(|e| e.to_string())
}

struct Loaded {
    model: Engine,
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

    pub fn is_loaded(&self) -> bool {
        self.loaded.lock().unwrap().is_some()
    }

    pub fn loaded_model_id(&self) -> Option<String> {
        self.loaded.lock().unwrap().as_ref().map(|l| l.model_id.clone())
    }

    /// Ensure the model `model_id` (stored at `model_dir`) is loaded.
    pub fn ensure_loaded(&self, model_id: &str, model_dir: &Path) -> Result<(), String> {
        let mut guard = self.loaded.lock().unwrap();
        if guard.as_ref().map(|l| l.model_id.as_str()) == Some(model_id) {
            return Ok(());
        }
        let model_dir: PathBuf = model_dir.to_path_buf();
        if !model_dir.exists() {
            return Err(format!("model '{model_id}' is not installed at {}", model_dir.display()));
        }
        log::info!("Loading model '{model_id}' from {}", model_dir.display());
        let model = load_engine(model_id, &model_dir).map_err(|e| format!("failed to load '{model_id}': {e}"))?;
        *guard = Some(Loaded { model, model_id: model_id.to_string(), last_used: Instant::now() });
        Ok(())
    }

    /// Transcribe 16 kHz mono f32 `samples`. Returns cleaned text (empty if it
    /// looks like a hallucination).
    pub fn transcribe(&self, samples: &[f32]) -> Result<String, String> {
        let mut guard = self.loaded.lock().unwrap();
        let loaded = guard.as_mut().ok_or("no model loaded")?;
        let started = Instant::now();
        let text = run_transcribe(&mut loaded.model, samples)?;
        loaded.last_used = Instant::now();
        let text = text.trim().to_string();
        log::info!(
            "Transcribed {:.1}s audio in {} ms -> {} chars",
            samples.len() as f32 / 16_000.0,
            started.elapsed().as_millis(),
            text.len()
        );
        if is_hallucination(&text) { Ok(String::new()) } else { Ok(text) }
    }

    /// Fast partial transcription pass for streaming audio without verbose logging.
    pub fn transcribe_partial(&self, samples: &[f32]) -> Option<String> {
        let mut guard = self.loaded.lock().ok()?;
        let loaded = guard.as_mut()?;
        let text = run_transcribe(&mut loaded.model, samples).ok()?;
        let text = text.trim().to_string();
        if is_hallucination(&text) {
            None
        } else {
            Some(text)
        }
    }

    /// Unload the model after `idle_minutes` of inactivity (0 disables).
    pub fn unload_idle(&self, idle_minutes: u32) {
        if idle_minutes == 0 {
            return;
        }
        let mut guard = self.loaded.lock().unwrap();
        let should = guard
            .as_ref()
            .map(|l| l.last_used.elapsed().as_secs() >= (idle_minutes as u64) * 60)
            .unwrap_or(false);
        if should {
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
