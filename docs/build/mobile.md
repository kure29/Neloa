---
title: Android 与 iOS
---

# Neloa 移动端构建与真机验证

## 当前交付状态

| 平台 | 当前产物 | 构建与安装说明 |
| --- | --- | --- |
| Android | GitHub Release 中的 ARM64 签名 Release APK | 使用项目固定签名；支持 Android 7.0（API 24）及以上 ARM64 设备 |
| iOS | `src-tauri/gen/apple/build/arm64/Neloa.ipa` | 未签名 IPA，需要 P12 证书与描述文件重签；发现使用原生 Bonjour 适配器 |

Android 发布包仅提供面向主流真机的 ARM64 APK。APK 由项目固定的 Android PKCS#12 密钥签名，并使用 Rust Release、符号剥离、Thin LTO 与 Android 代码压缩。iOS 使用完整 Xcode 生成无签名 IPA；安装到真机前仍需使用 Apple 开发证书和匹配的描述文件签名。

## 已接入的移动端能力

- iOS 和 Android 使用与桌面端相同的 React 界面、Noise XX 身份认证、QUIC 文件协议和信任数据格式。
- Android 的设备私钥通过 Android Keystore 保存；iOS 使用 Keychain。
- 系统文件选择器返回的 Android `content://` 文件可被读取。选择时只读取信息，点击发送后才复制到应用缓存，传输完成、失败或取消后自动删除缓存副本。
- iOS 文件选择器显式使用复制模式，避免安全作用域在异步传输期间失效。
- Android 已声明网络、Wi-Fi 与多播权限，并在 Activity 存活期间持有 mDNS 多播锁。
- iOS 已声明 `NSLocalNetworkUsageDescription` 与 `_neloa._udp` Bonjour 服务；发现由 `NWBrowser` 和 `NetService` 完成，不需要受限的多播网络 entitlement。
- iOS 仅在应用处于前台活动状态时运行 Bonjour；进入后台会主动停止，返回前台会重建。若系统将 mDNS 会话标记为 `DefunctConnection`，浏览器会自动重连。
- 移动端支持纯文本系统剪贴板，但当前产品定义为“应用在前台时同步”。Android 10 以后限制后台读取剪贴板；iOS 也可能显示系统粘贴授权提示。

## Android 真机安装

要求：ARM64 手机或平板；Android 7.0（API 24）或更新版本。

最方便的方式是把 APK 发送到手机，允许当前文件管理器“安装未知应用”，然后点 APK 安装。也可以打开 USB 调试后运行：

```bash
adb install -r Neloa_<版本号>_arm64.apk
```

APK 下载完成后可在下载目录校验 SHA-256：

```bash
shasum -a 256 Neloa_<版本号>_arm64.apk
```

## Android Release 签名

首次配置只运行一次：

```bash
./scripts/setup-android-signing.sh
```

脚本会隐藏密码输入，在 `~/Documents/Neloa-signing/neloa-release.p12` 创建别名为 `neloa` 的独立 Android 密钥，并把密钥的 Base64 与密码写入仓库的 `ANDROID_KEYSTORE_BASE64`、`ANDROID_KEYSTORE_PASSWORD` GitHub Actions Secrets。密钥及密码不会写入 Git 历史。

必须同时备份 `.p12` 文件和密码。后续所有更新都要使用同一个密钥；丢失后无法覆盖升级已经安装的应用。要换一个保管目录，可在运行脚本前设置 `NELOA_SIGNING_DIR`。

配置完成后，从 GitHub Actions 手动运行 `Build Installers`，填写新的发布标签。工作流会在临时目录解码密钥、生成签名 Release APK，并在任务结束时随运行器销毁临时文件。

如需在这台 Mac 上重新构建：

```bash
export JAVA_HOME=/opt/homebrew/opt/openjdk@21/libexec/openjdk.jdk/Contents/Home
export ANDROID_HOME=/opt/homebrew/share/android-commandlinetools
export NDK_HOME="$ANDROID_HOME/ndk/28.2.13676358"
export ANDROID_NDK_HOME="$NDK_HOME"
export ANDROID_KEYSTORE_PATH="$HOME/Documents/Neloa-signing/neloa-release.p12"
read -s ANDROID_KEYSTORE_PASSWORD
export ANDROID_KEYSTORE_PASSWORD
npm run tauri -- android build --target aarch64 --split-per-abi --apk --ci
unset ANDROID_KEYSTORE_PASSWORD
```

输出位于：

```text
src-tauri/gen/android/app/build/outputs/apk/universal/release/app-universal-release.apk
```

