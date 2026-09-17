---
layout: home
title: Neloa Documentation
titleTemplate: false

hero:
  name: Neloa
  text: Documentation
  tagline: Transfer files and clipboard content between your own devices, without an account. Files and text are always end-to-end encrypted by the clients, and the relay only forwards data it cannot read.
  actions:
    - theme: brand
      text: First time here
      link: /en/guide/
    - theme: alt
      text: Download 0.1.12
      link: https://github.com/kure29/Neloa/releases/latest

features:
  - title: First time here
    details: Download, pair two devices with a six-digit code, and send your first file.
    link: /en/guide/install
    linkText: Start
  - title: Everyday use
    details: Multi-file queues, cancellation and retry, verification, save location, and the limits of clipboard sync.
    link: /en/guide/transfer
    linkText: Read more
  - title: Deploy and build
    details: Deploying a self-hosted relay, plus build steps for macOS, Windows, Android, and iOS.
    link: /en/build/
    linkText: Go to builds
---

## Current status

The current stable version is **0.1.12**, with a universal macOS DMG, Windows x64 installer, an Android ARM64 APK, and an unsigned iOS IPA. The desktop packages are not commercially code-signed, and the IPA must be re-signed with your own Apple certificate. [Download and install](/en/guide/install) covers each of these.

## What changed in 0.1.12

- A peer with a discovered LAN address stays on direct QUIC instead of silently switching to the relay after a local error.
- Initial pairing is LAN-only; the relay accepts previously paired devices only.
- The macOS package now declares Local Network and Bonjour usage and connection timeouts provide actionable guidance.

## How this documentation is organised

User, build, and architecture documentation lives in the `docs/` directory. [Relay service and wire protocol](/en/reference/relay) is pulled directly from `relay/README.md`, avoiding a second copy of the protocol reference. Each topic has one source of truth, so the published documentation cannot drift away from the repository.

## What it deliberately does not do

- There is no account system and no cloud storage: trust between devices is established entirely on the devices themselves.
- The relay takes no part in pairing. To reach each other through a relay, two devices must first be paired over a local network.
- Clipboard sync is plain text only and off by default; on mobile it only works while the app is in the foreground.
