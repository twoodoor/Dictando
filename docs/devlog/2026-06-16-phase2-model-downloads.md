# Phase 2: Model catalog + download manager

- **Date:** 2026-06-16
- **Phase:** 2 — Model catalog + download manager
- **Status:** code-complete & build-verified (live download test pending user-present run)

## What changed
- `models.rs`: real catalog with published download URLs + SHA-256 checksums (Parakeet v3/v2, Moonshine Base, Whisper Small/Turbo). New `install()` — streamed `reqwest` download with `download-progress` events, SHA-256 verification, and format-aware install: `onnx` → `.tar.gz` extracted (single top-dir unwrapped → `<app_data>/models/<id>/`); `ggml` → `.bin` placed in the model dir. Added `DownloadProgress` payload + `emit_error`.
- `lib.rs`: `download_model` now runs `install()` on a worker thread; progress/errors flow over the `download-progress` event.
- New `src/components/ModelsView.tsx`: model grid matching the reference UI — accuracy/speed meters, language badge, size, download-with-progress, delete, set-active, language filter. Wired to `backend.listModels/downloadModel/deleteModel/setActiveModel` + `events.onDownloadProgress`.
- `App.tsx`: added a native-only **Models** tab.
- New deps: `reqwest` (blocking), `flate2`, `tar`, `sha2`.

## Why
Users need to browse and install fast/accurate local models. This delivers the catalog + downloader so Parakeet v3 (and others) can be fetched into the app-data `models/` dir and selected as active.

## Decisions & rationale
- **Reused the reference app's published model artifacts** (`blob.handy.computer`) with their SHA-256 checksums — same ONNX/GGML formats `transcribe-rs` loads, so no re-hosting needed for now. (Self-hosting on a Dictando endpoint is a Phase 5 option.)
- **Blocking `reqwest` on a worker thread** with manual chunked reads — simple, gives byte-level progress, no async runtime plumbing in the command.
- Checksum verification is mandatory when a hash is present; mismatch deletes the partial download and errors.

## Verification
- `cargo build` exit 0; `npm run lint` clean; `npm run build` green.
- **Deferred (needs user-present run):** launch `tauri dev`, download Parakeet v3 (~478 MB) from the Models tab, confirm extraction + checksum + active-model selection, then dictate end-to-end (mic → transcription → paste).

## Follow-ups
- Phase 3: local SQLite history (`history.rs`) so the History tab is real natively + transcriptions persist; full settings surface (overlay, tray, launch-on-startup, custom words, auto-delete); audio playback.
- Whisper/GGML loading needs the `transcribe-rs` `whisper-cpp` feature wired in `transcription.rs` (currently Parakeet/ONNX only) before Whisper entries are usable.
