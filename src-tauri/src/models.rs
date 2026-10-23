//! Model catalog and on-disk layout.
//!
//! Models are stored under `<app_data_dir>/models/<id>/`. Phase 1 ships the
//! catalog and path resolution; Phase 2 adds streamed downloading, archive
//! extraction, and checksum verification (see the `download_url`/`sha256`
//! fields kept here for that purpose).

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter};

/// Catalog entry sent to the frontend (mirrors `ModelInfo` in bridge.ts, plus
/// `installed` which is computed at runtime).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub languages: String,
    pub size_bytes: u64,
    pub accuracy: f32,
    pub speed: f32,
    pub format: String, // "onnx" | "ggml"
    pub supports_translation: bool,
    pub installed: bool,
    /// Full display names of every language the model transcribes.
    pub language_list: Vec<String>,
    pub params: String,
    pub architecture: String,
    /// Typical time to transcribe a ~4 s utterance on a desktop CPU.
    pub latency_ms: u32,
    /// True when `latency_ms` comes from our benchmark, false = estimate.
    pub latency_measured: bool,
    /// Benchmark word error rate in percent (English clips), if measured.
    pub wer_pct: Option<f32>,
    pub ram_mb: u32,
    pub best_for: String,
    pub notes: Vec<String>,
    pub punctuation: bool,
    pub uses_dictionary: bool,
    /// False when the engine refuses it for live dictation (too slow on CPU).
    pub dictation_ready: bool,
}

/// Internal catalog entry including download metadata not exposed to the UI.
pub struct CatalogEntry {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub languages: &'static str,
    pub size_bytes: u64,
    pub accuracy: f32,
    pub speed: f32,
    pub format: &'static str,
    pub supports_translation: bool,
    /// Download URL (filled in for Phase 2). Parakeet entries point at a
    /// compressed archive that is extracted into the model directory.
    pub download_url: &'static str,
    /// Expected SHA-256 of the downloaded artifact (Phase 2 verification).
    pub sha256: &'static str,
    /// ISO-639-1 codes this model can transcribe. Empty = all languages
    /// (e.g. Whisper). Used by smart routing to find a model for the language.
    pub supported_lang_codes: &'static [&'static str],
    pub details: Details,
}

/// Plain-language facts shown on the model card so users know what they are
/// signing up for. Latencies are for a ~4 s utterance on a Ryzen 9 5950X
/// (release build); `latency_measured` marks numbers from `bench_models`.
pub struct Details {
    pub params: &'static str,
    pub architecture: &'static str,
    pub latency_ms: u32,
    pub latency_measured: bool,
    /// < 0 = not measured.
    pub wer_pct: f32,
    pub ram_mb: u32,
    pub best_for: &'static str,
    pub notes: &'static [&'static str],
    pub punctuation: bool,
}

