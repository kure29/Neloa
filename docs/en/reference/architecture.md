---
title: Architecture and security
---

# Neloa architecture

Neloa is local-first. Every client should remain useful without an account or an internet connection.

## Current milestone

- One React interface and one design system for every platform. `DesktopShell` and `MobileShell` share `views/` and `ui/`; macOS and Windows differ only in window controls, system font stack, and frame radius. Colour comes from a single set of CSS custom properties with a light and a dark value, so component rules never branch on the colour scheme.
- Tauri 2 desktop and mobile shells, with generated Android and iOS native projects.
- Stable X25519 device identity stored in the native OS credential store: Keychain, Credential Manager, Android Keystore, or iOS Keychain.
- `_neloa._udp.local.` service advertising and discovery: `mdns-sd` on desktop/Android and an Apple Bonjour adapter on iOS.
- Live peer updates from Rust to the interface.
- Simultaneous QUIC client/server endpoint on UDP port `48631`.
- Noise XX handshake over every QUIC stream.
- Six-digit short authentication string derived from the Noise handshake hash.
- Explicit confirmation on both devices before persisting the peer public key.
- Trusted-device verification and revocation.
- End-to-end encrypted test text with application-level acknowledgement.
- Native multi-file selection, desktop drag and drop, bounded send queues, and file metadata inspection. Android content URIs are opened through the platform file-descriptor bridge, staged only when sending, and deleted after the transfer; iOS uses sandbox copies.
- File offer/accept flow tied to a trusted device identity.
- Encrypted binary chunks with strict offsets, size limits, progress, and cancellation.
- SHA-256 verification before publication and an authenticated completion receipt.
- Temporary `.part` writes followed by atomic, no-overwrite publication in `Downloads/Neloa`.
- Transfer result history and retry for interrupted outgoing files.
- Opt-in clipboard monitoring on a dedicated thread using the native system pasteboard. Mobile synchronization is foreground-only because the operating systems restrict background clipboard reads.
- Plain-text clipboard events sent to every online trusted device through authenticated Noise sessions.
- Event UUID deduplication and normalized line-ending comparison to prevent rebroadcast loops.
- An 8 KB payload limit, strong credential-marker blocking, and no clipboard-body persistence.
- Protocol range and capability negotiation in discovery and authenticated handshakes.
- Explicit rejection of legacy, malformed, or feature-incompatible peers before application data is accepted.
- A user-readable local connection status for QUIC, mDNS, identity, clipboard, peer capability, and firewall checks.
- An optional self-hosted relay client with persistent authenticated WebSocket connections, trusted-device presence, and bounded virtual streams.
- LAN-first transport selection: QUIC is attempted whenever a local address exists, then the relay is used only as a fallback for an already trusted online device.

Advertising declares `protocolVersion=1`, `minProtocolVersion=1`, and `capabilities=discovery,pairing,noise-xx,test-message,file-transfer,clipboard-text`. The same protocol range and capability list is authenticated inside the Noise XX handshake; mDNS values are presentation and early-filtering hints only.

## Interface layout

```text
src/
  bridge.ts        typed Tauri commands/events, plus the browser preview simulation
  lib/useNeloa.ts  all application state and actions; both shells consume it
  ui/              icons, kit primitives, overlays
  views/           RadarView | HistoryView | SettingsView — shell-agnostic
  shell/           DesktopShell (title bar) | MobileShell (tab bar, safe areas)
  styles/          tokens | base | components | desktop | mobile
```

`views/` and `ui/` contain no platform or shell branches except through `useShell()`, which only switches a modal between a centred dialog and a bottom sheet. The shell is chosen in the frontend from `?platform=`, pointer type, viewport width, and user agent. The Rust backend reports `macos`, `windows`, `linux`, `ios`, or `android`; protocol behavior remains shell-independent.

## Security boundary

QUIC uses a per-launch self-signed certificate only as a reliable encrypted datagram transport. It is not treated as the long-term device identity. The optional relay exposes each tunnel as the same bidirectional byte-stream interface. All application payloads are wrapped by Noise XX, and no plaintext application message may be written directly to either transport.

