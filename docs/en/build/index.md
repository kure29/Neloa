---
title: Building from source
---

# Building from source

## Requirements

You need Node.js 20+, npm, Rust stable, and the [Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/) for your platform.

```bash
npm install
npm run tauri dev
```

## Common commands

| Command | Purpose |
| --- | --- |
| `npm run dev` | Front-end preview only, without native networking or file access |
| `npm run tauri dev` | Run the desktop client |
| `npm run tauri -- build` | Produce an installer for the current desktop platform |
| `npm run android:build` | Build the Android client |
| `npm run ios:build` | Build the iOS client |

::: warning Do not re-initialise the native projects
The generated mobile projects contain custom native bridging code (the Android multicast lock, the iOS Bonjour adapter, and the Xcode build script). Unless you intend to re-merge changes in the generated directories, do not run `tauri android init` or `tauri ios init` again.
:::

## Continuous integration

`.github/workflows/ci.yml` runs on every push to `main` and on every pull request:

- `npm run build` (front-end type check and bundle)
- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test --lib` (the Rust client core)

Installers are produced by `.github/workflows/build-installers.yml`, triggered manually with a release tag.

## Per-platform pages

- [Windows installer](/en/build/windows) — build the NSIS installer with GitHub Actions or on Windows itself.
- [Android and iOS](/en/build/mobile) — signing setup, device builds, and device acceptance order.
