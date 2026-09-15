---
title: Windows installer
---

# Building the Neloa installer on Windows

Translated from `WINDOWS_BUILD.md` in the repository root, which remains the original. If the two disagree, the original wins.

After extracting `Neloa-Windows-Source-0.1.11.zip`, double-click `build-windows.bat`. The script installs the project dependencies and produces an NSIS `.exe` using the current-user install mode.

## Install these before the first build

1. Node.js 20 or later.
2. Rust stable, with the default `x86_64-pc-windows-msvc` toolchain.
3. Visual Studio 2022 Build Tools with **Desktop development with C++** and the Windows 10/11 SDK selected.
4. Network access to download npm, Cargo, and NSIS build dependencies.

Windows 10 1803 and later usually already include WebView2; when it is missing, the installer follows Tauri's default strategy.

## Output location

A successful build leaves the installer at:

```text
src-tauri\target\release\bundle\nsis\Neloa_0.1.11_x64-setup.exe
```

The final file name can vary slightly between Tauri versions; trust whichever `.exe` is in that directory. Once the installer is produced you can delete `node_modules` and `src-tauri\target` to reclaim disk space.

## Current signing status

This is a development build and has no commercial code-signing certificate configured. Windows SmartScreen may report an unknown publisher; configure Windows code signing before distributing it publicly.
