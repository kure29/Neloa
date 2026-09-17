---
title: 下载与安装
---

# 下载与安装

正式构建位于 [GitHub Releases 最新版本](https://github.com/kure29/Neloa/releases/latest)。请按设备选择对应文件；Release 说明中列有全部 SHA-256。

| 平台 | 0.1.12 下载文件 | 安装说明 |
| --- | --- | --- |
| macOS | `Neloa_0.1.12_universal.dmg` | Apple Silicon 与 Intel；ad-hoc 签名，系统可能要求手动确认打开 |
| Windows | `Neloa_0.1.12_x64-setup.exe` | x64 NSIS；没有商业签名，SmartScreen 可能显示“未知发布者” |
| Android 真机 | `Neloa_0.1.12_arm64.apk` | ARM64；使用项目固定密钥签名 |
| iOS | `Neloa_0.1.12_unsigned.ipa` | 需要使用自己的 P12 证书与描述文件重签，不能直接安装 |

## 关于签名

这一节值得单独读，因为它决定了你会看到哪些系统提示。

- **macOS** 的 DMG 是 ad-hoc 签名。首次打开可能被 Gatekeeper 拦下，需要在「系统设置 → 隐私与安全性」中确认打开。
- **Windows** 的 NSIS 安装包没有商业代码签名，SmartScreen 会提示"未知发布者"，需要选择"仍要运行"。
- **Android** 的 APK 由项目固定的 PKCS#12 密钥签名。首次从旧版本或调试签名迁移时需要卸载重装；此后所有更新都必须使用同一个密钥，否则无法覆盖升级。
- **iOS** 分发的是未签名 IPA。你需要用自己的 Apple 开发证书和匹配的描述文件重签，步骤见[移动端构建](/build/mobile)。

## 首次启动与局域网权限

- macOS/iOS 首次启动时请允许 Neloa 访问“本地网络”；曾经拒绝过时，需要到系统设置中重新开启。
- Windows/macOS 防火墙需要允许 Neloa 接收入站连接。发现使用 Bonjour/mDNS，实际传输使用 UDP 48631。
- 首次配对只能在局域网内完成。设备已经配对且没有可用局域网路由时，才会使用配置好的中继。

## Android 安装

把 APK 发送到手机，允许当前文件管理器"安装未知应用"，然后点 APK 安装。也可以打开 USB 调试后运行：

```bash
adb install -r Neloa_<版本号>_arm64.apk
```

下载完成后可以在下载目录校验 SHA-256：

```bash
shasum -a 256 Neloa_<版本号>_arm64.apk
```

## 关于应用标识

应用标识已统一为 `com.kure29.neloa`。桌面端首次在新标识下启动时，会从旧的 `app.neloa.desktop` 数据目录**复制**缺失的设备身份、信任列表和剪贴板设置，不会覆盖新版本已经写入的数据。移动端系统把新标识视为一个全新应用，因此升级仍需重新安装并重新配对。
