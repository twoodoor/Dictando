//! Local speech-to-text via `transcribe-rs` (ONNX) and direct `whisper-rs` (GGML).
//!
//! Audio in: 16 kHz mono `f32` samples (from `audio.rs`).

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters,
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

/// Does the recording contain at least ~300 ms of audio clearly above its own
/// noise floor? Whisper's stock phrases ("Thank you.", "You") are only junk
/// when there was no real speech — a dictated "Thank you." must survive.
pub fn has_speech(samples: &[f32]) -> bool {
    const FRAME: usize = 320; // 20 ms at 16 kHz
    let mut levels: Vec<f32> = samples
        .chunks(FRAME)
        .map(|f| (f.iter().map(|s| s * s).sum::<f32>() / f.len() as f32).sqrt())
        .collect();
    if levels.is_empty() {
        return false;
    }
    let loud: Vec<f32> = levels.clone();
    levels.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let floor = levels[levels.len() / 10];
    let threshold = (floor * 4.0).max(0.01);
    loud.iter().filter(|&&l| l > threshold).count() >= 15
}

/// Whisper initial prompt built from the user's Dictionary, so names and
/// jargon ("Athmo", "Tauri") come out spelled the user's way. Whisper reads the
/// prompt as preceding context; it caps it at ~224 tokens, so keep it short.
fn vocabulary_prompt(custom_words: &[String]) -> Option<String> {
    let words: Vec<String> = custom_words
        .iter()
        .map(|w| w.replace('\0', "").trim().to_string())
        .filter(|w| !w.is_empty())
        .take(60)
        .collect();
    (!words.is_empty()).then(|| format!("{}.", words.join(", ")))
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
///
/// Whisper always encodes a full 30 s window, so its cost barely depends on
/// clip length: on a Ryzen 9 5950X (CPU, AVX2) Small takes ~2.6 s and Turbo
/// ~13 s per utterance. 15 s timed Turbo out on ordinary dictations and lost
/// them; a late result beats no result.
const WHISPER_TIMEOUT_SECS: u64 = 60;

/// High-performance direct Whisper engine using greedy decoding & multi-threading.
pub struct DirectWhisperEngine {
    context: WhisperContext,
}

impl DirectWhisperEngine {
    pub fn load(model_path: &Path) -> Result<Self, String> {
        let mut context_params = WhisperContextParameters::default();
        context_params.use_gpu = false;
        context_params.flash_attn = false;

        let path_str = model_path.to_str().ok_or("invalid model path")?;
        let context = WhisperContext::new_with_params(path_str, context_params)
            .map_err(|e| format!("failed to load whisper model: {e}"))?;

        Ok(Self { context })
    }

    /// Run inference. This call can block for seconds on CPU.
    /// Call from a dedicated thread; the caller enforces the timeout.
    pub fn transcribe(
        &mut self,
        samples: &[f32],
        language: Option<&str>,
        prompt: Option<&str>,
    ) -> Result<String, String> {
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
        if let Some(p) = prompt {
            params.set_initial_prompt(p);
        }

        let mut state = self
            .context
            .create_state()
            .map_err(|e| format!("failed to create whisper state: {e}"))?;

        state
            .full(params, samples)
            .map_err(|e| format!("whisper inference failed: {e}"))?;

        let num_segments = state.full_n_segments();
        let mut full_text = String::new();
        for i in 0..num_segments {
            if let Some(segment) = state.get_segment(i) {
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
    /// Held for a whole load or inference, so dictations are transcribed one
    /// at a time. While an inference runs its engine is out of `loaded`;
    /// without this a second dictation saw "no model", loaded another copy
    /// (Whisper Turbo: ~570 MB, seconds) and both ran at once, each at half
    /// speed.
    run: Mutex<()>,
}

impl Transcriber {
    pub fn new() -> Self {
        Self { loaded: Mutex::new(None), run: Mutex::new(()) }
    }

    fn lock_run(&self) -> std::sync::MutexGuard<'_, ()> {
        self.run.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn is_loaded(&self) -> bool {
        self.loaded.lock().unwrap().is_some()
    }

    pub fn loaded_model_id(&self) -> Option<String> {
        self.loaded.lock().unwrap().as_ref().map(|l| l.model_id.clone())
    }

    /// Ensure the model `model_id` (stored at `model_dir`) is loaded.
    pub fn ensure_loaded(&self, model_id: &str, model_dir: &Path) -> Result<(), String> {
        // Wait for an in-flight inference to hand its engine back first.
        let _run = self.lock_run();
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
    /// looks like a hallucination). `custom_words` (the Dictionary) bias
    /// Whisper's spelling; the ONNX engines have no prompt input.
    ///
    /// Runs inference on an off-mutex background thread with safety timeout so
    /// the app never freezes even on long audio or unsupported languages.
    pub fn transcribe(
        &self,
        samples: &[f32],
        language: Option<&str>,
        custom_words: &[String],
    ) -> Result<String, String> {
        if samples.len() < 1600 {
            // Less than 100ms of audio — nothing to transcribe.
            return Ok(String::new());
        }

        let started = Instant::now();

        // One inference at a time (see `run`), held until this call returns.
        let _run = self.lock_run();

        // Move the engine OUT of the mutex so the lock is NEVER held during inference.
        // We put it back when the inference thread finishes (or times out).
        let (engine, model_id) = {
            let mut guard = self.loaded.lock().unwrap();
            let loaded = guard.take().ok_or("no model loaded")?;
            (loaded.model, loaded.model_id)
        };

        let samples_owned: Vec<f32> = samples.to_vec();
        let language_owned: Option<String> = language.map(|s| s.to_string());
        let prompt = vocabulary_prompt(custom_words);
        let prompt_thread = prompt.clone();

        // Channel carries (engine, inference_result) back to this thread.
        let (tx, rx) = std::sync::mpsc::channel::<(Engine, Result<String, String>)>();

        std::thread::spawn(move || {
            let mut eng = engine;
            let lang_ref: Option<&str> = language_owned.as_deref();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match &mut eng {
                Engine::Whisper(w) => w.transcribe(&samples_owned, lang_ref, prompt_thread.as_deref()),
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
            Ok((engine, Err(e))) => {
                log::error!("Inference error for '{model_id}': {e}");
                // Keep the model loaded: one bad clip must not force a reload.
                let mut guard = self.loaded.lock().unwrap();
                *guard = Some(Loaded { model: engine, model_id: model_id.clone(), last_used: Instant::now() });
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
        // Whisper invents stock phrases on silence, and can echo its prompt.
        let whisper_junk = model_id.starts_with("whisper")
            && ((is_hallucination(&text) && !has_speech(samples))
                || prompt.as_deref().is_some_and(|p| text.trim_end_matches('.') == p.trim_end_matches('.')));
        let empty = text.trim_matches(|c: char| c.is_ascii_punctuation() || c.is_whitespace()).is_empty();
        if whisper_junk || empty {
            log::info!("discarded transcript as non-speech: {text:?}");
            Ok(String::new())
        } else {
            Ok(text)
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn noise(n: usize, amp: f32) -> Vec<f32> {
        // Deterministic pseudo-noise.
        let mut x = 0x1234_5678u32;
        (0..n)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                (x as f32 / u32::MAX as f32 - 0.5) * 2.0 * amp
            })
            .collect()
    }

    #[test]
    fn silence_is_not_speech() {
        assert!(!has_speech(&noise(32_000, 0.002)));
    }

    #[test]
    fn spoken_burst_is_speech() {
        // 2 s of room noise with 0.6 s of speech-level signal in the middle.
        let mut x = noise(32_000, 0.002);
        for (i, s) in x.iter_mut().enumerate().skip(12_000).take(9_600) {
            *s += (i as f32 * 0.05).sin() * 0.1;
        }
        assert!(has_speech(&x));
    }

    #[test]
    fn vocabulary_prompt_from_dictionary() {
        let words = vec!["Zeea".to_string(), " Athmo ".to_string(), String::new()];
        assert_eq!(vocabulary_prompt(&words).as_deref(), Some("Zeea, Athmo."));
        assert_eq!(vocabulary_prompt(&[]), None);
    }
    // ── Accuracy / latency benchmark ────────────────────────────────────
    // Not run by default. Point BENCH_DIR at a folder with `sentences.txt`
    // (one reference per line) and `NN_<anything>.wav` clips (PCM16, any
    // rate; NN = 1-based line number). Uses the installed app models:
    //   BENCH_DIR=... cargo test --release --lib bench_models -- --ignored --nocapture

    fn read_wav(path: &std::path::Path) -> (u32, Vec<f32>) {
        let b = std::fs::read(path).unwrap();
        let (mut rate, mut channels, mut pos) = (16_000u32, 1usize, 12usize);
        while pos + 8 <= b.len() {
            let id = &b[pos..pos + 4];
            let len = u32::from_le_bytes(b[pos + 4..pos + 8].try_into().unwrap()) as usize;
            let body = &b[pos + 8..(pos + 8 + len).min(b.len())];
            if id == b"fmt " {
                channels = u16::from_le_bytes([body[2], body[3]]) as usize;
                rate = u32::from_le_bytes(body[4..8].try_into().unwrap());
            } else if id == b"data" {
                let pcm: Vec<f32> = body
                    .chunks_exact(2 * channels)
                    .map(|f| i16::from_le_bytes([f[0], f[1]]) as f32 / 32768.0)
                    .collect();
                return (rate, pcm);
            }
            pos += 8 + len + (len & 1);
        }
        panic!("no data chunk in {}", path.display());
    }

    fn norm_words(s: &str) -> Vec<String> {
        s.to_lowercase()
            .replace(['-', '\u{2019}'], " ")
            .split_whitespace()
            .map(|w| w.chars().filter(|c| c.is_alphanumeric() || *c == '\'').collect::<String>())
            .filter(|w| !w.is_empty())
            .collect()
    }

    /// Word-level edit distance → (errors, reference words).
    fn wer(reference: &str, hypothesis: &str) -> (usize, usize) {
        let (r, h) = (norm_words(reference), norm_words(hypothesis));
        let mut prev: Vec<usize> = (0..=h.len()).collect();
        for i in 1..=r.len() {
            let mut cur = vec![i; h.len() + 1];
            for j in 1..=h.len() {
                let sub = prev[j - 1] + usize::from(r[i - 1] != h[j - 1]);
                cur[j] = sub.min(prev[j] + 1).min(cur[j - 1] + 1);
            }
            prev = cur;
        }
        (prev[h.len()], r.len())
    }

    #[test]
    fn wer_counts_word_edits() {
        assert_eq!(wer("What does it look like?", "what does it look like"), (0, 5));
        assert_eq!(wer("What does it look like?", "What does it look."), (1, 5));
        assert_eq!(wer("as well", "as a well"), (1, 2));
    }

    #[test]
    #[ignore]
    fn bench_models() {
        let dir = std::path::PathBuf::from(std::env::var("BENCH_DIR").expect("set BENCH_DIR"));
        let app_data = std::path::PathBuf::from(std::env::var("APPDATA").expect("APPDATA"))
            .join("com.mumblr.desktop");
        let refs: Vec<String> = std::fs::read_to_string(dir.join("sentences.txt"))
            .unwrap()
            .lines()
            .map(str::to_string)
            .collect();
        let mut wavs: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "wav"))
            .collect();
        wavs.sort();
        let clips: Vec<(String, Vec<f32>)> = wavs
            .iter()
            .map(|p| {
                let n: usize = p.file_name().unwrap().to_str().unwrap()[..2].parse().unwrap();
                let (rate, pcm) = read_wav(p);
                (refs[n - 1].clone(), crate::audio::resample(&pcm, rate, 16_000))
            })
            .collect();
        let audio_secs: f32 = clips.iter().map(|c| c.1.len() as f32 / 16_000.0).sum();
        let dictionary: Vec<String> =
            ["Zeea", "Tudor", "Athmo", "Tauri"].iter().map(|s| s.to_string()).collect();
        println!("\n{} clips, {:.0} s of audio\n", clips.len(), audio_secs);

        for id in [
            "sense-voice-int8",
            "parakeet-tdt-0.6b-v2",
            "parakeet-tdt-0.6b-v3",
            "whisper-small",
            "whisper-turbo",
        ] {
            let model_dir = crate::models::model_dir(&app_data, id);
            if !model_dir.exists() {
                continue;
            }
            let t = Transcriber::new();
            if let Err(e) = t.ensure_loaded(id, &model_dir) {
                println!("{id}: load failed: {e}");
                continue;
            }
            let prompts: &[bool] = if id.starts_with("whisper") { &[false, true] } else { &[false] };
            for &use_prompt in prompts {
                let words: &[String] = if use_prompt { &dictionary } else { &[] };
                let warm = Instant::now();
                if let Err(e) = t.transcribe(&clips[0].1, Some("en"), words) {
                    println!("{id}: warm-up failed after {} ms: {e}", warm.elapsed().as_millis());
                    let _ = t.ensure_loaded(id, &model_dir);
                }
                let (mut errs, mut total, mut ms) = (0, 0, 0u128);
                let mut misses = Vec::new();
                for (reference, samples) in &clips {
                    let started = Instant::now();
                    let hyp = t.transcribe(samples, Some("en"), words).unwrap_or_else(|e| format!("<{e}>"));
                    ms += started.elapsed().as_millis();
                    let (e, n) = wer(reference, &hyp);
                    errs += e;
                    total += n;
                    if e > 0 {
                        misses.push(format!("    [{e}] {hyp}"));
                    }
                }
                println!(
                    "{id:22} {:9} WER {:5.1}%  ({errs:3}/{total} words)  {:5} ms/clip  {:4.0} ms per audio-second",
                    if use_prompt { "+dict" } else { "" },
                    100.0 * errs as f32 / total as f32,
                    ms / clips.len() as u128,
                    ms as f32 / audio_secs,
                );
                for m in &misses {
                    println!("{m}");
                }
            }
        }
    }
}
