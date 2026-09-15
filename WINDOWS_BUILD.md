# 在 Windows 上生成 Neloa 安装程序

解压 `Neloa-Windows-Source-0.1.11.zip` 后，双击 `build-windows.bat`。脚本会安装项目依赖并生成当前用户安装模式的 NSIS `.exe`。

## 第一次构建前需要安装

1. Node.js 20 或更高版本。
2. Rust stable，选择默认的 `x86_64-pc-windows-msvc` 工具链。
3. Visual Studio 2022 Build Tools，勾选 **Desktop development with C++** 和 Windows 10/11 SDK。
4. 可联网下载 npm、Cargo 和 NSIS 构建依赖。

Windows 10 1803 及更高版本通常已经包含 WebView2；如果系统缺失，安装程序会按 Tauri 的默认策略处理。

## 输出位置

构建成功后，安装程序位于：

```text
src-tauri\target\release\bundle\nsis\Neloa_0.1.11_x64-setup.exe
```

最终文件名可能随 Tauri 版本略有不同，以该目录内的 `.exe` 为准。安装包生成后，可以删除 `node_modules` 和 `src-tauri\target` 回收空间。

## 当前签名状态

这是开发测试包，暂未配置商业代码签名证书。Windows SmartScreen 可能显示“未知发布者”；用于公开分发前应配置 Windows 代码签名。
