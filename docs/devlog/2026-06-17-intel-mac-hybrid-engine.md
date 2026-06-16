# Intel macOS support via hybrid engine (Parakeet / Whisper)

- **Date:** 2026-06-17
- **Phase:** Deployment follow-up
- **Status:** code-complete; Windows path verified; Intel-mac builds via CI

## Context
The `v0.1.0` CI failed on macOS: `ort-sys` (ONNX Runtime, via `transcribe-rs`) ships **no `x86_64-apple-darwin` prebuilt binary**, and `transcribe-rs` forces `ort`'s default `download-binaries` (can't be disabled without forking). So an Intel-macOS build that includes Parakeet **cannot compile**. The user needs Intel support (their Mac is an Intel i7) and wants all editions to work.

## Decision (with user)
**Hybrid engine, chosen per target:** keep fast **Parakeet/ONNX** on Windows + Apple-Silicon macOS; use **Whisper/whisper.cpp** (compiles from source, no prebuilt-binary issue) on **Intel macOS**.

## What changed
- **`Cargo.toml`**: `transcribe-rs` is now target-specific — `features=["whisper-cpp"]` for `cfg(all(macos, x86_64))`, `features=["onnx"]` everywhere else.
- **`transcription.rs`**: engine selected via `cfg` — Parakeet (`onnx::parakeet::ParakeetModel`, loads a dir) vs Whisper (`whisper_cpp::WhisperEngine`, loads a single `.bin`). Shared load/transcribe/idle-unload/hallucination logic; only the load + `transcribe_with` calls differ.
- **`models.rs`**: `engine_format()` + `list()` now returns only models the compiled engine can load (ggml on Intel-mac, onnx elsewhere).
- **`settings.rs`**: `default_model_id()` → `whisper-small` on Intel-mac, `parakeet-tdt-0.6b-v3` elsewhere.
- **`lib.rs`** setup: if a saved active model's format doesn't match this target's engine, fall back to the default (handles cross-engine settings).
- **CI** (`release.yml`): macOS matrix now builds **both** `aarch64-apple-darwin` (Parakeet) and `x86_64-apple-darwin` (Whisper); Windows unchanged.

## Verification
- `cargo build` (Windows/Parakeet path) exit 0 — hybrid `cfg` doesn't break the main path.
- **Can't build whisper.cpp locally** (no `cmake` on the Windows dev box) → the Intel-mac/Whisper path is validated by the macOS CI runner (has cmake). Earlier `v0.1.0` blockers (npm-ci lockfile, ort universal) already fixed.

## Notes / follow-ups
- Intel-mac users get Whisper models (Small/Turbo) instead of Parakeet — slightly slower, still solid.
- Cross-compiling `x86_64` from the Apple-Silicon runner is the one remaining unknown to confirm in CI; if whisper.cpp cross-build is fussy, fallback is a dedicated `macos-13` (Intel) runner.
