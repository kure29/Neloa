---
title: Windows 安装程序
---

# 构建 Windows 安装程序

正式的 Windows x64 NSIS 安装程序由 GitHub Actions 的 **Build Installers** 工作流在 `windows-latest` 上构建，并上传到对应的草稿 Release。这样不需要分发额外的源码压缩包或批处理文件，构建入口也与 macOS、Android 保持一致。

## 使用 GitHub Actions

1. 打开仓库的 **Actions → Build Installers**。
2. 选择 **Run workflow**，填写要生成的发布标签。
3. 等待 `Windows x64` 任务完成。
4. 在对应的草稿 Release 中下载 `.exe`。

推送 `v*` 标签时也会自动运行同一工作流。

## 在 Windows 本机构建

### 第一次构建前需要安装

1. Node.js 20 或更高版本。
2. Rust stable，选择默认的 `x86_64-pc-windows-msvc` 工具链。
3. Visual Studio 2022 Build Tools，勾选 **Desktop development with C++** 和 Windows 10/11 SDK。
4. 可联网下载 npm、Cargo 和 NSIS 构建依赖。

在仓库根目录运行：

```powershell
npm ci
npm run tauri -- build --bundles nsis
```

Windows 10 1803 及更高版本通常已经包含 WebView2；如果系统缺失，安装程序会按 Tauri 的默认策略处理。

## 输出位置

本地构建成功后，安装程序位于：

```text
src-tauri\target\release\bundle\nsis\Neloa_0.1.12_x64-setup.exe
```

最终文件名可能随 Tauri 版本略有不同，以该目录内的 `.exe` 为准。构建产物不会提交到 Git；正式交付以 GitHub Release 中的文件为准。

## 当前签名状态

这是开发测试包，暂未配置商业代码签名证书。Windows SmartScreen 可能显示“未知发布者”；用于公开分发前应配置 Windows 代码签名。
