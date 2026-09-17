---
title: Download and install
---

# Download and install

Stable builds live on the [latest GitHub Release](https://github.com/kure29/Neloa/releases/latest). Pick the file that matches your device; the release notes list every SHA-256.

| Platform | 0.1.12 asset | Install notes |
| --- | --- | --- |
| macOS | `Neloa_0.1.12_universal.dmg` | Apple Silicon and Intel; ad-hoc signed, so the system may ask you to confirm before opening |
| Windows | `Neloa_0.1.12_x64-setup.exe` | x64 NSIS; not commercially signed, so SmartScreen may report an unknown publisher |
| Android device | `Neloa_0.1.12_arm64.apk` | ARM64; signed with the project's fixed key |
| Android emulator | `Neloa_0.1.12_x86_64.apk` | For selected x86_64 emulators |
| iOS | `Neloa_0.1.12_unsigned.ipa` | Must be re-signed with your own P12 certificate and provisioning profile |

## About signing

This section is worth reading on its own, because it decides which system prompts you will see.

- **macOS** builds are ad-hoc signed. Gatekeeper may block the first launch; confirm the app under System Settings → Privacy & Security.
- **Windows** installers have no commercial code signature. SmartScreen will warn about an unknown publisher, and you have to choose to run it anyway.
- **Android** APKs are signed with the project's fixed PKCS#12 key. Migrating from an older or debug-signed build requires uninstalling first; after that, every update must use the same key or it cannot be installed over the existing app.
- **iOS** ships an unsigned IPA. You need to re-sign it with your own Apple development certificate and a matching provisioning profile — see [Android and iOS](/en/build/mobile).

## First launch and Local Network access

- Allow Local Network access when macOS or iOS first asks. If you denied it earlier, re-enable Neloa in system settings.
- The macOS/Windows firewall must allow inbound Neloa connections. Discovery uses Bonjour/mDNS; transfers use UDP 48631.
- Initial pairing is LAN-only. A configured relay is used only for a previously paired device with no available LAN route.

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