Remote relay URLs must use `wss://`; plaintext `ws://` is accepted only for a loopback development server. The shared access token is stored in the native OS credential store, while `relay-settings.json` contains only the enable flag and public URL. Relay presence is filtered to devices already present in the local trust store. Even if the server sends forged device metadata, the subsequent Noise handshake must still match the pinned peer public key before application data is accepted.

The Noise static private key is stored under the application service name in macOS Keychain, Windows Credential Manager, Android Keystore-backed storage, or iOS Keychain. `trusted-devices.json` contains only peer public keys, fingerprints, device metadata, and timestamps.

Desktop builds retain the original credential-store service name when the public application identifier changes. On first launch under `com.kure29.neloa`, Neloa copies a missing `device-id`, trust store, and clipboard settings from the legacy `app.neloa.desktop` data directory without overwriting any data already created by the new version. Mobile operating systems treat the new bundle/application ID as a separate sandbox, so mobile upgrades still require a fresh install and pairing.

During first pairing, both devices compare a six-digit value derived from the same Noise handshake transcript. Trust is committed only after both sides explicitly accept. Later sessions must match both the discovered device ID and the stored Noise public key.

Every Noise handshake carries the sender's current and minimum supported protocol versions plus its feature capabilities. Version ranges must overlap, malformed ranges fail closed, and the capability required by the session purpose must be present. A peer that omits these fields is treated as legacy protocol `0` and receives an actionable incompatibility error instead of entering pairing or transfer flows.

Before sending file data, the sender hashes the source and offers its sanitized base name, byte size, and SHA-256 digest. The receiver writes only to a transfer-specific temporary file, rejects invalid offsets or excess bytes, verifies the final digest, flushes it to disk, and publishes it without replacing an existing file. Cancellation and failure remove only that transfer's temporary file.

Clipboard synchronization is fail-closed and disabled by default. Enabling it records the current clipboard as a local baseline without transmitting it. Subsequent text updates receive UUIDs, are checked for size and strong credential markers, and are sent only to currently discovered trusted peers. Remote updates are acknowledged only after the OS clipboard write succeeds. Received content becomes the new local baseline, preventing it from being sent back; CRLF, CR, and LF are canonicalized for comparison across Windows and macOS. Events retain only peer, direction, byte count, status, and time—not clipboard text.

Android requires `CHANGE_WIFI_MULTICAST_STATE` plus a held `WifiManager.MulticastLock` while the Activity is alive so mDNS packets are delivered reliably. iOS does not open a raw multicast socket: `NWBrowser` browses `_neloa._udp`, while `NetService` publishes and resolves the Bonjour service that points at the existing Rust QUIC listener on UDP 48631. Swift forwards resolved IPv4 addresses and TXT metadata to the shared Rust peer store. This path uses the declared Bonjour service and local-network privacy prompt without the restricted multicast entitlement.

## Planned boundaries

```text
React UI
  -> typed Tauri commands/events
Application services
  -> discovery | pairing | transfer | clipboard | history
Platform adapters
  -> mDNS/Bonjour | QUIC | filesystem | system clipboard | secure key store
```

The relay server lives in `relay/`, with shared wire definitions in
`crates/neloa-relay-protocol`. It authenticates a small single-user deployment
with a server token, keeps presence and tunnel state in memory, and forwards
bounded binary frames without inspecting their Noise-encrypted contents. The
client maintains one WebSocket connection, maps relay tunnels to bounded local
byte streams, and reuses the existing pairing, transfer, test-message, and
clipboard Noise protocol without transport-specific envelopes. Relay pairing is
intentionally disabled: devices must establish trust locally before they are
eligible for relay presence or tunnels.

## Next milestones

1. Relay acceptance: deploy behind a real TLS reverse proxy, validate reconnect and cross-network file/clipboard transfer on two physical devices, and add operational metrics without payload logging.
2. Mobile acceptance: complete Android real-device validation and expand iOS coverage across network changes, long transfers, lifecycle, and foreground clipboard behavior.
3. Hardening: pairing throttling, diagnostic error categorization, migration regression coverage, and platform firewall/lifecycle handling.
4. Packaging: signed `.dmg`/`.app`, Windows MSIX or NSIS, Android release signing, and iOS/TestFlight distribution.
