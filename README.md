<div align="center">
  <img src="assets/neloa-icon.svg" alt="Neloa" width="104" />
  <h1>Neloa</h1>
  <p><strong>无需账号，在你的设备之间安全传输文件与剪贴板。</strong></p>
  <p>Local-first encrypted file and clipboard transfer—without an account.</p>
  <p>
    <a href="https://github.com/kure29/Neloa/actions/workflows/ci.yml"><img src="https://github.com/kure29/Neloa/actions/workflows/ci.yml/badge.svg" alt="CI" /></a>
    <a href="https://github.com/kure29/Neloa/releases/latest"><img src="https://img.shields.io/github/v/release/kure29/Neloa?label=release" alt="Latest release" /></a>
    <a href="LICENSE"><img src="https://img.shields.io/github/license/kure29/Neloa" alt="Apache License 2.0" /></a>
    <img src="https://img.shields.io/badge/Tauri-2-24C8DB?logo=tauri&logoColor=white" alt="Tauri 2" />
  </p>
  <p><a href="https://kure29.github.io/Neloa/"><strong>说明书</strong></a> · <a href="#简体中文">简体中文</a> · <a href="#english">English</a></p>
</div>

> [!IMPORTANT]
> 仍在积极开发中，当前版本 **0.1.11**。macOS 与 Windows 安装包没有商业代码签名，iOS IPA 需要用你自己的 Apple 证书重签后安装。

## 简体中文

Neloa 优先使用局域网直连；设备不在同一网络时，也可以通过你自己的中继服务器建立连接。文件和剪贴板内容始终由客户端端到端加密，中继只转发无法解读的加密数据。

**安装、配对、传输、剪贴板同步、中继部署、故障排查与全平台构建步骤都在说明书里：<https://kure29.github.io/Neloa/>**（[English](https://kure29.github.io/Neloa/en/)）

说明书提供完整的中文与英文两个版本，各自独立成篇。

### 下载与安装

正式构建位于 [GitHub Releases](https://github.com/kure29/Neloa/releases)。请按设备选择对应文件：

| 平台 | 构建 | 安装说明 |
| --- | --- | --- |
| macOS | Universal DMG | 兼容 Apple Silicon 与 Intel；ad-hoc 签名，系统可能要求手动确认打开 |
| Windows | x64 NSIS | 没有商业签名，SmartScreen 可能显示“未知发布者” |
| Android | ARM64 / x86_64 APK | ARM64 用于真机，x86_64 用于部分模拟器；使用项目固定密钥签名 |
| iOS | 未签名 IPA | 需要自己的 P12 证书与描述文件重签，不能直接安装 |

### 文档

说明书覆盖使用与构建；下面这几份是各自主题的唯一真源，也一并收进了说明书：

| 主题 | 位置 |
| --- | --- |
| 使用手册（安装 / 配对 / 传输 / 剪贴板 / 中继 / 排查） | [docs/guide/](docs/guide/index.md) |
| 架构与威胁模型 | [ARCHITECTURE.md](ARCHITECTURE.md) |
| 自建中继部署与线协议 | [relay/README.md](relay/README.md) |
| 移动端构建与真机验收 | [MOBILE_BUILD.md](MOBILE_BUILD.md) |
| Windows 安装程序 | [WINDOWS_BUILD.md](WINDOWS_BUILD.md) |

### 本地开发

需要 Node.js 20+、npm、Rust stable，以及对应平台的 [Tauri 2 环境依赖](https://v2.tauri.app/start/prerequisites/)。

```bash
npm install
npm run tauri dev     # 桌面客户端
npm run docs:dev      # 本地预览说明书
```

常用命令还有 `npm run dev`（仅预览前端界面，不含原生能力）、`npm run tauri -- build`、`npm run android:build`、`npm run ios:build`。移动端生成工程包含自定义原生桥接代码，不要随意重新运行 `tauri android init` 或 `tauri ios init`。

## English

Neloa transfers files and plain-text clipboard updates between your own devices. It prefers direct LAN QUIC connections and falls back to a self-hosted WebSocket relay for devices already paired locally; application data stays protected by Noise XX end-to-end encryption on either route.

**The manual covers installation, pairing, transfers, clipboard sync, relay deployment, troubleshooting, and per-platform builds: <https://kure29.github.io/Neloa/en/>** ([简体中文](https://kure29.github.io/Neloa/))

Download stable packages from [GitHub Releases](https://github.com/kure29/Neloa/releases). Desktop packages are not commercially signed, and the iOS IPA must be re-signed with your own Apple certificate and provisioning profile.

To develop locally you need Node.js 20+, npm, and Rust stable:

```bash
npm install
npm run tauri dev
```

See [ARCHITECTURE.md](ARCHITECTURE.md) for the threat model and [relay/README.md](relay/README.md) for relay deployment.

## License

Neloa is available under the [Apache License 2.0](LICENSE).
