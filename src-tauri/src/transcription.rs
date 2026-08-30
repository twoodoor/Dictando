//! Local speech-to-text via `transcribe-rs` (ONNX) and direct `whisper-rs` (GGML).
//!
//! Audio in: 16 kHz mono `f32` samples (from `audio.rs`).

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState,
};

#[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
use transcribe_rs::onnx::gigaam::GigaAMModel;
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

use crate::models::find_model_dir;

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

/// Convert display language name to standard ISO-639-1 language code.
pub fn language_to_code(lang: &str) -> Option<&'static str> {
    match lang {
        "English" => Some("en"),
        "Spanish" => Some("es"),
        "French" => Some("fr"),
        "German" => Some("de"),
        "Italian" => Some("it"),
        "Portuguese" => Some("pt"),
        "Romanian" => Some("ro"),
        "Dutch" => Some("nl"),
        "Russian" => Some("ru"),
        "Polish" => Some("pl"),
        "Ukrainian" => Some("uk"),
        "Czech" => Some("cs"),
        "Swedish" => Some("sv"),
        "Danish" => Some("da"),
        "Finnish" => Some("fi"),
        "Greek" => Some("el"),
        _ => None, // "Auto-detect" or empty
    }
}

/// Whisper inference timeout. Whisper.cpp BLAS kernels block in a single C call
/// for the entire inference duration and never return to Rust, so abort callbacks
/// don't help for large models. We use a thread + channel timeout instead.
const WHISPER_TIMEOUT_SECS: u64 = 15;

/// High-performance direct Whisper engine using greedy decoding & multi-threading.
pub struct DirectWhisperEngine {
    #[allow(dead_code)]
    context: WhisperContext,
    state: WhisperState,
}

impl DirectWhisperEngine {
    pub fn load(model_path: &Path) -> Result<Self, String> {
        let mut context_params = WhisperContextParameters::default();
        context_params.use_gpu = false;
        context_params.flash_attn = false;

        let path_str = model_path.to_str().ok_or("invalid model path")?;
        let context = WhisperContext::new_with_params(path_str, context_params)
            .map_err(|e| format!("failed to load whisper model: {e}"))?;
        let state = context
            .create_state()
            .map_err(|e| format!("failed to create whisper state: {e}"))?;

        Ok(Self { context, state })
    }

    /// Run inference. This call can block for seconds on CPU.
    /// Call from a dedicated thread; the caller enforces the timeout.
    pub fn transcribe(&mut self, samples: &[f32], language: Option<&str>) -> Result<String, String> {
        // Optimal thread count for whisper.cpp on multi-core CPUs is 4-8.
        // Higher counts (16-32) cause L3 cache thrashing across CCX dies.
        let threads = std::thread::available_parallelism()
            .map(|n| n.get() as i32)
            .unwrap_or(4)
            .clamp(1, 8);

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_n_threads(threads);
        params.set_language(language);
        params.set_translate(false);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_suppress_blank(true);
        params.set_suppress_nst(true);
        params.set_no_speech_thold(0.6);
        params.set_single_segment(false);
        // Disable temperature fallback — single greedy pass only.
        params.set_temperature(0.0);
        params.set_temperature_inc(0.0);

        self.state
            .full(params, samples)
            .map_err(|e| format!("whisper inference failed: {e}"))?;

        let num_segments = self.state.full_n_segments();
        let mut full_text = String::new();
        for i in 0..num_segments {
            if let Some(segment) = self.state.get_segment(i) {
                if let Ok(text) = segment.to_str() {
                    full_text.push_str(text);
                }
            }
        }

        Ok(full_text)
    }
}

pub enum Engine {
    #[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
    Onnx(Box<dyn SpeechModel + Send>),
    Whisper(DirectWhisperEngine),
}

/// Locate the single GGML `.bin` inside a Whisper model directory.
fn find_ggml_bin(model_dir: &Path) -> Result<PathBuf, String> {
    let resolved = find_model_dir(model_dir);
    let mut bins: Vec<PathBuf> = std::fs::read_dir(&resolved)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|x| x == "bin").unwrap_or(false))
        .collect();

    if bins.is_empty() {
        return Err(format!("no .bin model file in {}", resolved.display()));
    }

    // Prioritize quantized models (q5_0, q4) over unquantized ones.
    bins.sort_by_key(|p| {
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.contains("q5_0") || name.contains("q4") { 0 } else { 1 }
    });

    Ok(bins.remove(0))
}

