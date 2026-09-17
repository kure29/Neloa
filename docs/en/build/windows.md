---
title: Windows installer
---

# Building the Neloa Windows installer

Translated from the [Chinese source document](/build/windows). If the two disagree, the Chinese source wins.

Official Windows x64 NSIS installers are built by the **Build Installers** GitHub Actions workflow on `windows-latest` and uploaded to the matching draft Release. No separate source archive or batch wrapper is required.

## GitHub Actions

1. Open **Actions → Build Installers** in the repository.
2. Choose **Run workflow** and enter the release tag to build.
3. Wait for the `Windows x64` job to finish.
4. Download the `.exe` from the matching draft Release.

Pushing a `v*` tag starts the same workflow automatically.

## Build locally on Windows

### Install these before the first build

1. Node.js 20 or later.
2. Rust stable, with the default `x86_64-pc-windows-msvc` toolchain.
3. Visual Studio 2022 Build Tools with **Desktop development with C++** and the Windows 10/11 SDK selected.
4. Network access to download npm, Cargo, and NSIS build dependencies.

Run from the repository root:

```powershell
npm ci
npm run tauri -- build --bundles nsis
```

Windows 10 1803 and later usually already include WebView2; when it is missing, the installer follows Tauri's default strategy.

## Output location

A successful local build leaves the installer at:

```text
src-tauri\target\release\bundle\nsis\Neloa_0.1.12_x64-setup.exe
```

The final file name can vary slightly between Tauri versions; trust whichever `.exe` is in that directory. Build output is not committed to Git; the files in GitHub Releases are the delivery artifacts.

## Current signing status

This is a development build and has no commercial code-signing certificate configured. Windows SmartScreen may report an unknown publisher; configure Windows code signing before distributing it publicly.
