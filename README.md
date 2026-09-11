# Neloa

[![CI](https://github.com/kure29/Neloa/actions/workflows/ci.yml/badge.svg)](https://github.com/kure29/Neloa/actions/workflows/ci.yml)

Neloa is a local-first application for transferring files and clipboard text between nearby devices. Native clients are being developed for macOS, Windows, Android, and iOS.

## Status

The repository currently contains an executable LAN transfer milestone:

- one shared React interface with a single design system across macOS, Windows, and mobile, following the system light/dark preference;
- Tauri 2 desktop and mobile shells, with generated Android and iOS native projects;
- a persistent X25519 device identity stored in macOS Keychain, Windows Credential Manager, Android Keystore, or iOS Keychain;
- live LAN advertising and discovery through mDNS;
- QUIC connections carrying Noise XX authenticated sessions;
- six-digit short authentication string comparison on both devices;
- a persistent trust store with explicit trust removal;
- an authenticated internal test-message channel for protocol diagnostics;
- native file selection and metadata inspection, including Android `content://` sources and iOS sandbox copies;
- explicit incoming file offers with accept or reject controls;
- chunked, cancellable file transfer inside the authenticated Noise session;
- SHA-256 verification, atomic publication, collision-safe filenames, progress, history, and retry;
- opt-in, persistent plain-text clipboard synchronization for online trusted devices while the mobile app is in the foreground;
- clipboard event deduplication, cross-platform line-ending normalization, an 8 KB limit, and local blocking for strong credential markers;
- no clipboard body persistence and no transmission of content that existed before synchronization was enabled.
- protocol range and capability negotiation in both mDNS discovery and every Noise handshake;
- fail-closed blocking for legacy or incompatible peers with a clear upgrade message;
- an in-app LAN diagnostics center covering QUIC, mDNS, device identity, peer capabilities, clipboard readiness, and platform-specific firewall guidance;
- one-click copying of a sanitized diagnostic report without IP addresses, full device IDs, public keys, file paths, or clipboard content.

The Android ARM64 debug APK has been built locally and structurally verified. The iOS Xcode project, local-network declarations, and signing entitlement are generated, but an iOS binary still requires full Xcode, an Apple development team, and real-device validation. Pairing throttling, settings migration, signed release packaging, and cross-device acceptance testing remain product milestones.

## Run locally

Requirements: Node.js 20+, npm, Rust, and the platform prerequisites required by Tauri.

```bash
npm install
npm run tauri dev
```

Build the web interface and check the Rust backend:

```bash
npm run build
cd src-tauri
cargo check
cargo clippy --all-targets -- -D warnings
cargo test --lib
```

To preview the interface in a regular browser, run `npm run dev` and open:

- `http://127.0.0.1:1420/?platform=macos`
- `http://127.0.0.1:1420/?platform=windows`
- `http://127.0.0.1:1420/?platform=mobile`

The macOS and Windows previews are pixel-identical inside the window; only the window controls, the system font stack, and the frame radius differ. `?platform=mobile` forces the mobile shell (safe-area header and bottom tab bar). Without the query parameter, the mobile shell is used only for a coarse pointer on a narrow viewport — a real phone, or browser device emulation — so resizing a desktop window never removes its window controls.

The browser preview uses simulated devices, transfers, and clipboard events. Real discovery, pairing, file I/O, and system clipboard access are available in native Tauri builds, including the generated mobile clients.

## Installation packages

Run `npm run tauri build` on macOS to generate the `.app` and `.dmg` under `src-tauri/target/release/bundle/`.

A Windows NSIS installer must be compiled on Windows. Run `./packaging/create-windows-source-package.sh` on macOS to produce a compact source archive under `releases/`; after transferring and extracting that archive on Windows, double-click `build-windows.bat`. See `WINDOWS_BUILD.md` for the one-time Windows prerequisites and output path.

Development packages are currently unsigned. macOS Gatekeeper and Windows SmartScreen may show an unknown-publisher warning until release signing and notarization are configured.

Local build artifacts are written to `releases/`, which is intentionally excluded from Git history; distributable binaries should be attached to GitHub Releases. The current workspace contains an ARM64 Android debug APK. The iOS project is at `src-tauri/gen/apple/neloa.xcodeproj`; an IPA cannot be produced without full Xcode and Apple signing. See `MOBILE_BUILD.md` for installation, rebuild commands, platform limitations, and the real-device acceptance checklist.

## GitHub Actions

The `CI` workflow runs on every push to `main`, every pull request, and on demand. It installs dependencies from the lockfiles, builds the React frontend, checks Rust formatting, runs Clippy with warnings denied, and executes the Rust library tests.

The `Build Installers` workflow runs on demand from the repository's **Actions** tab and whenever a `v*` tag is pushed. Manual runs ask for a test release tag, defaulting to `v0.1.0-test`. A successful run creates or updates a draft prerelease with downloadable assets:

- an unsigned Windows x64 NSIS installer;
- a universal Intel/Apple Silicon macOS DMG with an ad-hoc signature, but without notarization;
- an ARM64 Android debug APK signed with the temporary debug identity for real-device testing.

These test builds do not require repository secrets. Draft releases are used instead of Actions artifacts so the large Android package does not consume the account's limited Actions/Packages artifact allowance. iOS and production signing are intentionally excluded until the Apple, Android, and Windows release credentials are configured as GitHub Actions secrets. Build artifacts belong in GitHub Releases and must not be committed to the repository.

## Clipboard behavior

Clipboard synchronization is disabled by default. Enable it in **Settings → Clipboard Sync** on both paired devices. Enabling establishes a local baseline and does not send the clipboard content that was already present. Only later plain-text changes are sent, and the clipboard body is never written to Neloa's settings or history files.

For manual acceptance, copy an ordinary short text on each device and confirm it can be pasted on the other. Then verify pause/resume, Windows/macOS line endings, oversized text, and a sample containing `-----BEGIN PRIVATE KEY-----`; protected content must stay local.

Before a two-machine test, open **Settings → LAN Diagnostics** on each device. All critical checks should be green, and the other computer should show protocol `v1–v1` with the expected capabilities. If a problem remains, use **Copy Report**; the generated text is intentionally sanitized and is also written as a local clipboard baseline so Neloa does not synchronize the report to peers.

## Repository layout

- `src/`: shared desktop/mobile interface and typed Tauri bridge.
- `src-tauri/`: native shells, secure identity, trust store, mDNS discovery, and QUIC/Noise network service.
- `assets/`: editable source artwork for generated app icons.
- `ARCHITECTURE.md`: boundaries and implementation milestones.
- `MOBILE_BUILD.md`: Android/iOS build status and real-device test checklist.
- `neloa_*.html`: unchanged original design demonstrations.