## iOS 真机构建

1. 从 App Store 安装完整 Xcode，首次打开并安装附加组件，然后执行：

   ```bash
   sudo xcode-select --switch /Applications/Xcode.app/Contents/Developer
   rustup target add aarch64-apple-ios aarch64-apple-ios-sim
   ```

2. 应用标识已统一为 `com.kure29.neloa`。从旧标识版本升级时，需要先卸载旧应用再安装一次。

3. 打开 `src-tauri/gen/apple/neloa.xcodeproj`，选择 `neloa_iOS` target，在 Signing & Capabilities 中选择自己的 Team并开启自动签名。无需添加 Multicast Networking 能力。

4. 连接已信任的 iPhone，保持 Mac 与 iPhone 在同一 Wi-Fi。可让 Tauri 打开工程：

   ```bash
   APPLE_DEVELOPMENT_TEAM=你的TeamID npm run tauri -- ios dev --open
   ```

5. 需要导出测试 IPA 时：

   ```bash
   APPLE_DEVELOPMENT_TEAM=你的TeamID npm run tauri -- ios build --export-method debugging
   ```

   生成用于 GitHub Release、由下载者自行重签的未签名 IPA：

   ```bash
   env CARGO_BUILD_RUSTFLAGS="--remap-path-prefix=$HOME=/Users/build" \
     PATH="$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin:$PATH" \
     npm run tauri -- ios build --no-sign --ci
   ```

   `CARGO_BUILD_RUSTFLAGS` 会把 Rust 二进制中的本机主目录重映射为通用路径，避免发布包携带构建机用户名。

不要再次运行 `tauri ios init` 或 `tauri android init`，除非准备重新合并生成目录中的原生改动；Android 多播锁、iOS Bonjour 适配层和 Xcode 构建脚本都在生成工程内有定制。

## 真机验收顺序

建议首次只用一部手机和当前 Mac，按以下顺序测试：

1. 启动两端，允许 iOS“本地网络”权限或确认 Android Wi-Fi 正常，等待彼此出现在设备页。
2. 发起配对，确认两端六位验证码一致并同时接受；重启应用后仍应显示为已配对。
3. 手机发送一个小文件到 Mac，再由 Mac 发回手机；分别接受、拒绝、取消一次。
4. 发送包含中文、空格和较长文件名的文件，确认文件名、大小及 SHA-256 校验正常。
5. 把应用保持在前台，在设置中开启剪贴板同步，测试普通短文本双向复制。
6. 测试暂停/恢复、超过 8 KB 的文本和包含 `-----BEGIN PRIVATE KEY-----` 的文本；后两者应留在本机。
7. 将手机应用切到后台，确认产品不会承诺持续剪贴板监听；重新回到前台后再继续测试。
8. 如发现失败，先在两端打开“设置 → 连接 → 连接状态”，记录系统版本、网络类型、失败方向和错误提示。

## 自建中继真机验收

中继可以承载首次配对和后续传输，按下面顺序验证：

1. 将中继部署在有效 HTTPS 证书之后，准备公共 `wss://.../v1/ws` 地址和同一个至少 32 字符的令牌。
2. 在两端打开“设置 → 连接 → 自建中继”，填入相同地址和令牌并保存，确认状态为“已连接”。
3. 让手机切到蜂窝网络或另一个 Wi-Fi；设备出现后选择「中继」，核对六位验证码并完成首次配对。
4. 在自动、局域网和中继之间切换，双向发送测试文本、小文件和较大文件，再验证前台剪贴板同步；传输中的加密、确认、校验和取消行为应保持一致。
5. 临时输入错误令牌、停止中继服务并重新启动，确认客户端给出可读错误并自动重连，同时局域网直连仍可使用。
6. 使用无效或过期 TLS 证书时连接必须失败；不要通过公网暴露明文 WebSocket 端口。

## 平台限制与兼容说明

- Android Release APK 使用同一永久证书；首次从旧 Debug 签名迁移仍需卸载重装。
- iOS 发现层使用原生 Bonjour 适配器，不依赖需要 Apple 额外批准的多播 entitlement。`DefunctConnection` 生命周期恢复和 Swift/Rust 静态启动桥已通过 P12 重签后的真机基础验证。
- 移动系统不允许把剪贴板同步做成与桌面端完全相同的无限后台轮询；当前仅在应用前台同步。
- iOS 与 Android 应用标识统一为 `com.kure29.neloa`。首次从旧标识迁移时，系统会将其视为一个新应用。
