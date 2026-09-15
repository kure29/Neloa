---
title: Download and install
---

# Download and install

Stable builds live on [GitHub Releases](https://github.com/kure29/Neloa/releases). Pick the file that matches your device.

| Platform | Build | Install notes |
| --- | --- | --- |
| macOS | Universal DMG | Apple Silicon and Intel; ad-hoc signed today, so the system may ask you to confirm before opening |
| Windows | x64 NSIS | Not commercially signed, so SmartScreen may report an unknown publisher |
| Android | ARM64 / x86_64 APK | ARM64 for most physical devices, x86_64 for some emulators; signed with the project's fixed key |
| iOS | Unsigned IPA | Must be re-signed with your own P12 certificate and provisioning profile; it cannot be installed as-is |

## About signing

This section is worth reading on its own, because it decides which system prompts you will see.

- **macOS** builds are ad-hoc signed. Gatekeeper may block the first launch; confirm the app under System Settings → Privacy & Security.
- **Windows** installers have no commercial code signature. SmartScreen will warn about an unknown publisher, and you have to choose to run it anyway.
- **Android** APKs are signed with the project's fixed PKCS#12 key. Migrating from an older or debug-signed build requires uninstalling first; after that, every update must use the same key or it cannot be installed over the existing app.
- **iOS** ships an unsigned IPA. You need to re-sign it with your own Apple development certificate and a matching provisioning profile — see [Android and iOS](/en/build/mobile).

## Installing on Android

Send the APK to your phone, allow the current file manager to install unknown apps, then tap the APK. With USB debugging enabled you can also run:

```bash
adb install -r Neloa_<version>_arm64.apk
```

You can verify the SHA-256 of a downloaded APK:

```bash
shasum -a 256 Neloa_<version>_arm64.apk
```

## About the application identifier

The application identifier is now `com.kure29.neloa`. On first launch under the new identifier, desktop builds **copy** a missing device identity, trust store, and clipboard settings from the legacy `app.neloa.desktop` data directory; they never overwrite data the new version has already written. Mobile systems treat the new identifier as a different app, so mobile upgrades still require a fresh install and a fresh pairing.
