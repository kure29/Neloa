<div align="center">
  <img src="assets/neloa-icon.svg" alt="Neloa logo" width="104" />
  <h1>Neloa</h1>
  <p><strong>Private file and clipboard transfer for nearby devices.</strong></p>
  <p><strong>面向附近设备的私密文件与剪贴板传输工具。</strong></p>
  <p>Local-first · End-to-end encrypted · No account required</p>
  <p>本地优先 · 端到端加密 · 无需账号</p>
  <p>
    <a href="https://github.com/kure29/Neloa/actions/workflows/ci.yml"><img src="https://github.com/kure29/Neloa/actions/workflows/ci.yml/badge.svg" alt="CI status" /></a>
    <a href="https://github.com/kure29/Neloa/releases"><img src="https://img.shields.io/github/v/release/kure29/Neloa?include_prereleases&label=release" alt="Latest release" /></a>
    <img src="https://img.shields.io/badge/Tauri-2-24C8DB?logo=tauri&logoColor=white" alt="Tauri 2" />
  </p>
  <p><a href="#english">English</a> · <a href="#简体中文">简体中文</a></p>
</div>

> [!IMPORTANT]
> Neloa is under active development. macOS and Windows test builds are not production-signed yet.<br />
> Neloa 仍在积极开发中，macOS 与 Windows 测试包尚未进行正式签名。

## English

Neloa discovers devices on the same local network and transfers files or clipboard text directly between them—without accounts, cloud uploads, or persisted clipboard content.

### Highlights

- Direct LAN transfer with mDNS/Bonjour discovery
- Noise XX authenticated encryption over QUIC
- Six-digit verification before trusting a new device
- Cancellable, SHA-256-verified file transfers
- Opt-in text clipboard sync with sensitive-content safeguards
- Optional self-hosted relay fallback for previously paired devices
- Shared Tauri 2 interface for macOS, Windows, Android, and iOS

### Platform status

| macOS | Windows | Android | iOS |
| --- | --- | --- | --- |
| Universal DMG | x64 NSIS | Signed ARM64/x86_64 APKs | Local IPA; Apple signing required |

iOS 0.1.8 has completed P12 re-signing, installation, and baseline real-device validation. Its native Bonjour adapter stops and resumes with the app lifecycle. Android still needs real-device network acceptance testing.

### Quick start

Requirements: Node.js 20+, npm, Rust stable, and the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your platform.

```bash
npm install
npm run tauri dev
```

For a browser-only UI preview, run `npm run dev`. Native discovery, pairing, file access, and clipboard integration require a Tauri build.

Test packages are available from [GitHub Releases](https://github.com/kure29/Neloa/releases) after a draft release is published.

### Documentation

- [Architecture and security](ARCHITECTURE.md)
- [Android and iOS builds](MOBILE_BUILD.md)
- [Windows build](WINDOWS_BUILD.md)
- [Self-hosted relay deployment](relay/README.md)

---

## 简体中文

Neloa 可以发现同一局域网内的设备，并在设备之间直接传输文件或剪贴板文本。无需注册账号，不会把内容上传到云端，也不会持久保存剪贴板内容。

### 核心特点

- 通过 mDNS/Bonjour 发现设备并在局域网内直传
- 基于 QUIC 与 Noise XX 的身份认证和加密
- 首次信任设备前核对六位验证码
- 支持取消传输，并通过 SHA-256 校验文件
- 剪贴板同步默认关闭，并提供敏感内容保护
- 已配对设备可选用自建中继作为局域网直连的回退链路
- 使用 Tauri 2 为 macOS、Windows、Android 和 iOS 提供统一界面

### 平台状态

| macOS | Windows | Android | iOS |
| --- | --- | --- | --- |
| 通用 DMG | x64 NSIS | 已签名 ARM64/x86_64 APK | 本地 IPA，需 Apple 签名 |

iOS 0.1.8 已完成 P12 重签安装与真机基础验证；原生 Bonjour 适配器可随应用前后台状态停止和恢复。Android 仍需补充真机网络验收。

### 快速开始

需要 Node.js 20+、npm、Rust stable，以及当前平台对应的 [Tauri 环境依赖](https://v2.tauri.app/start/prerequisites/)。

```bash
npm install
npm run tauri dev
```

只预览界面可运行 `npm run dev`。真实的设备发现、配对、文件访问和剪贴板能力需要使用 Tauri 原生构建。

测试包会在草稿版本发布后显示于 [GitHub Releases](https://github.com/kure29/Neloa/releases)。

### 项目文档

- [架构与安全说明](ARCHITECTURE.md)
- [Android 与 iOS 构建说明](MOBILE_BUILD.md)
- [Windows 构建说明](WINDOWS_BUILD.md)
- [自建中继部署](relay/README.md)
