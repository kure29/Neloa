---
title: Android and iOS
---

# Mobile builds and on-device verification

Translated from `MOBILE_BUILD.md` in the repository root, which remains the original. If the two disagree, the original wins.

## Current delivery status

| Platform | Current artifact | Verified | Not yet verified |
| --- | --- | --- | --- |
| Android | Split ARM64 / x86_64 signed release APKs on GitHub Releases | Fixed signing, both ABIs, icons, and checksums confirmed inside the packages | Multi-file, local-network, and relay regression on real devices for 0.1.11 |
| iOS | `src-tauri/gen/apple/build/arm64/Neloa.ipa` | P12 re-signing and installation, the native Bonjour/Rust startup bridge, and a basic on-device flow | Multi-file, long transfers, and foreground/background regression for 0.1.11 |

Android APKs are split per ABI: ARM64 for mainstream phones, x86_64 for emulators such as MuMu. The APKs are signed with the project's fixed Android PKCS#12 key and built with Rust release mode, symbol stripping, Thin LTO, and Android code shrinking. iOS produces an unsigned IPA through the full Xcode flow; installing it on a device still requires an Apple development certificate and a matching provisioning profile.

## Mobile capabilities that are wired up

- iOS and Android use the same React interface, Noise XX identity authentication, QUIC file protocol, and trust data format as the desktop clients.
- The Android device private key is stored in the Android Keystore; iOS uses the Keychain.
- Android `content://` files returned by the system file picker can be read. Selecting one only reads its metadata; the file is copied into the app cache when you press send, and the cached copy is deleted once the transfer completes, fails, or is cancelled.
- The iOS file picker explicitly uses copy mode, so the security scope cannot expire during an asynchronous transfer.
- Android declares the network, Wi-Fi, and multicast permissions and holds the mDNS multicast lock while the Activity is alive.
- iOS declares `NSLocalNetworkUsageDescription` and the `_neloa._udp` Bonjour service; discovery uses `NWBrowser` and `NetService` and needs no restricted multicast entitlement.
- iOS runs Bonjour only while the app is active in the foreground. It stops on entering the background and rebuilds on return. If the system marks the mDNS session as `DefunctConnection`, the browser reconnects automatically.
- Mobile supports plain-text system clipboard, but the product definition today is "sync while the app is in the foreground". Android 10 and later restrict background clipboard reads, and iOS may show a system paste permission prompt.

## Installing on an Android device

Requirements: an ARM64 phone or tablet, or an x86_64 emulator; Android 7.0 (API 24) or later.

The easiest route is to send the APK to the phone, allow the current file manager to install unknown apps, and tap the APK. With USB debugging enabled you can also run:

```bash
adb install -r Neloa_<version>_arm64.apk
```

After downloading, verify the SHA-256 in the Downloads directory:

```bash
shasum -a 256 Neloa_<version>_arm64.apk
```

## Android release signing

Configure this once:

```bash
./scripts/setup-android-signing.sh
```

The script hides password input, creates a separate Android key with the alias `neloa` at `~/Documents/Neloa-signing/neloa-release.p12`, and writes the key's Base64 and password into the `ANDROID_KEYSTORE_BASE64` and `ANDROID_KEYSTORE_PASSWORD` GitHub Actions secrets. Neither the key nor the password is written into Git history.

Back up both the `.p12` file and its password. Every later update must use the same key; if it is lost, you can no longer upgrade an installed app in place. Set `NELOA_SIGNING_DIR` before running the script to keep the key somewhere else.

Once configured, run the `Build Installers` workflow manually from GitHub Actions and supply a new release tag. The workflow decodes the key into a temporary directory, produces the signed release APKs, and destroys the temporary files when the job ends along with the runner.

To rebuild on this Mac instead:

```bash
export JAVA_HOME=/opt/homebrew/opt/openjdk@21/libexec/openjdk.jdk/Contents/Home
export ANDROID_HOME=/opt/homebrew/share/android-commandlinetools
export NDK_HOME="$ANDROID_HOME/ndk/28.2.13676358"
export ANDROID_NDK_HOME="$NDK_HOME"
export RUSTC=/Users/example/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc
export ANDROID_KEYSTORE_PATH="$HOME/Documents/Neloa-signing/neloa-release.p12"
read -s ANDROID_KEYSTORE_PASSWORD
export ANDROID_KEYSTORE_PASSWORD
npm run tauri -- android build --target aarch64 x86_64 --split-per-abi --apk --ci
unset ANDROID_KEYSTORE_PASSWORD
```