/// The static model catalog.
///
/// `format` drives install handling: `onnx` entries are `.tar.gz` archives that
/// extract to a single directory (renamed to the model id); `ggml` entries are
/// a single `.bin` file placed inside the model directory. URLs + checksums are
/// the publicly published artifacts also used by the reference app (Handy).
pub const CATALOG: &[CatalogEntry] = &[
    CatalogEntry {
        id: "parakeet-tdt-0.6b-v3",
        name: "Parakeet V3",
        description: "⚖️ Balanced — fast and accurate. English + 24 European languages.",
        languages: "25 European languages",
        size_bytes: 478 * 1024 * 1024,
        accuracy: 0.92,
        speed: 0.95,
        format: "onnx",
        supports_translation: false,
        download_url: "https://blob.handy.computer/parakeet-v3-int8.tar.gz",
        sha256: "43d37191602727524a7d8c6da0eef11c4ba24320f5b4730f1a2497befc2efa77",
        // NVIDIA model card: 25 European languages (auto-detected).
        supported_lang_codes: &[
            "bg", "hr", "cs", "da", "nl", "en", "et", "fi", "fr", "de", "el", "hu", "it", "lv",
            "lt", "mt", "pl", "pt", "ro", "sk", "sl", "es", "sv", "ru", "uk",
        ],
        details: Details {
            params: "600M",
            architecture: "NVIDIA Parakeet TDT (FastConformer) · ONNX int8",
            latency_ms: 330,
            latency_measured: true,
            wer_pct: 3.4,
            ram_mb: 700,
            best_for: "Everyday dictation in English or any of 24 European languages. The recommended default.",
            notes: &[
                "Detects the spoken language automatically.",
                "Romanian: 12.4% WER on FLEURS — the best fast option for Romanian.",
                "Adds punctuation and capitals itself.",
                "Can't use Dictionary words as hints — unusual names may come out misspelled.",
            ],
            punctuation: true,
        },
    },
    CatalogEntry {
        id: "parakeet-tdt-0.6b-v2",
        name: "Parakeet V2",
        description: "⚖️ Balanced — English-only. The best model for English speakers.",
        languages: "English Only",
        size_bytes: 473 * 1024 * 1024,
        accuracy: 0.9,
        speed: 0.95,
        format: "onnx",
        supports_translation: false,
        download_url: "https://blob.handy.computer/parakeet-v2-int8.tar.gz",
        sha256: "ac9b9429984dd565b25097337a887bb7f0f8ac393573661c651f0e7d31563991",
        supported_lang_codes: &["en"],
        details: Details {
            params: "600M",
            architecture: "NVIDIA Parakeet TDT (FastConformer) · ONNX int8",
            latency_ms: 330,
            latency_measured: true,
            wer_pct: 3.4,
            ram_mb: 700,
            best_for: "English-only speakers who want top accuracy at near-instant speed.",
            notes: &[
                "English only — other languages come out as garbled English.",
                "Adds punctuation and capitals itself.",
                "Can't use Dictionary words as hints — unusual names may come out misspelled.",
            ],
            punctuation: true,
        },
    },
    CatalogEntry {
        id: "moonshine-base",
        name: "Moonshine Base",
        description: "⚡ Lightning — very fast, English only. Handles accents well.",
        languages: "English Only",
        size_bytes: 55 * 1024 * 1024,
        accuracy: 0.72,
        speed: 0.98,
        format: "onnx",
        supports_translation: false,
        download_url: "https://blob.handy.computer/moonshine-base.tar.gz",
        sha256: "04bf6ab012cfceebd4ac7cf88c1b31d027bbdd3cd704649b692e2e935236b7e8",
        supported_lang_codes: &["en"],
        details: Details {
            params: "61M",
            architecture: "Useful Sensors Moonshine Base · ONNX",
            latency_ms: 150,
            latency_measured: false,
            wer_pct: -1.0,
            ram_mb: 150,
            best_for: "Low-power laptops that need fast English dictation with little memory.",
            notes: &[
                "English only.",
                "Less accurate than Parakeet on long or technical sentences.",
                "Can't use Dictionary words as hints — unusual names may come out misspelled.",
            ],
            punctuation: true,
        },
    },
    CatalogEntry {
        id: "sense-voice-int8",
        name: "SenseVoice",
        description: "⚡ Lightning — fastest model. Chinese, English, Japanese, Korean, Cantonese.",
        languages: "Multi-language",
        size_bytes: 152 * 1024 * 1024,
        accuracy: 0.8,
        speed: 0.99,
        format: "onnx",
        supports_translation: false,
        download_url: "https://blob.handy.computer/sense-voice-int8.tar.gz",
        sha256: "171d611fe5d353a50bbb741b6f3ef42559b1565685684e9aa888ef563ba3e8a4",
        supported_lang_codes: &["en", "zh", "ja", "ko", "yue"],
        details: Details {
            params: "234M",
            architecture: "Alibaba SenseVoice Small (non-autoregressive) · ONNX int8",
            latency_ms: 130,
            latency_measured: true,
            wer_pct: 5.4,
            ram_mb: 300,
            best_for: "Chinese, Japanese, Korean or Cantonese dictation, or the absolute fastest response.",
            notes: &[
                "Handles English, but drops or misspells more words than Parakeet (5.4% vs 3.4% WER).",
                "No European languages besides English.",
                "Can't use Dictionary words as hints — unusual names may come out misspelled.",
            ],
            punctuation: true,
        },
    },
    CatalogEntry {
        id: "giga-am-v3-int8",
        name: "GigaAM v3",
        description: "⚡ Lightning — ultra-fast (~60ms) and highly accurate for Russian & Slavic speech.",
        languages: "Russian",
        size_bytes: 152 * 1024 * 1024,
        accuracy: 0.92,
        speed: 0.99,
        format: "onnx",
        supports_translation: false,
        download_url: "https://blob.handy.computer/giga-am-v3-int8.tar.gz",
        sha256: "d872462268430db140b69b72e0fc4b787b194c1dbe51b58de39444d55b6da45b",
        supported_lang_codes: &["ru"],
        details: Details {
            params: "240M",
            architecture: "Sber GigaAM v3 (Conformer) · ONNX int8",
            latency_ms: 100,
            latency_measured: false,
            wer_pct: -1.0,
            ram_mb: 300,
            best_for: "Russian speakers.",
            notes: &[
                "Russian only — not trained for other languages, including other Slavic ones.",
                "Output may lack punctuation; AI cleanup can add it.",
                "Can't use Dictionary words as hints — unusual names may come out misspelled.",
            ],
            punctuation: false,
        },
    },
    CatalogEntry {
        id: "whisper-small-ro",
        name: "Whisper Small Română",
        description: "🇷🇴 Built for Romanian — Whisper Small fine-tuned on ~555 h of Romanian speech.",
        languages: "Romanian",
        size_bytes: 264_464_607,
        accuracy: 0.85,
        speed: 0.7,
        format: "ggml",
        supports_translation: false,
        download_url: "https://huggingface.co/YarosTN/whisper-small-ro-ggml/resolve/main/ggml-small-ro-q8_0.bin",
        sha256: "aa84f01debb86916108f41f8dd3df18e2126708dc47c523d3958b9c5ba485f66",
        // Fine-tuned for Romanian only; routing sends Romanian dictation here.
        supported_lang_codes: &["ro"],
        details: Details {
            params: "244M",
            architecture: "Whisper Small fine-tune (IonGrozea/whisper-small_ro) · whisper.cpp GGML (q8_0)",
            latency_ms: 2600,
            latency_measured: false,
            wer_pct: -1.0,
            ram_mb: 600,
            best_for: "Romanian dictation with correct diacritics (ă, â, î, ș, ț).",
            notes: &[
                "Romanian only — set Language to Romanian in Settings so it is used.",
                "Author reports 9.3% WER on Romanian (stock Whisper Small: 29.8% on FLEURS).",
                "Same speed as Whisper Small: ~2–3 s per dictation on CPU.",
                "Uses your Dictionary words as spelling hints.",
                "Community model (Apache-2.0), new and not yet widely tested.",
            ],
            punctuation: true,
        },
    },
    CatalogEntry {
        id: "whisper-tiny",
        name: "Whisper Tiny",
        description: "Fastest. Best for low-power machines (older Intel Macs).",
        languages: "Multi-language",
        size_bytes: 78 * 1024 * 1024,
        accuracy: 0.55,
        speed: 0.99,
        format: "ggml",
        supports_translation: true,
        download_url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin",
        sha256: "",
        supported_lang_codes: &[], // all languages
        details: Details {
            params: "39M",
            architecture: "OpenAI Whisper Tiny · whisper.cpp GGML",
            latency_ms: 400,
            latency_measured: false,
            wer_pct: -1.0,
            ram_mb: 125,
            best_for: "Old or very low-power machines (e.g. Intel Macs) that can't run anything bigger.",
            notes: &[
                "Lowest accuracy in the catalog — expect frequent mistakes.",
                "Uses your Dictionary words as spelling hints.",
                "Can invent text on silence; Mumblr filters the common cases.",
            ],
            punctuation: true,
        },
    },
    CatalogEntry {
        id: "whisper-base",
        name: "Whisper Base",
        description: "Light and fast, more accurate than Tiny.",
        languages: "Multi-language",
        size_bytes: 148 * 1024 * 1024,
        accuracy: 0.65,
        speed: 0.92,
        format: "ggml",
        supports_translation: true,
        download_url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin",
        sha256: "",
        supported_lang_codes: &[], // all languages
        details: Details {
            params: "74M",
            architecture: "OpenAI Whisper Base · whisper.cpp GGML",
            latency_ms: 800,
            latency_measured: false,
            wer_pct: -1.0,
            ram_mb: 210,
            best_for: "Low-power machines that need a language Parakeet doesn't cover.",
            notes: &[
                "Modest accuracy, especially outside English.",
                "Uses your Dictionary words as spelling hints.",
            ],
            punctuation: true,
        },
    },
    CatalogEntry {
        id: "whisper-small",
        name: "Whisper Small",
        description: "Fairly accurate, but heavier — slow on low-power CPUs.",
        languages: "Multi-language",
        size_bytes: 487 * 1024 * 1024,
        accuracy: 0.75,
        speed: 0.7,
        format: "ggml",
        supports_translation: true,
        download_url: "https://blob.handy.computer/ggml-small.bin",
        sha256: "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b",
        supported_lang_codes: &[], // all languages
        details: Details {
            params: "244M",
            architecture: "OpenAI Whisper Small · whisper.cpp GGML (f16)",
            latency_ms: 2600,
            latency_measured: true,
            wer_pct: 3.4,
            ram_mb: 850,
            best_for: "Languages Parakeet doesn't cover, when a short wait is fine.",
            notes: &[
                "Takes ~2–3 s per dictation, even for short phrases.",
                "Weak in Romanian (29.8% WER on FLEURS) — use Whisper Small Română instead.",
                "Uses your Dictionary words as spelling hints. Benchmark WER drops to 2.0% with them.",
            ],
            punctuation: true,
        },
    },
    CatalogEntry {
        id: "whisper-medium",
        name: "Whisper Medium",
        description: "Good accuracy. Slow on CPU — best with GPU or fast machines.",
        languages: "Multi-language",
        size_bytes: 539 * 1024 * 1024,
        accuracy: 0.82,
        speed: 0.5,
        format: "ggml",
        supports_translation: true,
        download_url: "https://blob.handy.computer/whisper-medium-q4_1.bin",
        sha256: "79283fc1f9fe12ca3248543fbd54b73292164d8df5a16e095e2bceeaaabddf57",
        supported_lang_codes: &[], // all languages
        details: Details {
            params: "769M",
            architecture: "OpenAI Whisper Medium · whisper.cpp GGML (q4_1)",
            latency_ms: 8000,
            latency_measured: false,
            wer_pct: -1.0,
            ram_mb: 900,
            best_for: "Not usable for live dictation on CPU.",
            notes: &[
                "Mumblr refuses to run it for dictation — too slow on CPU. Pick Small, Turbo or Parakeet instead.",
                "Uses your Dictionary words as spelling hints.",
            ],
            punctuation: true,
        },
    },
    CatalogEntry {
        id: "whisper-large",
        name: "Whisper Large v3",
        description: "Best accuracy, very slow on CPU. For powerful machines only.",
        languages: "Multi-language",
        size_bytes: 1_080 * 1024 * 1024,
        accuracy: 0.9,
        speed: 0.3,
        format: "ggml",
        supports_translation: true,
        download_url: "https://blob.handy.computer/ggml-large-v3-q5_0.bin",
        sha256: "d75795ecff3f83b5faa89d1900604ad8c780abd5739fae406de19f23ecd98ad1",
        supported_lang_codes: &[], // all languages
        details: Details {
            params: "1.55B",
            architecture: "OpenAI Whisper Large v3 · whisper.cpp GGML (q5_0)",
            latency_ms: 20000,
            latency_measured: false,
            wer_pct: -1.0,
            ram_mb: 1500,
            best_for: "Not usable for live dictation on CPU.",
            notes: &[
                "Mumblr refuses to run it for dictation — too slow on CPU. Pick Turbo instead.",
                "Uses your Dictionary words as spelling hints.",
            ],
            punctuation: true,
        },
    },
    CatalogEntry {
        id: "whisper-turbo",
        name: "Whisper Turbo",
        description: "Fast distilled large model (quantized). Good speed/accuracy trade-off.",
        languages: "Multi-language",
        size_bytes: 574 * 1024 * 1024,
        accuracy: 0.87,
        speed: 0.7,
        format: "ggml",
        supports_translation: true,
        download_url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin",
        sha256: "",
        supported_lang_codes: &[], // all languages
        details: Details {
            params: "809M",
            architecture: "OpenAI Whisper Large v3 Turbo · whisper.cpp GGML (q5_0)",
            latency_ms: 13000,
            latency_measured: true,
            wer_pct: 3.4,
            ram_mb: 1000,
            best_for: "The most accurate choice for less common languages, if you can wait.",
            notes: &[
                "Slow on CPU: ~13 s per dictation. Same English accuracy as Parakeet (3.4% WER).",
                "Uses your Dictionary words as spelling hints. Benchmark WER drops to 2.7% with them.",
            ],
            punctuation: true,
        },
    },

];

