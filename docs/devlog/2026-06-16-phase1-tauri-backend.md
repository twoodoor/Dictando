# Phase 1: Tauri backend — capture → transcribe → inject

- **Date:** 2026-06-16
- **Phase:** 1 — Tauri foundation + transcription parity
- **Status:** code-complete & build-verified (interactive runtime test pending Phase 2 model)

## What changed
- Installed toolchain: rustup (cargo/rustc 1.96.0) + Visual Studio Build Tools 2022 (C++/VC Tools) via winget. WebView2 already present.
- Scaffolded `src-tauri/` with the Tauri CLI (v2.11.2). App id `com.dictando.app`, product name Dictando.
- `package.json`: renamed `react-example` → `dictando`, added `tauri`/`tauri:dev`/`tauri:build` scripts, `@tauri-apps/api` dep, `rimraf` for cross-platform `clean`.
- New Rust modules:
  - `settings.rs` — `AppSettings` (serde camelCase, mirrors `bridge.ts`) + `SettingsStore` (JSON at `<app_data>/settings.json`, partial-patch merge).
  - `models.rs` — static model catalog (Parakeet v3 default, v2, Whisper small/turbo) + on-disk path resolution `<app_data>/models/<id>/`. Download URLs/sha256 fields reserved for Phase 2.
  - `transcription.rs` — `Transcriber` wrapping `transcribe_rs::onnx::parakeet::ParakeetModel` (Int8); lazy load, idle-unload timer, ported hallucination filter.
  - `audio.rs` — `Recorder` using `cpal` on a dedicated thread (Stream is !Send on WASAPI); downmix to mono + linear resample to 16 kHz f32.
  - `inject.rs` — `enigo` (direct typing) + `arboard` (clipboard paste, optional restore); Ctrl/Cmd+V per-OS.
  - `shortcuts.rs` — parse `KeyboardEvent.code[]` → `Shortcut`; (re)register via `tauri-plugin-global-shortcut`.
  - `lib.rs` — `AppState`, push-to-talk + toggle pipeline (global shortcut → record → transcribe on a worker thread → inject → emit `transcription`/`recording-state` events), and all Tauri commands.
- New frontend `src/lib/bridge.ts` — typed IPC abstraction (commands + events) that no-ops/throws `NOT_NATIVE` on web so the same React code runs in both.

## Why
Core pivot from the Python/PyWebView wrapper to a native Tauri app with fast local models. This module set reaches functional parity with `windows_app/dictando.py` (hotkey → local STT → paste) on a native, cross-platform base.

## Decisions & rationale
- **Default model Parakeet v3 (ONNX/Int8)** via `transcribe-rs` — verified the ONNX engine (`ort 2.0.0-rc.12`) compiles cleanly on Windows before building around it.
- **Linear resampler** in `audio.rs` for Phase 1 instead of pulling in `rubato` — fewer deps, adequate for 16 kHz speech. Flagged for a quality upgrade later.
- **Mic selection** uses cpal device *names* as ids natively (web MediaDevices ids don't map to cpal); default device used when unset.
- History/download commands are **stubbed** (return empty / not-implemented) so the frontend contract is complete now; real impls land in Phases 2–3.

## Frontend adaptation (done)
- `App.tsx`: Firebase login is **optional** in native mode (`isNative`) — the app renders local-first without a user; transcription events optionally sync to Firestore when logged in.
- `RecordView`/`SettingsView`/`HistoryView`: now accept `User | null`, branch native vs web, and use `bridge.ts` natively (status/events, `getSettings`/`updateSettings`, `listMicrophones`, `listHistory`). Web/Firebase path preserved.
- Verified: `npm run lint` (tsc) clean; `npm run build` produces `dist/` (Tauri's frontend) with only benign warnings; `cargo build` green.

## Follow-ups / open questions
- Phase 2: fill `models.rs` download URLs + checksums; implement streamed download/extract/verify.
- Manual runtime verification still required: real mic capture, model download, and paste into a target app (`npm run tauri:dev`).
- `whisper-cpp` feature deferred (Whisper family) — add when wiring the Whisper catalog entries.
