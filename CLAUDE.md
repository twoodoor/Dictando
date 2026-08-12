# Mumblr — Agent Guide (CLAUDE.md)

> **Single source of truth.** `GEMINI.md` and `AGENTS.md` are kept in sync with this file. Edit `CLAUDE.md`, then mirror the change into the other two.

## What Mumblr is

Mumblr (formerly Dictando) is a fast, accurate, multilingual **speech-to-text / dictation app**. The goal is to rival Wispr Flow and the open-source reference app *Handy*: instant local transcription, a downloadable catalog of fast/accurate models, and an optional AI layer that formats and edits dictated text.

It is being rebuilt from a Python/PyWebView wrapper into a **native Tauri desktop app** (Windows + macOS) that reuses the existing React UI. See `docs/devlog/` for the running change log and `C:\Users\Panoptikon\.claude\plans\i-would-like-to-ticklish-sutton.md` for the full roadmap.

## Architecture (target)

| Layer | Tech | Location |
|---|---|---|
| Desktop shell | **Tauri** (Rust) | `src-tauri/` *(being built)* |
| Transcription engine | **`transcribe-rs`** crate — hybrid: ONNX (Parakeet/Moonshine/SenseVoice) **and** whisper.cpp (OpenAI Whisper GGML) on every target except Intel macOS, which is whisper.cpp-only | `src-tauri/src/transcription.rs` |
| Audio capture | `cpal` + `rubato` (16 kHz mono) | `src-tauri/src/audio.rs` |
| VAD | `vad-rs` (Silero) | `src-tauri/src/vad.rs` |
| Global shortcuts / push-to-talk | `rdev` / `tauri-plugin-global-shortcut` | `src-tauri/src/shortcuts.rs` |
| Text injection | `enigo` + clipboard | `src-tauri/src/inject.rs` |
| AI enhancement (opt-in) | Gemini (`@google/genai`) | `src-tauri/src/ai.rs` |
| Local history | SQLite | `src-tauri/src/history.rs` |
| Sound cues (opt-in) | Synthesized water-drop chimes via `rodio` | `src-tauri/src/sound.rs` |
| UI | **React 19 + TypeScript + Tailwind v4 + Motion + Lucide + Sonner** | `src/` |
| Web/PWA + optional sync | Vite PWA, Express (`server.js`), Firebase (auth + Firestore) | root + `src/firebase.ts` |

**Data model:** local-first. Transcription and history are fully on-device/offline. Firebase login is **optional**, used only for cross-device history/settings sync. The AI enhancement layer is **toggleable** — with it off, transcription makes zero network calls.

The React UI talks to Rust via Tauri `invoke`/events. A thin `src/lib/bridge.ts` abstracts IPC so the same components run in the native app and degrade gracefully on web/PWA.

### Default model

**Parakeet TDT 0.6B v3 (int8 ONNX)** — 25 languages, auto language detection, ~4.8 avg WER, up to ~30× realtime on CPU. Models download from blob endpoints into the Tauri app-data `models/` dir. Catalog defined in `src-tauri/src/models.rs`.

## Repository map

```
src/                      React/TS UI (App.tsx, components/{Record,History,Settings,Login}View.tsx)
  firebase.ts             Firebase init (auth + Firestore) — optional sync
src-tauri/                Rust/Tauri backend (under construction)
server.js                 Express server for Cloud Run / PWA hosting
firebase.json, .firebaserc, firestore.*   Firebase hosting + rules
vite.config.ts            Vite + PWA config
windows_app/dictando.py   DEPRECATED Python/PyWebView app (faster-whisper). Retired after Tauri parity.
docs/devlog/              Running change log (see README.md index)
CLAUDE.md / GEMINI.md / AGENTS.md   This guide (kept in sync)
```

## Build & run

```bash
npm install                # JS deps
npm run dev                # web/PWA dev server (Vite, port 3000)
npm run build              # build React frontend to dist/
npm run lint               # type-check (tsc --noEmit)
npm start                  # serve built dist/ via Express (Cloud Run)

# Native app:
npm run tauri dev          # native dev window (Vite on :1420, strictPort)
npm run tauri build        # installers in src-tauri/target/release/bundle/
```