/// Root directory that holds all installed models.
pub fn models_root(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("models")
}

/// Directory for a specific model id.
pub fn model_dir(app_data_dir: &Path, model_id: &str) -> PathBuf {
    models_root(app_data_dir).join(model_id)
}

/// True if a model directory exists and is non-empty.
pub fn is_installed(app_data_dir: &Path, model_id: &str) -> bool {
    let dir = model_dir(app_data_dir, model_id);
    if !dir.is_dir() {
        return false;
    }
    // For whisper-turbo: auto-purge obsolete unquantized 1.6GB model to prevent CPU hangs
    if model_id == "whisper-turbo" {
        let old_file = dir.join("ggml-large-v3-turbo.bin");
        if old_file.exists() && !dir.join("ggml-large-v3-turbo-q5_0.bin").exists() {
            log::info!("Removing obsolete 1.6GB unquantized whisper-turbo model from {}", old_file.display());
            let _ = std::fs::remove_file(&old_file);
            return false;
        }
    }
    std::fs::read_dir(&dir)
        .map(|mut it| it.next().is_some())
        .unwrap_or(false)
}


/// Model formats the compiled-in engines can load on this target.
/// Intel macOS ships only whisper.cpp (ONNX has no x86_64-darwin prebuilt);
/// every other target compiles both ONNX (Parakeet et al.) and whisper.cpp,
/// so it can load both formats.
pub fn supported_formats() -> &'static [&'static str] {
    if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        &["ggml"]
    } else {
        &["onnx", "ggml"]
    }
}

