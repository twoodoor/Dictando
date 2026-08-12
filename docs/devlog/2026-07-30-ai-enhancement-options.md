# Configurable AI Cleanup Options & Native In-App Auto-Updater

- **Date:** 2026-07-30
- **Phase:** Phase 4 & Phase 5 (AI Enhancement & Auto-Updater)
- **Status:** done

## What changed
- Extended `AppSettings` struct in `src-tauri/src/settings.rs` and `src/lib/bridge.ts` with 5 new AI cleanup settings: `aiFixPunctuation`, `aiRemoveFillers`, `aiRemoveRepetitions`, `aiStylePreset`, and `aiCustomInstructions`.
- Refactored `src-tauri/src/ai.rs` to construct dynamic prompts based on enabled cleanup toggles, selected style preset (`clean`, `polished`, `concise`, `casual`), user custom rules, and dictionary terms.
- Integrated **Tauri Auto-Updater** (`tauri-plugin-updater` & `@tauri-apps/plugin-updater`) with public key verification (`dW50cnVzdGVk...`) pointing to GitHub Releases.
- Added **Check for Updates** interactive button & auto-relaunch support in `src/components/SettingsView.tsx`.

## Why
Users need fine-grained control over AI cleanup and an automated, in-app update mechanism so they don't have to manually download and reinstall installer files for every release.

## Decisions & rationale
- **Backwards Compatibility:** Added `#[serde(default)]` helper macros to `AppSettings` to prevent settings reset when loading existing configuration files.
- **Style Presets:** Created 4 core presets (*Clean & Natural*, *Polished & Professional*, *Concise & Direct*, *Casual & Relaxed*) to serve common dictation use cases.
- **Dictionary Integration:** Vocabulary terms added in the Dictionary tab are automatically injected into the AI prompt to enforce exact brand and jargon spelling.
- **Auto-Updater:** Uses Tauri v2 minisiq public key verification pointing to GitHub Releases endpoint `https://github.com/twoodoor/Dictando/releases/latest/download/latest.json`.