fn load_engine(model_id: &str, model_dir: &Path) -> Result<Engine, String> {
    // Reject models that are far too slow for real-time CPU inference.
    // whisper-medium: ~90s per utterance on Ryzen 5950X.
    // whisper-large:  ~3-5 minutes per utterance on CPU.
    if model_id == "whisper-medium" || model_id == "whisper-large" {
        return Err(format!(
            "'{model_id}' is too slow for real-time CPU use (typically >60 s/utterance). \
             Switch to Whisper Tiny, Base, Small, or Turbo for usable performance."
        ));
    }

    let resolved_dir = find_model_dir(model_dir);
    log::info!("Resolved model directory for '{model_id}': {}", resolved_dir.display());

    #[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
    if model_id.starts_with("parakeet") {
        return Ok(Engine::Onnx(Box::new(
            ParakeetModel::load(&resolved_dir, &Quantization::Int8).map_err(|e| e.to_string())?,
        )));
    } else if model_id.starts_with("moonshine") {
        let variant = if model_id.contains("tiny") {
            MoonshineVariant::Tiny
        } else {
            MoonshineVariant::Base
        };
        return Ok(Engine::Onnx(Box::new(
            MoonshineModel::load(&resolved_dir, variant, &Quantization::default())
                .map_err(|e| e.to_string())?,
        )));
    } else if model_id.starts_with("sense-voice") {
        return Ok(Engine::Onnx(Box::new(
            SenseVoiceModel::load(&resolved_dir, &Quantization::Int8).map_err(|e| e.to_string())?,
        )));
    } else if model_id.starts_with("giga-am") || model_id.starts_with("gigaam") {
        return Ok(Engine::Onnx(Box::new(
            GigaAMModel::load(&resolved_dir, &Quantization::Int8).map_err(|e| e.to_string())?,
        )));
    }

    if model_id.starts_with("whisper") {
        let bin_path = find_ggml_bin(&resolved_dir)?;
        return Ok(Engine::Whisper(DirectWhisperEngine::load(&bin_path)?));
    }

    Err(format!("model '{model_id}' is not supported by this build"))
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
    ///
    /// Runs inference on an off-mutex background thread with safety timeout so
    /// the app never freezes even on long audio or unsupported languages.
    pub fn transcribe(&self, samples: &[f32], language: Option<&str>) -> Result<String, String> {
        if samples.len() < 1600 {
            // Less than 100ms of audio — nothing to transcribe.
            return Ok(String::new());
        }

        let started = Instant::now();

        // Move the engine OUT of the mutex so the lock is NEVER held during inference.
        // We put it back when the inference thread finishes (or times out).
        let (engine, model_id) = {
            let mut guard = self.loaded.lock().unwrap();
            let loaded = guard.take().ok_or("no model loaded")?;
            (loaded.model, loaded.model_id)
        };

        let samples_owned: Vec<f32> = samples.to_vec();
        let language_owned: Option<String> = language.map(|s| s.to_string());

        // Channel carries (engine, inference_result) back to this thread.
        let (tx, rx) = std::sync::mpsc::channel::<(Engine, Result<String, String>)>();

        std::thread::spawn(move || {
            let mut eng = engine;
            let lang_ref: Option<&str> = language_owned.as_deref();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match &mut eng {
                Engine::Whisper(w) => w.transcribe(&samples_owned, lang_ref),
                #[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
                Engine::Onnx(model) => {
                    let mut options = TranscribeOptions::default();
                    if let Some(lang) = lang_ref {
                        options.language = Some(lang.to_string());
                    }
                    model
                        .transcribe(&samples_owned, &options)
                        .map(|r| r.text)
                        .map_err(|e| e.to_string())
                }
            }))
            .map_err(|_| "Model inference failed unexpectedly".to_string())
            .and_then(|r| r);

            // Send engine back unconditionally so it can be restored.
            let _ = tx.send((eng, result));
        });

        let timeout_secs = if model_id.starts_with("whisper") {
            WHISPER_TIMEOUT_SECS
        } else {
            10
        };

        let text = match rx.recv_timeout(std::time::Duration::from_secs(timeout_secs)) {
            Ok((engine, Ok(text))) => {
                // Restore the engine into the mutex.
                let mut guard = self.loaded.lock().unwrap();
                *guard = Some(Loaded { model: engine, model_id: model_id.clone(), last_used: Instant::now() });
                text
            }
            Ok((_, Err(e))) => {
                log::error!("Inference error for '{model_id}': {e}");
                return Err(e);
            }
            Err(_timeout) => {
                log::warn!("Inference timed out after {timeout_secs}s for '{model_id}'");
                return Err(format!(
                    "Transcription timed out after {timeout_secs}s. \
                     Please try a different model supporting your language."
                ));
            }
        };

        let text = text.trim().to_string();
        log::info!(
            "Transcribed {:.1}s audio with '{}' in {} ms -> {} chars",
            samples.len() as f32 / 16_000.0,
            model_id,
            started.elapsed().as_millis(),
            text.len()
        );
        if is_hallucination(&text) { Ok(String::new()) } else { Ok(text) }
    }

    /// Unload the model after `idle_minutes` of inactivity (0 disables).
    #[allow(dead_code)]
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