/// Build the UI-facing catalog (only models the current engine can load),
/// computing `installed` per entry.
pub fn list(app_data_dir: &Path) -> Vec<ModelInfo> {
    let formats = supported_formats();
    CATALOG
        .iter()
        .filter(|e| formats.contains(&e.format))
        .map(|e| ModelInfo {
            id: e.id.to_string(),
            name: e.name.to_string(),
            description: e.description.to_string(),
            languages: e.languages.to_string(),
            size_bytes: e.size_bytes,
            accuracy: e.accuracy,
            speed: e.speed,
            format: e.format.to_string(),
            supports_translation: e.supports_translation,
            installed: is_installed(app_data_dir, e.id),
            language_list: language_codes(e).iter().map(|c| lang_name(c).to_string()).collect(),
            params: e.details.params.to_string(),
            architecture: e.details.architecture.to_string(),
            latency_ms: e.details.latency_ms,
            latency_measured: e.details.latency_measured,
            wer_pct: (e.details.wer_pct >= 0.0).then_some(e.details.wer_pct),
            ram_mb: e.details.ram_mb,
            best_for: e.details.best_for.to_string(),
            notes: e.details.notes.iter().map(|n| n.to_string()).collect(),
            punctuation: e.details.punctuation,
            uses_dictionary: e.format == "ggml",
            dictation_ready: !matches!(e.id, "whisper-medium" | "whisper-large"),
        })
        .collect()
}