The output lands in:

```text
src-tauri/gen/android/app/build/outputs/apk/universal/release/app-universal-release.apk
```

## Building for an iOS device

1. Install the full Xcode from the App Store, open it once and install the additional components, then run:

   ```bash
   sudo xcode-select --switch /Applications/Xcode.app/Contents/Developer
   rustup target add aarch64-apple-ios aarch64-apple-ios-sim
   ```

2. The application identifier is now `com.kure29.neloa`. Upgrading from the old identifier requires uninstalling the old app and installing once more.

3. Open `src-tauri/gen/apple/neloa.xcodeproj`, select the `neloa_iOS` target, choose your own Team under Signing & Capabilities, and enable automatic signing. No Multicast Networking capability needs to be added.

4. Connect a trusted iPhone and keep the Mac and the iPhone on the same Wi-Fi. Let Tauri open the project with:

   ```bash
   APPLE_DEVELOPMENT_TEAM=yourTeamID npm run tauri -- ios dev --open
   ```

5. To export a test IPA:

   ```bash
   APPLE_DEVELOPMENT_TEAM=yourTeamID npm run tauri -- ios build --export-method debugging
   ```

Do not run `tauri ios init` or `tauri android init` again unless you intend to re-merge the native changes in the generated directories: the Android multicast lock, the iOS Bonjour adapter, and the Xcode build script all live in there.

## On-device acceptance order

For a first pass, use one phone and the current Mac, in this order:

1. Start both ends, allow the iOS Local Network permission or confirm Android Wi-Fi is healthy, and wait for each to appear on the other's devices screen.
2. Start pairing, confirm the six-digit codes match and accept on both ends; after restarting the apps they should still show as paired.
3. Send a small file from the phone to the Mac and back again; accept, reject, and cancel once each.
4. Send a file whose name contains Chinese characters, spaces, and a long file name, then confirm the name, size, and SHA-256 verification.
5. Keep the app in the foreground, enable clipboard sync in settings, and test ordinary short text in both directions.
6. Test pause/resume, a text longer than 8 KB, and text containing `-----BEGIN PRIVATE KEY-----`; the latter two should stay local.
7. Move the app to the background and confirm the product never promises continuous clipboard monitoring; continue testing after returning to the foreground.
8. If something fails, open Settings → Connection → Technical details on both ends first and record the OS version, network type, failure direction, and error message.

## Relay acceptance on real devices

The relay never replaces the first pairing. Complete the local pairing above first, then verify across networks:

1. Deploy the relay behind a valid HTTPS certificate, and prepare a public `wss://.../v1/ws` address plus one token of at least 32 characters.
2. On both devices open Settings → Connection → Self-hosted relay, enter the same address and token, save, and confirm the status reads connected.
3. Move the phone to cellular or another Wi-Fi; the peer should reappear and show "paired · relay".
4. Send test text, a small file, and a larger file in both directions, then verify foreground clipboard sync; encryption, acknowledgement, verification, and cancellation should behave exactly as they do on a local network.
5. Temporarily enter a wrong token, stop the relay, and start it again; confirm the client reports a readable error and reconnects automatically, and that local transfers keep working.
6. Connections must fail with an invalid or expired TLS certificate. Never expose a plaintext WebSocket port to the internet.

## Known pre-release items

- Android release APKs use one permanent certificate; a first migration from an older debug signature still requires uninstalling and reinstalling.
- The iOS discovery layer uses a native Bonjour adapter and does not depend on the multicast entitlement that requires extra Apple approval. `DefunctConnection` lifecycle recovery and the Swift/Rust static startup bridge have passed basic on-device verification after P12 re-signing.
- 0.1.11 adds multi-file selection and batch sending; multi-select, consecutive accepts, rejections, cancellation, and relay transfers should each be verified on Android and iOS before release.
- Mobile systems do not allow clipboard sync to become the same unlimited background polling the desktop uses. "Check again on returning to the foreground" and an explicit paste entry point are candidates for a later release.
- The iOS and Android application identifier is `com.kure29.neloa`. On the first migration from the old identifier, the system treats it as a new app.