Installers: Windows `.msi`/`.exe` build locally; macOS `.dmg`/`.app` build via
`.github/workflows/release.yml` (tag `v*`) on a Mac runner — Tauri can't
cross-compile macOS from Windows. Full guide: `docs/deployment.md`.

Rust side: `cargo build` / `cargo clippy` inside `src-tauri/`.

> **Build prerequisites (whisper-cpp):** the `whisper-cpp` feature compiles whisper.cpp from C++ source and generates FFI bindings with `bindgen`, so every build host (dev + CI) needs **two tools on top of the MSVC toolchain Tauri already requires on Windows**:
> 1. **CMake** on `PATH` — `winget install Kitware.CMake` / `brew install cmake` / distro package.
> 2. **LLVM/libclang** (bindgen dependency) — `winget install LLVM.LLVM` (Windows) / `brew install llvm` (macOS) / `libclang-dev` (Linux). On Windows, point bindgen at it: `LIBCLANG_PATH=C:\Program Files\LLVM\bin`.

> **Platform note:** dev environment is Windows + PowerShell. The `clean` script uses `rm -rf` (Unix-only) — prefer `rimraf` for cross-platform.

## Conventions

- **Docs sync:** any architectural change → update `CLAUDE.md` and mirror to `GEMINI.md`/`AGENTS.md`, and append a dated entry to `docs/devlog/`.
- UI: Tailwind utility classes, dark theme (zinc-950 bg, blue-500 accent, red-500 recording). Match existing components.
- Keep raw transcription offline; never route audio/text through the network unless the AI layer is explicitly enabled.

## Build status (2026-06-16)

- Phases 0–3a **done and runtime-verified**: `npm run tauri dev` runs the native app; Parakeet v3 downloads from the Models tab and dictation works end-to-end (~150–250 ms transcriptions) — shortcut → record → transcribe (Parakeet/ONNX) → paste, with local SQLite history.
- **UI redesigned** (Wispr-inspired): full-window app shell — slim custom titlebar (`decorations: false`) + left `Sidebar` (Dictate · Models · History · Dictionary · Settings), light default + dark/system theme via CSS-variable tokens in `index.css` + `lib/theme.ts`, violet accent, serif display headings. Dev port is **1420** (`dev:tauri` script, strictPort).
- In-app hotkey recorder finalizes on the main (non-modifier) key — reliably captures combos like Ctrl+Space.
- **Phase 3b/4 done:** system tray (Show/Quit), close→minimize (X minimizes so the hotkey keeps working), launch-on-startup (`tauri-plugin-autostart`), always-on-top recording overlay HUD (`overlay` window, `#overlay` route → `Overlay.tsx`), and the opt-in **Gemini AI cleanup** layer (`ai.rs`, toggle + local key in Settings; off by default = fully offline).
- **Sound cues (opt-in):** discreet start/finish water-drop chimes, **synthesized at startup** (no asset files) and played via `rodio` on a dedicated thread (`sound.rs`) — start = rising drop, finish = resolving two-note drop. Gated on the `audioFeedback` setting (Settings → Audio; off by default); fired from `begin_recording`/`finish_recording`. Tuning dials are the four constants per drop in `render_start`/`render_finish`.
- **Hybrid engine on all platforms:** whisper.cpp is now compiled alongside ONNX on Windows/Apple-Silicon/Linux (not just Intel macOS), so the six OpenAI Whisper GGML models (`whisper-tiny`…`whisper-turbo`) appear in the Models tab and load everywhere. `models::supported_formats()` gates the catalog; `transcription.rs` dispatches `whisper*` ids to `WhisperEngine`. Parakeet v3 stays the non-Intel default. Requires CMake at build time (see Build & run).

## Known issues / tech debt

- **Hardcoded Google OAuth client secret** in `windows_app/dictando.py:32-33` — must not be carried into the Tauri app; rotate and remove.
- `audio.rs` uses a linear resampler (adequate for 16 kHz speech); consider `rubato` for higher quality.