/// Every language OpenAI Whisper was trained on (ISO-639-1, Whisper's order).
const WHISPER_LANGS: &[&str] = &[
    "en", "zh", "de", "es", "ru", "ko", "fr", "ja", "pt", "tr", "pl", "ca", "nl", "ar", "sv",
    "it", "id", "hi", "fi", "vi", "he", "uk", "el", "ms", "cs", "ro", "da", "hu", "ta", "no",
    "th", "ur", "hr", "bg", "lt", "la", "mi", "ml", "cy", "sk", "te", "fa", "lv", "bn", "sr",
    "az", "sl", "kn", "et", "mk", "br", "eu", "is", "hy", "ne", "mn", "bs", "kk", "sq", "sw",
    "gl", "mr", "pa", "si", "km", "sn", "yo", "so", "af", "oc", "ka", "be", "tg", "sd", "gu",
    "am", "yi", "lo", "uz", "fo", "ht", "ps", "tk", "nn", "mt", "sa", "lb", "my", "bo", "tl",
    "mg", "as", "tt", "haw", "ln", "ha", "ba", "jw", "su", "yue",
];

/// Languages a catalog entry can transcribe (Whisper's empty list = all 99).
fn language_codes(e: &CatalogEntry) -> &'static [&'static str] {
    if e.supported_lang_codes.is_empty() && e.format == "ggml" {
        WHISPER_LANGS
    } else {
        e.supported_lang_codes
    }
}

