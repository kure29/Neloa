---
title: 从源码构建
---

# 从源码构建

## 环境要求

需要 Node.js 20+、npm、Rust stable，以及对应平台的 [Tauri 2 环境依赖](https://v2.tauri.app/start/prerequisites/)。

```bash
npm install
npm run tauri dev
```

## 常用命令

| 命令 | 用途 |
| --- | --- |
| `npm run dev` | 仅预览前端界面，不包含原生网络和文件能力 |
| `npm run tauri dev` | 运行桌面客户端 |
| `npm run tauri -- build` | 生成当前桌面平台安装包 |
| `npm run android:build` | 构建 Android 客户端 |
| `npm run ios:build` | 构建 iOS 客户端 |

::: warning 不要重新初始化原生工程
移动端生成工程里包含自定义的原生桥接代码（Android 多播锁、iOS Bonjour 适配层、Xcode 构建脚本）。除非你准备重新合并生成目录里的改动，否则不要重新运行 `tauri android init` 或 `tauri ios init`。
:::

## 持续集成

`.github/workflows/ci.yml` 在每次推送到 `main` 和每个 pull request 上运行：

- `npm run build`（前端类型检查与打包）
- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test --lib`（客户端 Rust 核心）

安装包由 `.github/workflows/build-installers.yml` 手动触发，填写新的发布标签后生成各平台产物。

## 分平台说明

- [Windows 安装程序](/build/windows) — 使用 GitHub Actions 或在 Windows 本机构建 NSIS 安装程序。
- [Android 与 iOS](/build/mobile) — 签名配置、真机构建与真机验收顺序。
