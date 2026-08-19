# Model Inference Optimizations & Whisper Fixes (v0.3.1)

- **Date:** 2026-08-19
- **Phase:** Engine & Release (v0.3.1)
- **Status:** done

## What changed
- Added direct `whisper-rs` integration in `src-tauri/src/transcription.rs` replacing slow beam-search decoding with high-speed multi-threaded greedy decoding (`SamplingStrategy::Greedy { best_of: 1 }`).
- Configured dynamic thread utilization via `std::thread::available_parallelism()` across all CPU cores for Whisper inference.
- Implemented `find_model_dir` in `src-tauri/src/models.rs` to auto-heal models extracted into nested subdirectories (such as Parakeet V2 and other `.tar.gz` archives with AppleDouble metadata).
- Updated `extract_targz` to filter out macOS `._*` AppleDouble and `.DS_Store` files to prevent directory nesting during model installation.
- Propagated user-selected language settings (`language_to_code`) to the transcription engine for all models.
- Bumped version to `0.3.1` across `package.json`, `src-tauri/tauri.conf.json`, and `src-tauri/Cargo.toml`.

## Why
Whisper models previously hung the app due to 3-beam search decoding on CPU taking 20–40s per transcription, locking the transcriber mutex. Furthermore, tar archive metadata caused model directories to nest, preventing Parakeet V2 and other models from loading.

## Decisions & rationale
- Direct `whisper-rs` wrapper with Greedy sampling reduces CPU latency by 5–10× while eliminating multi-temperature stall loops.
- Auto-healing directory resolver seamlessly supports both existing installed models and newly downloaded models.
