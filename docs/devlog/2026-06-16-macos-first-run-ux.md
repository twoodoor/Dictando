# macOS first-run UX: microphone + Accessibility permissions

- **Date:** 2026-06-16
- **Phase:** Follow-up #1
- **Status:** code-complete & compiles (Windows verified); needs a real-Mac test

## Context
On macOS, dictation can't work until the app has **Microphone** access (to record) and **Accessibility** access (so the global shortcut fires and `enigo` can type into other apps). Without guidance, a first-run Mac user would hit a silent dead end.

## What changed
- **`src-tauri/Info.plist`**: `NSMicrophoneUsageDescription` so macOS shows a proper mic prompt (Tauri merges this into the `.app`).
- **Backend commands** (`lib.rs`, macOS-gated via `cfg`, safe no-op fallbacks elsewhere): `get_platform`, `accessibility_status`, `request_accessibility` (system prompt), `open_accessibility_settings` (opens the Privacy → Accessibility pane). Uses the macOS-only `macos-accessibility-client` crate (target-gated dep, so the Windows build is untouched).
- **`MacOnboarding.tsx`**: a first-run modal with two steps — *Enable microphone* (triggers the OS prompt via `getUserMedia`) and *Accessibility* (opens settings; live status via polling + window-focus re-check, auto-checks off when granted). "Skip for now" available; the primary button unlocks once Accessibility is granted.
- **`App.tsx`**: shows the onboarding only on `macos` when Accessibility isn't yet granted.
- **`bridge.ts`**: exposes the new commands.

## Incidental fix
- `tsconfig.json` had no `include`/`exclude` with `allowJs: true`, so `tsc`/`npm run lint` started type-checking JS build artifacts under `src-tauri/target/` after a release build. Scoped to `"include": ["src"]`, `"exclude": [..., "src-tauri"]`.

## Verification
- `npm run lint` clean; `cargo check` (Windows) exit 0 — macOS dep is target-gated and commands compile cross-platform.
- **Pending:** run on a Mac (or via the CI build) to confirm the prompts/flow; will iterate after that.