/// English display name for an ISO-639-1 code.
pub fn lang_name(code: &str) -> &str {
    match code {
        "af" => "Afrikaans", "am" => "Amharic", "ar" => "Arabic", "as" => "Assamese",
        "az" => "Azerbaijani", "ba" => "Bashkir", "be" => "Belarusian", "bg" => "Bulgarian",
        "bn" => "Bengali", "bo" => "Tibetan", "br" => "Breton", "bs" => "Bosnian",
        "ca" => "Catalan", "cs" => "Czech", "cy" => "Welsh", "da" => "Danish", "de" => "German",
        "el" => "Greek", "en" => "English", "es" => "Spanish", "et" => "Estonian",
        "eu" => "Basque", "fa" => "Persian", "fi" => "Finnish", "fo" => "Faroese",
        "fr" => "French", "gl" => "Galician", "gu" => "Gujarati", "ha" => "Hausa",
        "haw" => "Hawaiian", "he" => "Hebrew", "hi" => "Hindi", "hr" => "Croatian",
        "ht" => "Haitian Creole", "hu" => "Hungarian", "hy" => "Armenian", "id" => "Indonesian",
        "is" => "Icelandic", "it" => "Italian", "ja" => "Japanese", "jw" => "Javanese",
        "ka" => "Georgian", "kk" => "Kazakh", "km" => "Khmer", "kn" => "Kannada",
        "ko" => "Korean", "la" => "Latin", "lb" => "Luxembourgish", "ln" => "Lingala",
        "lo" => "Lao", "lt" => "Lithuanian", "lv" => "Latvian", "mg" => "Malagasy",
        "mi" => "Maori", "mk" => "Macedonian", "ml" => "Malayalam", "mn" => "Mongolian",
        "mr" => "Marathi", "ms" => "Malay", "mt" => "Maltese", "my" => "Burmese",
        "ne" => "Nepali", "nl" => "Dutch", "nn" => "Nynorsk", "no" => "Norwegian",
        "oc" => "Occitan", "pa" => "Punjabi", "pl" => "Polish", "ps" => "Pashto",
        "pt" => "Portuguese", "ro" => "Romanian", "ru" => "Russian", "sa" => "Sanskrit",
        "sd" => "Sindhi", "si" => "Sinhala", "sk" => "Slovak", "sl" => "Slovenian",
        "sn" => "Shona", "so" => "Somali", "sq" => "Albanian", "sr" => "Serbian",
        "su" => "Sundanese", "sv" => "Swedish", "sw" => "Swahili", "ta" => "Tamil",
        "te" => "Telugu", "tg" => "Tajik", "th" => "Thai", "tk" => "Turkmen", "tl" => "Tagalog",
        "tr" => "Turkish", "tt" => "Tatar", "uk" => "Ukrainian", "ur" => "Urdu", "uz" => "Uzbek",
        "vi" => "Vietnamese", "yi" => "Yiddish", "yo" => "Yoruba", "yue" => "Cantonese",
        "zh" => "Chinese",
        other => other,
    }
}

pub fn catalog_entry(model_id: &str) -> Option<&'static CatalogEntry> {
    CATALOG.iter().find(|e| e.id == model_id)
}

