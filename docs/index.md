---
layout: home
title: Neloa 文档
titleTemplate: false

hero:
  name: Neloa
  text: 文档
  tagline: 无需账号，在你的设备之间安全传输文件与剪贴板。文件与剪贴板内容始终由客户端端到端加密，中继只转发无法解读的加密数据。
  actions:
    - theme: brand
      text: 第一次使用
      link: /guide/
    - theme: alt
      text: 下载 0.1.13
      link: https://github.com/kure29/Neloa/releases/latest

features:
  - title: 第一次使用
    details: 从下载安装、两端核对六位验证码配对，到成功发出第一个文件。
    link: /guide/install
    linkText: 开始
  - title: 日常使用
    details: 多文件队列、取消与重试、校验与保存位置、剪贴板同步的边界。
    link: /guide/transfer
    linkText: 看用法
  - title: 部署与构建
    details: 自建中继的部署与接入，以及 macOS、Windows、Android、iOS 的构建方式。
    link: /build/
    linkText: 去构建
---

## 当前状态

当前稳定版本为 **0.1.13**，提供 macOS Universal DMG、Windows x64 安装包、Android ARM64 APK 和未签名 iOS IPA。macOS 和 Windows 安装包没有商业代码签名，iOS IPA 需要使用你自己的 Apple 证书重签后才能安装——这些都会在[下载与安装](/guide/install)里逐项说明。

## 连接方式

- 每台设备可以选择「自动」「局域网」或「中继」。
- 自动模式优先使用可用的局域网 QUIC 地址，否则使用已配置的中继。
- 首次配对可以走局域网或中继；无论哪条路径，都要在两端核对并确认六位验证码。

## 这份文档的组织方式

用户文档、构建说明和架构参考都写在 `docs/` 目录里；[中继服务与线协议](/reference/relay) 直接引入 `relay/README.md`，避免重复维护协议说明。每个主题只有一个真源，文档站不会和仓库里的描述各自漂移。

## 它不做什么

- 没有账号系统，也没有云端存储：设备之间的信任完全建立在本地。
- 中继只负责发现和转发加密字节；配对决定仍由两端用户完成。
- 剪贴板同步是纯文本且默认关闭的；移动端只在应用处于前台时工作。
