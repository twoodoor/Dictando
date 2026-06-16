# Project rebuild kickoff: Tauri pivot, docs & devlog scaffolding

- **Date:** 2026-06-16
- **Phase:** 0 — Docs + memory scaffolding
- **Status:** done

## What changed
- Added `CLAUDE.md` as the single source of truth agent guide; added `GEMINI.md` and `AGENTS.md` as synced pointer files.
- Created this dev log system under `docs/devlog/` (index + this entry + template).
- No application code changed yet.

## Why
The user wants Dictando upgraded to a fast, accurate, cross-platform (Windows + Mac) dictation app that rivals Wispr Flow and the open-source reference app *Handy*. Today Dictando is a React PWA + Express/Firebase cloud app plus a **Windows-only Python/PyWebView wrapper** (`windows_app/dictando.py`) using `faster-whisper` with only `tiny.en`/`base` models. That can't deliver fast local models cleanly and has no Mac story.

## Decisions & rationale (locked with user)
1. **Architecture → fresh Tauri app (Rust) reusing the existing React UI**, with the `transcribe-rs` crate as the model engine. Chosen over forking Handy (loses Dictando's UI/brand) and over improving the Python wrapper (stays slow, Windows-centric). Tauri produces native signed `.msi`/`.exe` and `.dmg`/`.app`.
2. **Data model → hybrid local-first.** Transcription + history fully on-device/offline; Firebase login optional, only for cross-device sync.
3. **AI layer → cloud LLM (Gemini), toggleable.** Reuses the existing `@google/genai` integration; with the toggle off, transcription is 100% offline.
4. **Scope → phased roadmap** (Phases 0–5). Full plan: `~/.claude/plans/i-would-like-to-ticklish-sutton.md`.

### Research grounding
- Reference app *Handy* (MIT, ~23k★) is Tauri (Rust + React); its model lineup (Parakeet V3, Canary, Moonshine, SenseVoice, GigaAM, Whisper) comes from the `transcribe-rs` crate with `features = ["onnx", "whisper-cpp"]`.
- Default model chosen: **Parakeet TDT 0.6B v3 (int8 ONNX)** — 25 languages, auto-detect, ~4.8 avg WER, up to ~30× realtime on CPU.
- Models download from blob endpoints into the Tauri app-data `models/` dir.

## Follow-ups / open questions
- **Security:** `windows_app/dictando.py:32-33` contains a hardcoded Google OAuth client secret. Must be rotated and must NOT be carried into the Tauri app.
- `package.json` `"name"` is still `"react-example"`; `clean` script is Unix-only (`rm -rf`) — switch to `rimraf`.
- Confirm exact model download URLs/checksums when building `models.rs` (Phase 2).