/// Pick the model to run for `lang_code`: the user's `active_model_id` when it
/// is installed and supports the language, otherwise the most accurate
/// installed model that does (`None` = auto-detect → multilingual models only).
///
/// Returns `None` if no installed model can handle the language, so the caller
/// should fall back to the user's manually selected model.
pub fn best_model_for_language(
    app_data_dir: &Path,
    active_model_id: &str,
    lang_code: Option<&str>,
) -> Option<&'static str> {
    let formats = supported_formats();

    fn supports(entry: &CatalogEntry, lang: Option<&str>) -> bool {
        // Empty lang_codes → universal (Whisper), supports everything.
        if entry.supported_lang_codes.is_empty() {
            return true;
        }
        match lang {
            // Auto-detect: model must be multilingual (>1 lang).
            None => entry.supported_lang_codes.len() > 1,
            Some(code) => entry.supported_lang_codes.contains(&code),
        }
    }

    let usable = |e: &&CatalogEntry| {
        formats.contains(&e.format)
            // Rejected by `load_engine` as too slow for CPU dictation.
            && !matches!(e.id, "whisper-medium" | "whisper-large")
            && is_installed(app_data_dir, e.id)
            && supports(e, lang_code)
    };

    // The model the user picked wins whenever it can handle the language.
    // Routing used to prefer the *fastest* model, which silently swapped
    // Whisper Turbo / Parakeet for SenseVoice on every English dictation.
    if let Some(active) = CATALOG.iter().find(|e| e.id == active_model_id) {
        if usable(&active) {
            return Some(active.id);
        }
    }

    // Otherwise fall back to the most accurate installed model for the language.
    CATALOG
        .iter()
        .filter(usable)
        .max_by(|a, b| {
            a.accuracy
                .partial_cmp(&b.accuracy)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.speed.partial_cmp(&b.speed).unwrap_or(std::cmp::Ordering::Equal))
        })
        .map(|e| e.id)
}

/// Check if a model supports the given language code.
pub fn is_model_compatible_with_language(model_id: &str, lang_code: Option<&str>) -> bool {
    let Some(code) = lang_code else {
        return true; // Auto-detect mode accepts any model
    };
    if let Some(entry) = catalog_entry(model_id) {
        if entry.supported_lang_codes.is_empty() {
            return true; // Universal (e.g. Whisper)
        }
        return entry.supported_lang_codes.contains(&code);
    }
    true
}

// ---------------------------------------------------------------------------
// Download / install
// ---------------------------------------------------------------------------

/// Progress payload emitted on the `download-progress` event (mirrors
/// `DownloadProgress` in bridge.ts).
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub model_id: String,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub phase: String, // downloading | verifying | extracting | done | error
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn emit(app: &AppHandle, model_id: &str, downloaded: u64, total: u64, phase: &str) {
    let _ = app.emit(
        "download-progress",
        DownloadProgress {
            model_id: model_id.to_string(),
            downloaded_bytes: downloaded,
            total_bytes: total,
            phase: phase.to_string(),
            error: None,
        },
    );
}

/// Emit a terminal error for `model_id` on the download-progress channel.
pub fn emit_error(app: &AppHandle, model_id: &str, error: String) {
    let _ = app.emit(
        "download-progress",
        DownloadProgress {
            model_id: model_id.to_string(),
            downloaded_bytes: 0,
            total_bytes: 0,
            phase: "error".into(),
            error: Some(error),
        },
    );
}

fn to_hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// Download, verify (SHA-256), and install the model identified by `model_id`.
/// Emits `download-progress` events throughout. Blocking — run on a worker
/// thread (see the `download_model` command).
pub fn install(app: &AppHandle, model_id: &str, app_data_dir: &Path) -> Result<(), String> {
    let entry = catalog_entry(model_id).ok_or_else(|| format!("unknown model '{model_id}'"))?;
    if entry.download_url.is_empty() {
        return Err(format!("no download URL for '{model_id}'"));
    }

    let root = models_root(app_data_dir);
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let tmp = root.join(format!("{model_id}.download"));

    // --- download (streamed) ---
    let client = reqwest::blocking::Client::builder()
        .build()
        .map_err(|e| e.to_string())?;
    let mut resp = client.get(entry.download_url).send().map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("download failed: HTTP {}", resp.status()));
    }
    let total = resp.content_length().unwrap_or(entry.size_bytes);

    let mut file = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut downloaded: u64 = 0;
    let mut buf = vec![0u8; 1 << 16];
    let mut last = std::time::Instant::now();
    loop {
        let n = resp.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        hasher.update(&buf[..n]);
        downloaded += n as u64;
        if last.elapsed().as_millis() > 200 {
            emit(app, model_id, downloaded, total, "downloading");
            last = std::time::Instant::now();
        }
    }
    file.flush().ok();
    drop(file);

    // --- verify ---
    emit(app, model_id, downloaded, total, "verifying");
    let digest = to_hex(&hasher.finalize());
    if !entry.sha256.is_empty() && digest != entry.sha256 {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("checksum mismatch for '{model_id}'"));
    }

    // --- install by format ---
    let dest = model_dir(app_data_dir, model_id);
    if dest.exists() {
        std::fs::remove_dir_all(&dest).ok();
    }
    match entry.format {
        "onnx" => {
            emit(app, model_id, downloaded, total, "extracting");
            extract_archive(&tmp, &root, &dest)?;
        }
        "ggml" => {
            std::fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
            let filename = entry.download_url.rsplit('/').next().unwrap_or("model.bin");
            move_file(&tmp, &dest.join(filename))?;
        }
        other => return Err(format!("unsupported model format '{other}'")),
    }
    let _ = std::fs::remove_file(&tmp);

    emit(app, model_id, total, total, "done");
    Ok(())
}

