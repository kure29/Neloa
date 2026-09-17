---
layout: home
title: Neloa 说明书
titleTemplate: false

hero:
  name: Neloa
  text: 说明书
  tagline: 无需账号，在你的设备之间安全传输文件与剪贴板。文件与剪贴板内容始终由客户端端到端加密，中继只转发无法解读的加密数据。
  actions:
    - theme: brand
      text: 下载 0.1.12
      link: https://github.com/kure29/Neloa/releases/latest
    - theme: alt
      text: 第一次使用
      link: /guide/

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

当前稳定版本为 **0.1.12**，提供 macOS Universal DMG、Windows x64 安装包、Android ARM64/x86_64 APK 和未签名 iOS IPA。macOS 和 Windows 安装包没有商业代码签名，iOS IPA 需要使用你自己的 Apple 证书重签后才能安装——这些都会在[下载与安装](/guide/install)里逐项说明。

## 0.1.12 更新

- 发现到局域网地址的设备固定使用 QUIC 直连，不再因为本地错误静默绕到中继。
- 首次配对仅允许通过局域网完成，中继只连接已经配对的设备。
- macOS 包新增本地网络与 Bonjour 权限声明，并补充配对超时的可操作提示。

## 这份说明书的组织方式

用户手册（指南、构建、项目三部分）写在 `docs/` 目录里；而 [架构与安全边界](/reference/architecture)、[中继服务与线协议](/reference/relay)、[Windows 构建](/build/windows)、[移动端构建](/build/mobile) 直接引入仓库中的原始文档，不做二次转写。这样每个主题只有一个真源，文档站不会和仓库里的描述各自漂移。

## 它不做什么

- 没有账号系统，也没有云端存储：设备之间的信任完全建立在本地。
- 中继不参与配对。想让两台设备通过中继通信，必须先在局域网完成一次配对。
- 剪贴板同步是纯文本且默认关闭的；移动端只在应用处于前台时工作。
