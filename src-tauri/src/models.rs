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
        description: "Fast and accurate. 25 languages with auto-detection.",
        languages: "Multi-language",
        size_bytes: 478 * 1024 * 1024,
        accuracy: 0.92,
        speed: 0.95,
        format: "onnx",
        supports_translation: false,
        download_url: "https://blob.handy.computer/parakeet-v3-int8.tar.gz",
        sha256: "43d37191602727524a7d8c6da0eef11c4ba24320f5b4730f1a2497befc2efa77",
    },
    CatalogEntry {
        id: "parakeet-tdt-0.6b-v2",
        name: "Parakeet V2",
        description: "English-only. The best model for English speakers.",
        languages: "English Only",
        size_bytes: 473 * 1024 * 1024,
        accuracy: 0.9,
        speed: 0.95,
        format: "onnx",
        supports_translation: false,
        download_url: "https://blob.handy.computer/parakeet-v2-int8.tar.gz",
        sha256: "ac9b9429984dd565b25097337a887bb7f0f8ac393573661c651f0e7d31563991",
    },
    CatalogEntry {
        id: "moonshine-base",
        name: "Moonshine Base",
        description: "Very fast, English only. Handles accents well.",
        languages: "English Only",
        size_bytes: 55 * 1024 * 1024,
        accuracy: 0.72,
        speed: 0.98,
        format: "onnx",
        supports_translation: false,
        download_url: "https://blob.handy.computer/moonshine-base.tar.gz",
        sha256: "04bf6ab012cfceebd4ac7cf88c1b31d027bbdd3cd704649b692e2e935236b7e8",
    },
    CatalogEntry {
        id: "sense-voice-int8",
        name: "SenseVoice",
        description: "Very fast. Chinese, English, Japanese, Korean, Cantonese.",
        languages: "Multi-language",
        size_bytes: 152 * 1024 * 1024,
        accuracy: 0.8,
        speed: 0.95,
        format: "onnx",
        supports_translation: false,
        download_url: "https://blob.handy.computer/sense-voice-int8.tar.gz",
        sha256: "171d611fe5d353a50bbb741b6f3ef42559b1565685684e9aa888ef563ba3e8a4",
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
        // sha256 intentionally empty → download verification is skipped (Hugging Face source).
        download_url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin",
        sha256: "",
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
    dir.is_dir()
        && std::fs::read_dir(&dir)
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
        })
        .collect()
}

pub fn catalog_entry(model_id: &str) -> Option<&'static CatalogEntry> {
    CATALOG.iter().find(|e| e.id == model_id)
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
            extract_targz(&tmp, &root, &dest)?;
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

/// Extract a `.tar.gz` into a staging dir, then place its contents at `dest`.
/// Handles archives that wrap everything in a single top-level directory or
/// contain macOS metadata files (like ._* and .DS_Store).
fn extract_targz(archive: &Path, root: &Path, dest: &Path) -> Result<(), String> {
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