/// Extract a `.tar.gz` or `.tar.bz2` archive into a staging dir, then place its contents at `dest`.
/// Handles archives that wrap everything in a single top-level directory or
/// contain macOS metadata files (like ._* and .DS_Store).
fn extract_archive(archive: &Path, root: &Path, dest: &Path) -> Result<(), String> {
    let staging = root.join(".staging");
    if staging.exists() {
        std::fs::remove_dir_all(&staging).ok();
    }
    std::fs::create_dir_all(&staging).map_err(|e| e.to_string())?;

    let f = std::fs::File::open(archive).map_err(|e| e.to_string())?;
    let gz = flate2::read::GzDecoder::new(f);
    tar::Archive::new(gz).unpack(&staging).map_err(|e| e.to_string())?;

    // Filter out AppleDouble metadata files and system artifacts.
    let real_entries: Vec<_> = std::fs::read_dir(&staging)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            !name.starts_with("._") && name != ".DS_Store" && name != "__MACOSX"
        })
        .collect();

    if real_entries.len() == 1 && real_entries[0].path().is_dir() {
        std::fs::rename(real_entries[0].path(), dest).map_err(|e| e.to_string())?;
        std::fs::remove_dir_all(&staging).ok();
    } else {
        std::fs::rename(&staging, dest).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Find the directory where actual model files (.onnx / vocab.txt / .bin) live.
/// If `dir` contains a subdirectory with the model files, returns that subdirectory.
pub fn find_model_dir(dir: &Path) -> PathBuf {
    if !dir.exists() || !dir.is_dir() {
        return dir.to_path_buf();
    }
    // Check if the dir directly contains model files (.onnx or .bin)
    let has_model_files = std::fs::read_dir(dir)
        .ok()
        .map(|entries| {
            entries.filter_map(|e| e.ok()).any(|e| {
                let p = e.path();
                let name = e.file_name().to_string_lossy().to_string();
                if name.starts_with("._") || name.starts_with('.') {
                    return false;
                }
                p.extension().map(|ext| ext == "onnx" || ext == "bin").unwrap_or(false)
                    || name == "tokens.txt"
            })
        })
        .unwrap_or(false);

    if has_model_files {
        return dir.to_path_buf();
    }

    // Look one level deeper for a subdirectory containing model files
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let p = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if p.is_dir() && !name.starts_with("._") && !name.starts_with('.') && name != "__MACOSX" {
                if let Ok(sub_entries) = std::fs::read_dir(&p) {
                    let sub_has_models = sub_entries.filter_map(|e| e.ok()).any(|e| {
                        let sub_p = e.path();
                        let sub_name = e.file_name().to_string_lossy().to_string();
                        if sub_name.starts_with("._") || sub_name.starts_with('.') {
                            return false;
                        }
                        sub_p.extension().map(|ext| ext == "onnx" || ext == "bin").unwrap_or(false)
                    });
                    if sub_has_models {
                        return p;
                    }
                }
            }
        }
    }

    dir.to_path_buf()
}

/// Move a file, falling back to copy+delete across volumes.
fn move_file(src: &Path, dst: &Path) -> Result<(), String> {
    if std::fs::rename(src, dst).is_ok() {
        return Ok(());
    }
    std::fs::copy(src, dst).map_err(|e| e.to_string())?;
    std::fs::remove_file(src).ok();
    Ok(())
}

