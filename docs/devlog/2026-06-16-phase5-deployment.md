# Phase 5: deployment — installers for Windows & macOS

- **Date:** 2026-06-16
- **Phase:** 5 — Packaging & deployment
- **Status:** Windows installers built & verified; macOS via CI; signing documented

## What changed
- **Bundle config** (`tauri.conf.json`): added `publisher`, `copyright`, `category`, short/long descriptions, `windows.nsis.installMode: currentUser`, `macOS.minimumSystemVersion`. Targets remain `"all"` (each OS emits its applicable installers).
- **CI** (`.github/workflows/release.yml`): `tauri-apps/tauri-action` matrix over `windows-latest` + `macos-latest` (universal `aarch64,x86_64`), triggered on `v*` tags or manual dispatch; uploads to a draft GitHub Release. Signing env vars are wired (commented) for both platforms.
- **Docs** (`docs/deployment.md`): artifact locations, local build commands, the CI flow, Windows Authenticode / Azure Trusted Signing, macOS codesign + notarization (+ required mic & Accessibility permissions), and updater setup (future).

## Built artifacts (local, Windows, release)
`src-tauri/target/release/bundle/`
- `nsis/Dictando_0.1.0_x64-setup.exe` — 8.1 MB
- `msi/Dictando_0.1.0_x64_en-US.msi` — 20.5 MB

Built in ~2m06s (`npm run tauri build`, exit 0; only the benign `unload_idle` warning).

## Decisions & rationale
- **macOS builds via CI**, not locally: Tauri cannot cross-compile a `.app`/`.dmg` from Windows, so a macOS runner is the supported path. The workflow produces a universal binary.
- **Installers ship unsigned for now** — fully functional but trigger SmartScreen/Gatekeeper warnings. Signing is config-ready (just add secrets/certs); documented rather than blocking, since it needs the user's certificates.

## Deferred (follow-ups)
- **Auto-updater**: plugin + signing key + endpoint (documented; needs an update host).
- **Optional Firebase sync** for native (cross-device history/settings): native OAuth in WebView2 is non-trivial; deferred. Web path still uses Firebase.
- A **Dictando-branded icon** (currently the default generated icon set).
- macOS `Info.plist` mic usage string + first-run Accessibility prompt UX.
