# Dictando — Deferred Follow-ups

Optional items deferred after the Phase 0–5 build. **I'll keep reminding you of
the open items until each is Done or Cancelled** — tell me to cancel any you
don't want, or "do #N" to start one.

| # | Item | Status | Notes |
|---|------|--------|-------|
| 1 | **macOS first-run UX** | ✅ Done* | Mic usage string (`src-tauri/Info.plist`) + Accessibility onboarding (`MacOnboarding.tsx`, `accessibility_status`/`request_accessibility`/`open_accessibility_settings`). *Code-complete & compiles; needs verification on a real Mac (dev machine is Windows). |
| 2 | **Auto-updater** | ⬜ Planned | `tauri-plugin-updater` + signing key + an update host (e.g. GitHub Releases manifest). |
| 3 | **Native cross-device sync** | ⬜ Planned | Sync history/settings via Firebase when logged in. Native OAuth in WebView2 is the tricky part. |
| 4 | **Branded app icon** | ⬜ Planned | Replace the default generated Tauri icon set with a Dictando mark. |
| 5 | **AI selection commands** | ⬜ Planned | Highlight text + speak "make this professional / summarize / translate" → AI rewrites the selection (Wispr-style). |

Status key: ⬜ Planned · 🔨 In progress · ✅ Done · ❌ Cancelled

_Last updated: 2026-06-16._
