# Neloa 移动端构建与真机验证

## 当前交付状态

| 平台 | 当前产物 | 已验证 | 尚未验证 |
| --- | --- | --- | --- |
| Android | GitHub Release 中的 ARM64 / x86_64 独立 APK | 0.1.2 的 React、Rust ARM64、Kotlin、资源与 APK 构建已通过 | 0.1.3 Actions 双 ABI 构建、Android 真机与 MuMu x86_64 安装、局域网发现、双向传输、前台剪贴板 |
| iOS | `src-tauri/gen/apple/neloa.xcodeproj` | 工程已生成；本地网络说明、Bonjour 服务与 entitlement 的 plist 语法已检查 | Xcode 编译、签名、iPhone 安装及运行时行为 |

Android APK 是便于内部测试的分架构 debug 包：ARM64 用于主流安卓真机，x86_64 用于 MuMu 等模拟器。它们使用调试签名，未针对体积优化，也不能作为应用商店发行包。iOS 必须使用完整 Xcode 和 Apple 签名，当前机器只有 Command Line Tools，因此没有生成 IPA。

## 已接入的移动端能力

- iOS 和 Android 使用与桌面端相同的 React 界面、Noise XX 身份认证、QUIC 文件协议和信任数据格式。
- Android 的设备私钥通过 Android Keystore 保存；iOS 使用 Keychain。
- 系统文件选择器返回的 Android `content://` 文件可被读取。选择时只读取信息，点击发送后才复制到应用缓存，传输完成、失败或取消后自动删除缓存副本。
- iOS 文件选择器显式使用复制模式，避免安全作用域在异步传输期间失效。
- Android 已声明网络、Wi-Fi 与多播权限，并在 Activity 存活期间持有 mDNS 多播锁。
- iOS 已声明 `NSLocalNetworkUsageDescription`、`_neloa._udp` Bonjour 服务和多播网络 entitlement。
- 移动端支持纯文本系统剪贴板，但当前产品定义为“应用在前台时同步”。Android 10 以后限制后台读取剪贴板；iOS 也可能显示系统粘贴授权提示。

## Android 真机安装

要求：ARM64 手机或平板，或 x86_64 模拟器；Android 7.0（API 24）或更新版本。

最方便的方式是把 APK 发送到手机，允许当前文件管理器“安装未知应用”，然后点 APK 安装。也可以打开 USB 调试后运行：

```bash
adb install -r Neloa_0.1.3_arm64.apk
```

0.1.3 APK 构建完成后可在下载目录校验 SHA-256：

```bash
shasum -a 256 Neloa_0.1.3_arm64.apk
```

如需在这台 Mac 上重新构建：

```bash
export JAVA_HOME=/opt/homebrew/opt/openjdk@21/libexec/openjdk.jdk/Contents/Home
export ANDROID_HOME=/opt/homebrew/share/android-commandlinetools
export NDK_HOME="$ANDROID_HOME/ndk/28.2.13676358"
export ANDROID_NDK_HOME="$NDK_HOME"
export RUSTC=/Users/example/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc
npm run tauri -- android build --debug --target aarch64 x86_64 --split-per-abi --apk --ci
```

输出位于：

```text
src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk
```

## iOS 真机构建

1. 从 App Store 安装完整 Xcode，首次打开并安装附加组件，然后执行：

   ```bash
   sudo xcode-select --switch /Applications/Xcode.app/Contents/Developer
   rustup target add aarch64-apple-ios aarch64-apple-ios-sim
   ```

2. 在 Apple Developer 账号中申请 Multicast Networking entitlement。Neloa 当前通过原始 mDNS 多播套接字发现设备，没有此能力时真机可能安装成功但无法发现附近设备。

3. 打开 `src-tauri/gen/apple/neloa.xcodeproj`，选择 `neloa_iOS` target，在 Signing & Capabilities 中选择自己的 Team，并确认 Multicast Networking 能力有效。

4. 连接已信任的 iPhone，保持 Mac 与 iPhone 在同一 Wi-Fi。可让 Tauri 打开工程：

   ```bash
   APPLE_DEVELOPMENT_TEAM=你的TeamID npm run tauri -- ios dev --open
   ```

5. 需要导出测试 IPA 时：

   ```bash
   APPLE_DEVELOPMENT_TEAM=你的TeamID npm run tauri -- ios build --export-method debugging
   ```

不要再次运行 `tauri ios init` 或 `tauri android init`，除非准备重新合并生成目录中的原生改动；Android 多播锁、iOS entitlement 和 Xcode 构建脚本都在生成工程内有定制。

## 真机验收顺序

建议首次只用一部手机和当前 Mac，按以下顺序测试：

1. 启动两端，允许 iOS“本地网络”权限或确认 Android Wi-Fi 正常，等待彼此出现在雷达页。
2. 发起配对，确认两端六位验证码一致并同时接受；重启应用后仍应显示为已配对。
3. 手机发送一个小文件到 Mac，再由 Mac 发回手机；分别接受、拒绝、取消一次。
4. 发送包含中文、空格和较长文件名的文件，确认文件名、大小及 SHA-256 校验正常。
5. 把应用保持在前台，在设置中开启剪贴板同步，测试普通短文本双向复制。
6. 测试暂停/恢复、超过 8 KB 的文本和包含 `-----BEGIN PRIVATE KEY-----` 的文本；后两者应留在本机。
7. 将手机应用切到后台，确认产品不会承诺持续剪贴板监听；重新回到前台后再继续测试。
8. 如发现失败，先在两端打开“设置 → 局域网诊断”，记录系统版本、网络类型、失败方向和错误提示。

## 已知的发布前事项

- Android debug APK 约 209 MB，主要是未优化的 Rust 调试符号；正式 release 构建会显著缩小，但必须配置自己的签名密钥。
- iOS 多播 entitlement 需要 Apple 批准；若不希望申请，后续应把 iOS 发现层改成原生 Network.framework Bonjour 适配器。
- 移动系统不允许把剪贴板同步做成与桌面端完全相同的无限后台轮询。后续可增加“回到前台自动检查”和用户主动粘贴入口。
- 当前应用标识沿用 `app.neloa.desktop`，为保持桌面端已有数据与配对身份没有在本轮更改；首次公开发布前应统一决定最终 bundle/application ID。
