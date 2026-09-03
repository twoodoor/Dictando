# Transcription & Overlay Stability Hardening (v0.3.11 → v0.3.15)

- **Date:** 2026-08-31
- **Phase:** Transcription engine / stability
- **Status:** done

## What changed

- **v0.3.11 — permanent WebView2 cache fix ([lib.rs](../../src-tauri/src/lib.rs)):** WebView2 locks its `Cache`/`Code Cache` directories at process start, so the old `bust_webview2_cache_if_stale()` couldn't delete them even when run before `Builder::default()`. Fixed by building the overlay window programmatically with a version-unique URL (`tauri://localhost/?v=0.3.11`) instead of declaring it in `tauri.conf.json`. WebView2 caches by URL, so every release now uses a URL it's never seen — guaranteed-fresh content, no file deletion required. The early cache-bust stays as a best-effort secondary defense.
- **Freeze on non-English audio + Romanian routing ([transcription.rs](../../src-tauri/src/transcription.rs), [models.rs](../../src-tauri/src/models.rs)):** unified ONNX and Whisper inference onto the same off-mutex background-thread path with a safety timeout (15s Whisper / 10s ONNX), so a stuck inference call can no longer hold the app hostage. Also fixed Parakeet V3's language metadata — the ONNX weights are actually English-only BPE, not the 25-language multilingual set the catalog claimed — so Romanian and other European languages now correctly route to Whisper Turbo/Base/Small instead of silently hanging on Parakeet.
- **Panic guard + language-incompatibility check ([lib.rs](../../src-tauri/src/lib.rs), [transcription.rs](../../src-tauri/src/transcription.rs), [models.rs](../../src-tauri/src/models.rs)):** added `models::is_model_compatible_with_language()`, checked before loading/running inference, so picking an English-only model with a non-English language now emits a friendly `transcription-error` toast instead of crashing. Wrapped the inference thread body in `std::panic::catch_unwind`, and added a <100ms audio-length floor (nothing to transcribe below that). Frontend (`bridge.ts`, `App.tsx`) now listens for `onTranscriptionError` and surfaces it as a toast.
- **`macOSPrivateApi` everywhere ([Cargo.toml](../../src-tauri/Cargo.toml), [tauri.conf.json](../../src-tauri/tauri.conf.json)):** enabled the `macos-private-api` Cargo feature and `macOSPrivateApi: true` config flag across all build targets, not just Intel macOS — required for the transparent recording-overlay window to render correctly everywhere.
- **v0.3.15 — WhisperState isolation + audio callback panic guard ([transcription.rs](../../src-tauri/src/transcription.rs), [audio.rs](../../src-tauri/src/audio.rs)):** `DirectWhisperEngine` no longer keeps one long-lived `WhisperState`; it now builds a fresh `WhisperState` per `transcribe()` call so KV-cache/encoder memory can never carry over between utterances or across a language switch. The `cpal` audio push callback (running on the real-time WASAPI thread) is now wrapped in `catch_unwind` so a panic inside resampling/RMS computation can't take down the whole audio stream.

## Why

Users were hitting hangs and hard crashes with non-English audio (esp. Romanian) and after Windows app updates (stale WebView2-cached overlay). Root causes were: Parakeet's language catalog overpromising support it didn't have, a single long-lived mutex-guarded engine state shared across calls/languages, and no panic isolation on either the inference thread or the real-time audio callback — any one of these could freeze or crash the whole app instead of failing one transcription.

## Decisions & rationale

- Chose a versioned overlay URL over deleting WebView2 cache files because the cache directories are locked by the OS at process start — deletion is fundamentally racy, while a new URL per version is deterministic and needs no filesystem access.
- Kept the mutex-free "move engine to background thread, send back over a channel" pattern for both engines (previously Whisper-only) rather than adding per-engine special-casing, so timeout/panic handling is uniform.
- Fixed the model catalog's language claims rather than adding a translation/fallback layer — the honest fix is not routing users to a model that can't do the job.

## Follow-ups / open questions

- No devlog entry existed for this stretch (5 commits, Aug 27–31) until now — should be captured closer to when the fix lands next time.
- Phase B (two-phase paste, hotkey conflict UI, overlay latency) is still pending per `CLAUDE.md`.
