<div align="center">
  <img src="assets/neloa-icon.svg" alt="Neloa logo" width="112" />
  <h1>Neloa</h1>
  <p><strong>Private file and clipboard transfer for nearby devices.</strong></p>
  <p>
    Local-first · End-to-end encrypted · No account required
  </p>
  <p>
    <a href="https://github.com/kure29/Neloa/actions/workflows/ci.yml"><img src="https://github.com/kure29/Neloa/actions/workflows/ci.yml/badge.svg" alt="CI status" /></a>
    <a href="https://github.com/kure29/Neloa/releases"><img src="https://img.shields.io/github/v/release/kure29/Neloa?include_prereleases&label=release" alt="Latest release" /></a>
    <img src="https://img.shields.io/badge/Tauri-2-24C8DB?logo=tauri&logoColor=white" alt="Tauri 2" />
    <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Android%20%7C%20iOS-6B7280" alt="Supported platforms" />
  </p>
  <p>
    <a href="#quick-start">Quick start</a> ·
    <a href="ARCHITECTURE.md">Architecture</a> ·
    <a href="MOBILE_BUILD.md">Mobile builds</a> ·
    <a href="WINDOWS_BUILD.md">Windows build</a>
  </p>
</div>

> [!IMPORTANT]
> Neloa is under active development. Current packages are intended for testing and are not yet production-signed on macOS or Windows.

Neloa finds devices on the same local network and transfers files or clipboard text directly between them. It does not require an account, upload your content to a cloud service, or persist clipboard contents.

## Why Neloa

- **Local by default.** Discovery and transfer happen over your LAN, without an internet connection.
- **Authenticated encryption.** QUIC carries Noise XX sessions tied to a persistent X25519 device identity.
- **Human-verifiable pairing.** Both devices compare the same six-digit code before trust is saved.
- **Reliable file delivery.** Transfers are cancellable, verified with SHA-256, and published atomically without overwriting existing files.
- **Safer clipboard sync.** Synchronization is opt-in, text-only, foreground-only on mobile, and blocks common private-key markers.
- **One cross-platform experience.** A shared React interface runs inside native Tauri 2 shells on desktop and mobile.

## Platform status

| Platform | Current state | Distribution |
| --- | --- | --- |
| macOS | App, discovery, pairing, file transfer, and clipboard sync implemented | Universal DMG; ad-hoc signed, not notarized |
| Windows | App, discovery, pairing, file transfer, and clipboard sync implemented | x64 NSIS installer; unsigned |
| Android | Native Keystore identity, mDNS multicast support, file picker, and foreground clipboard sync implemented | Signed ARM64 and x86_64 APKs |
| iOS | Native Keychain identity and lifecycle-aware Bonjour adapter implemented; cross-device discovery needs a final real-device retest | Local IPA; Apple signing required |

The generated mobile projects live in the repository so platform-specific networking, permissions, signing, and lifecycle behavior can be maintained alongside the shared application.

## Quick start

### Test builds

Test packages are produced by the `Build Installers` workflow and attached to draft prereleases. Published builds appear on the [GitHub Releases](https://github.com/kure29/Neloa/releases) page.

Because desktop test builds are not production-signed, macOS Gatekeeper or Windows SmartScreen may show an unknown-publisher warning. Android APKs use the project's persistent release identity. iOS builds must be signed with an Apple development team before installation.

### Run from source

Requirements:

- Node.js 20 or newer and npm;
- the Rust stable toolchain;
- the [platform prerequisites required by Tauri 2](https://v2.tauri.app/start/prerequisites/).

```bash
npm install
npm run tauri dev
```

To preview the interface without native device access:

```bash
npm run dev
```

Then open one of these URLs:

- macOS: `http://127.0.0.1:1420/?platform=macos`
- Windows: `http://127.0.0.1:1420/?platform=windows`
- Mobile: `http://127.0.0.1:1420/?platform=mobile`

Browser preview mode uses simulated peers, transfers, and clipboard events. Real discovery, pairing, file access, and clipboard integration require a native Tauri build.

## How it works

1. Neloa advertises and discovers nearby peers using mDNS on desktop/Android or Bonjour on iOS.
2. A simultaneous QUIC endpoint connects the devices over UDP port `48631`.
3. A Noise XX handshake authenticates both device identities and negotiates protocol capabilities.
4. On first connection, both users confirm a six-digit short authentication string.
5. Files and clipboard events travel only inside the authenticated session.

Trust is bound to both the discovered device ID and the stored Noise public key. Legacy, malformed, or capability-incompatible peers fail closed with an upgrade message.

## Security and privacy

- Device private keys are kept in macOS Keychain, Windows Credential Manager, Android Keystore, or iOS Keychain.
- Application data is protected by Noise XX; QUIC's per-launch certificate is transport-only and is not treated as device identity.
- Incoming files are written to transfer-specific `.part` files, checked for valid offsets and size, verified by SHA-256, then atomically published with a collision-safe name.
- Clipboard sync is disabled by default. Enabling it establishes a local baseline, so content copied before opt-in is never sent.
- Clipboard text is limited to 8 KB, normalized across platform line endings, deduplicated to prevent loops, and never stored in settings or transfer history.
- Diagnostic reports omit IP addresses, full device IDs, public keys, file paths, and clipboard contents.

For the complete trust model and protocol boundaries, see [ARCHITECTURE.md](ARCHITECTURE.md#security-boundary).

## Build and verify

Build the shared interface and run the Rust checks used in CI:

```bash
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --lib
```

Platform packaging instructions:

- macOS: `npm run tauri build` creates the `.app` and `.dmg` under `src-tauri/target/release/bundle/`.
- Windows: follow [WINDOWS_BUILD.md](WINDOWS_BUILD.md) to create an x64 NSIS installer on Windows.
- Android and iOS: follow [MOBILE_BUILD.md](MOBILE_BUILD.md) for signing, device installation, rebuild commands, limitations, and acceptance testing.

The `CI` workflow runs the frontend build, Rust formatting check, Clippy with warnings denied, and Rust library tests on every push to `main` and every pull request. The `Build Installers` workflow creates macOS, Windows, and Android test packages on demand or for a `v*` tag.

## Roadmap

- Complete Android/iOS real-device discovery, pairing, transfer, lifecycle, and clipboard acceptance testing.
- Add pairing throttling, settings migrations, and more actionable diagnostic categories.
- Configure production signing, notarization, and mobile distribution.
- Add an optional relay transport while keeping identity, encryption, and transfer envelopes independent of the relay.

## Repository layout

```text
src/          Shared React interface and typed Tauri bridge
src-tauri/    Native shells, identity, trust, discovery, and QUIC/Noise services
assets/       Editable source artwork for generated app icons
```

Additional project documentation:

- [ARCHITECTURE.md](ARCHITECTURE.md) — implementation boundaries, protocol, and security model
- [MOBILE_BUILD.md](MOBILE_BUILD.md) — Android/iOS builds and real-device checklist
- [WINDOWS_BUILD.md](WINDOWS_BUILD.md) — Windows prerequisites and NSIS packaging
