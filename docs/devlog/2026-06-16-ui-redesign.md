# UI redesign: full-window app shell, light/dark, Wispr-inspired

- **Date:** 2026-06-16
- **Phase:** UI overhaul (post Phase 3a)
- **Status:** done & build-verified (live in `tauri dev`)

## Context
End-to-end dictation was confirmed working (Parakeet v3 installed; ~150–250 ms transcriptions). The UI, however, still rendered as a tiny 384 px floating "phone card" with fake macOS dots — making the native app feel like a wrapper. User asked for a polished, Wispr-Flow-*inspired* (not copied) UI that fills its own window on Windows + Mac, with light default + dark mode, and a fix for the in-app hotkey setter.

## Decisions (with user)
- Layout: **left sidebar + slim custom titlebar**, full-window content.
- Theme: **light default + dark + system**, warm-neutral surfaces, **violet accent**, serif display headings.
- Nav: **Dictate · Models · History · Dictionary · Settings**.

## What changed
- **Window chrome:** `tauri.conf.json` → `decorations: false`, 1040×720 (min 880×600), centered. Added window-control + drag permissions to `capabilities/default.json`.
- **Theme system:** `index.css` rewritten with class-based dark variant (`@custom-variant dark`), semantic CSS-variable tokens (`--bg/--surface/--fg/--accent/…`) for light + `.dark`, helper utilities, serif `.font-display`. `lib/theme.ts` (`useTheme`, persists to native settings + localStorage, follows OS in "system"); applied pre-paint in `main.tsx`. New `theme` field on `AppSettings` (default `light`).
- **App shell:** `Titlebar.tsx` (drag region + min/max/close via `@tauri-apps/api/window`) and `Sidebar.tsx` (nav + theme switcher). `App.tsx` rewritten to a full-window flex shell; dropped the floating card + fake titlebar; login is optional (native) as before.
- **Restyle:** `RecordView`, `ModelsView`, `HistoryView`, `SettingsView` reworked onto the token system with spacious layouts and serif headings.
- **Dictionary:** new `DictionaryView.tsx` (manage `customWords` via settings). *(Model biasing with these words is a later enhancement.)*
- **Hotkey recorder fixed:** old recorder committed on the first key-up with a partial combo (mangled Ctrl+Space). New capture finalizes when a non-modifier key is pressed (modifiers + main key), Esc cancels — reliably records combos like Ctrl+Space.

## Verification
- `tsc` clean; `npm run build` clean (all views resolve); `tauri dev` rebuilt backend (decorations/theme/capabilities) and hot-reloaded the new UI; app process running.

## Follow-ups
- Phase 3b: recording overlay as a real always-on-top window; tray icon, launch-on-startup, mute-while-recording, audio feedback, auto-delete recordings.
- Apply `customWords` as transcription biasing / post-correction.
- LoginScreen still uses the old dark palette (web-only path) — restyle when sync (Phase 5) is wired.
