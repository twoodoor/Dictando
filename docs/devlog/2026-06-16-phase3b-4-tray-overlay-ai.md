# Phase 3b + 4: tray, autostart, overlay HUD, AI enhancement

- **Date:** 2026-06-16
- **Phase:** 3b (background/UX) + 4 (AI layer)
- **Status:** done & build-verified (running in `tauri dev`)

## What changed
**Background / window UX (3b)**
- **Close → minimize:** the titlebar X now minimizes to the taskbar so the global hotkey keeps working; a real Quit lives in the tray.
- **System tray** (`tauri` `tray-icon` feature): menu with *Show Dictando* / *Quit Dictando*; left-click shows the main window. Built in `lib.rs` setup.
- **Launch on startup:** `tauri-plugin-autostart` wired to the `launchOnStartup` setting (`sync_autostart` applies on boot and on settings change).
- **Recording overlay HUD:** a second always-on-top, transparent, borderless `overlay` window (`tauri.conf.json`) loading `index.html#overlay`. `main.tsx` routes the hash to `Overlay.tsx` (Wispr-style "Listening…" pill). Rust shows/hides + bottom-center positions it on recording-state transitions (`update_overlay`/`position_overlay`).

**AI enhancement (4)**
- `ai.rs`: opt-in Gemini (`gemini-2.0-flash`) cleanup — punctuation/capitalization, filler removal, spoken self-correction resolution; preserves custom-dictionary spellings; never adds/translates/summarizes. 15s timeout.
- Applied in `finish_recording` **only when** `aiEnhanceEnabled` and a key is set; falls back to raw text on any failure. Off by default → transcription stays 100% offline.
- Settings UI: "App" (launch-on-startup) and "AI enhancement" (toggle + local Gemini key) sections.

## Decisions & rationale
- Overlay implemented as a separate Tauri window (not an in-page div) so it floats above other apps — true OS HUD, matching the brief ("its own window, not a wrapper").
- Reused `reqwest` (added `json` feature) for the Gemini REST call rather than a new SDK dep.
- Tray is always created (even though a `showTrayIcon` setting exists) because, with close→minimize, the tray is the only Quit; a real toggle can come once a fuller background-control UX exists.

## Verification
- `cargo build` exit 0 (tray/autostart/overlay/AI all compile; only the pre-existing `unload_idle` dead-code warning). `tsc` + `vite build` clean. App relaunched; transcription confirmed working live.

## Follow-ups
- AI **selection commands** ("make this professional / summarize / translate") — needs reading the current selection; deferred.
- `mute-while-recording`, `audio-feedback`, `showTrayIcon`, overlay-position setting: still stored-only/auto; wire behaviors next.
- Phase 5: packaging + signing, updater, optional Firebase sync; restyle web `LoginScreen`.
