# Dictando — Build & Deployment

How to produce installable Dictando executables for Windows and macOS, and how
to sign them for distribution.

## Artifacts

| OS | Targets | Output (under `src-tauri/target/release/bundle/`) |
|---|---|---|
| Windows | NSIS `.exe`, WiX `.msi` | `nsis/Dictando_<ver>_x64-setup.exe`, `msi/Dictando_<ver>_x64_en-US.msi` |
| macOS | `.app`, `.dmg` | `macos/Dictando.app`, `dmg/Dictando_<ver>_<arch>.dmg` |

`bundle.targets` is `"all"`, so each OS produces its applicable installers.

## Local builds

```bash
npm install
npm run tauri build        # builds the OS you're currently on
```

- **Windows:** run on Windows. First build auto-downloads WiX + NSIS. Produces `.msi` and `.exe`.
- **macOS:** run on a Mac. For a universal (Apple Silicon + Intel) build:
  ```bash
  rustup target add aarch64-apple-darwin x86_64-apple-darwin
  npm run tauri build -- --target universal-apple-darwin
  ```

> Tauri **cannot cross-compile** a macOS app from Windows/Linux. To produce Mac
> artifacts from a Windows dev machine, use the CI workflow below (macOS runner).

## CI (both platforms)

`.github/workflows/release.yml` builds Windows + macOS via `tauri-apps/tauri-action`
on a tag push and attaches the installers to a **draft** GitHub Release.

```bash
git tag v0.1.0
git push origin v0.1.0      # → builds, creates draft release with installers
```

Or run it manually from the Actions tab (`workflow_dispatch`).

## Code signing

Unsigned apps still install but show warnings (Windows SmartScreen / macOS
Gatekeeper "unidentified developer"). For public distribution:

### Windows (Authenticode)
- Option A — **Azure Trusted Signing** (recommended, no physical token): set
  `bundle.windows.signCommand` or use the `azure/trusted-signing-action` step,
  then build. 
- Option B — an OV/EV code-signing certificate: set
  `bundle.windows.certificateThumbprint` + `timestampUrl` in `tauri.conf.json`,
  with the cert installed on the signing machine/runner.

### macOS (codesign + notarization)
Set these repo secrets (consumed by `tauri-action`, already wired in the
workflow — just uncomment): `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`,
`APPLE_SIGNING_IDENTITY` (a "Developer ID Application" cert), and for
notarization `APPLE_ID`, `APPLE_PASSWORD` (app-specific password), `APPLE_TEAM_ID`.

macOS also needs, before first dictation works:
- **Microphone** permission — add a usage description (see `Info.plist` /
  `bundle.macOS`), and
- **Accessibility** permission (System Settings → Privacy & Security →
  Accessibility) so global shortcuts and keystroke injection (`enigo`) work.

## Auto-update (future)

Not yet enabled. To add later:
1. `npm run tauri signer generate -- -w ~/.tauri/dictando.key` → keypair.
2. Add `tauri-plugin-updater`, set `plugins.updater.pubkey` + `endpoints` and
   `bundle.createUpdaterArtifacts: true` in `tauri.conf.json`.
3. Provide `TAURI_SIGNING_PRIVATE_KEY` (+ password) at build time (CI secrets,
   already referenced in the workflow) and host the update manifest at the
   endpoint.

## Versioning

Bump `version` in `package.json` **and** `src-tauri/tauri.conf.json` (or set
`bundle.version` to read from `package.json`) before tagging a release.
