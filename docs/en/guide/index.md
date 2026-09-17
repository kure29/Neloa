---
title: What Neloa is
---

# What Neloa is

Neloa transfers files and plain-text clipboard content between your own devices. It prefers a direct local-network connection and, when your devices are not on the same network, can connect them through a relay server you host yourself.

## Three deliberate decisions

**No account.** A device identity is a long-lived key pair kept in the system credential store: Keychain on macOS and iOS, Credential Manager on Windows, Keystore on Android. No server holds your identity, so no server can use it for anything else.

**Local network first.** On the same network, devices connect directly over QUIC and find each other over mDNS/Bonjour (service `_neloa._udp.local.`, transport port UDP 48631). A discovered LAN route never switches silently to the relay; relay transport is used only when a paired device has no LAN route.

**End-to-end encryption.** Every piece of application data above the transport is protected by Noise XX, and the long-term device key takes part in authentication. The relay can see online device metadata and the timing and size of traffic, but never plaintext.

## Platform support

| Platform | Build | Notes |
| --- | --- | --- |
| macOS | Universal DMG | Apple Silicon and Intel |
| Windows | x64 NSIS | Windows 10 1803 and later usually ship WebView2 |
| Android | ARM64 APK | For mainstream physical Android devices |
| iOS | Unsigned IPA | Must be re-signed with your own certificate and profile |

## A first transfer in 30 seconds

1. Open Neloa on both devices; keep them on the same network if you are using a local connection.
2. On the devices screen, pick the device you want to connect to, compare the six-digit code shown on both devices, and confirm on each.
3. Select one or more files; on macOS and Windows you can also drag files into the window.
4. Check the pending list and press send. The receiving side has to explicitly accept each file.
5. Received files are saved to the `Neloa` folder inside your system Downloads folder.

Before using a relay across networks, pair once over the local network and then configure the same relay on both devices. See [Self-hosted relay](/en/guide/relay).

## Where to go next

- [Download and install](/en/guide/install) — per-platform builds, signing status, and the prompts you will see on first launch.
- [Pairing and device trust](/en/guide/pairing) — what the six-digit code is actually verifying.
- [Sending and receiving files](/en/guide/transfer) — queues, cancellation, retry, verification, and save location.
- [Clipboard sync](/en/guide/clipboard) — off by default, plain text only, what gets blocked, and mobile limits.
