# Rename Application to Mumblr

- **Date:** 2026-07-27
- **Phase:** Rebranding / Packaging
- **Status:** done

## What changed
- `package.json`, `index.html`, `metadata.json`, `vite.config.ts`, `firebase.json`: Updated package name, title, and manifest metadata to **Mumblr**.
- `src-tauri/tauri.conf.json`: Updated product name to `Mumblr`, bundle identifier to `com.mumblr.app`, window title, publisher, copyright, and product description.
- `src-tauri/Cargo.toml`: Updated package name to `mumblr`, description, and author.
- `src-tauri/src/lib.rs`: Updated system tray menu labels ("Show Mumblr", "Quit Mumblr") and tray icon tooltip.
- `src/components/Titlebar.tsx`, `LoginScreen.tsx`, `MacOnboarding.tsx`, `DictionaryView.tsx`, `SettingsView.tsx`: Updated UI brand strings and descriptions to **Mumblr**.
- `src/lib/theme.ts`: Updated localStorage key namespace to `mumblr_theme` with fallback support.
- `CLAUDE.md`, `GEMINI.md`, `AGENTS.md`: Updated agent documentation to reference Mumblr.

## Why
Rebranded the application from Dictando to Mumblr per user request and generated updated native Windows installers.
