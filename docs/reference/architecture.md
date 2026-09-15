---
title: 架构与安全边界
---

# Neloa 架构

本页是仓库根目录 `ARCHITECTURE.md` 的中文版。**英文原文是唯一真源**，两者出现分歧时以原文为准；切换到 English 可以看到原文。

Neloa 是本地优先的。每个客户端在没有账号、没有互联网连接的情况下都应该保持可用。

## 当前里程碑

- 所有平台共用一套 React 界面和一套设计系统。`DesktopShell` 与 `MobileShell` 共享 `views/` 和 `ui/`；macOS 与 Windows 的差别只在窗口控件、系统字体栈和外框圆角。颜色来自一组 CSS 自定义属性，每个属性有亮色和暗色两个值，因此组件规则从不判断配色方案。
- Tauri 2 桌面与移动外壳，带有生成好的 Android 和 iOS 原生工程。
- 稳定的 X25519 设备身份，保存在操作系统凭据库中：Keychain、凭据管理器、Android Keystore 或 iOS Keychain。
- `_neloa._udp.local.` 服务广播与发现：桌面与 Android 使用 `mdns-sd`，iOS 使用 Apple Bonjour 适配层。
- 从 Rust 到界面层的实时对端更新。
- 同时承担 QUIC 客户端与服务器角色的端点，监听 UDP 端口 48631。
- 每条 QUIC 流上运行 Noise XX 握手。
- 由 Noise 握手摘要推导出的六位短认证字符串。
- 在持久化对端公钥之前，两台设备都必须显式确认。
- 已信任设备的校验与撤销。
- 端到端加密的测试文本，带应用层确认。
- 原生多文件选择、桌面拖放、有界发送队列与文件元数据检查。Android 的 content URI 通过平台文件描述符桥接打开，仅在发送时暂存，传输结束后删除；iOS 使用沙盒副本。
- 与已信任设备身份绑定的文件 offer/accept 流程。
- 带严格偏移、大小限制、进度与取消的加密二进制分块传输。
- 发布前的 SHA-256 校验，以及一条经过认证的完成回执。
- 先写临时 `.part` 文件，再以原子方式、不覆盖地发布到 `Downloads/Neloa`。
- 传输结果历史，以及中断的待发送文件重试。
- 可选的剪贴板监听，运行在专用线程上并使用原生系统剪贴板。移动端同步只在前台工作，因为操作系统限制后台读取剪贴板。
- 通过经过认证的 Noise 会话，把纯文本剪贴板事件发送给每一台在线的已信任设备。
- 事件 UUID 去重，以及归一化的换行符比较，防止回环广播。
- 8 KB 负载上限、强凭据标记拦截，以及不持久化剪贴板正文。
- 发现阶段和经过认证的握手中都携带协议范围与能力协商。
- 对遗留、畸形或能力不兼容的对端，在接受应用数据之前显式拒绝。
- 面向用户的本地连接状态，覆盖 QUIC、mDNS、身份、剪贴板、对端能力和防火墙检查。
- 可选的自建中继客户端，带持久化的经过认证的 WebSocket 连接、已信任设备在线状态和有界虚拟流。
- 局域网优先的传输选择：只要存在本地地址就先尝试 QUIC，只有在已经信任的在线设备上才回退到中继。

广播会声明 `protocolVersion=1`、`minProtocolVersion=1` 和 `capabilities=discovery,pairing,noise-xx,test-message,file-transfer,clipboard-text`。同一份协议范围与能力列表也会在 Noise XX 握手中被认证；mDNS 里的值只是展示和早期过滤用的提示。

## 界面布局

```text
src/
  bridge.ts        typed Tauri commands/events，以及浏览器预览模拟
  lib/useNeloa.ts  全部应用状态与动作；两个外壳共用
  ui/              图标、基础组件、浮层
  views/           RadarView | HistoryView | SettingsView —— 与外壳无关
  shell/           DesktopShell（标题栏） | MobileShell（标签栏、安全区）
  styles/          tokens | base | components | desktop | mobile
```

除了通过 `useShell()` 在居中对话框与底部面板之间切换之外，`views/` 和 `ui/` 里没有平台或外壳分支。外壳由前端根据 `?platform=`、指针类型、视口宽度和 user agent 选择。Rust 后端报告 `macos`、`windows`、`linux`、`ios` 或 `android`，协议行为与外壳无关。

## 安全边界

QUIC 使用一份每次启动生成的自签名证书，仅作为可靠的加密数据报传输，不作为长期设备身份。可选中继把每条隧道暴露为同一个双向字节流接口。所有应用负载都由 Noise XX 包裹，任何明文应用消息都不允许直接写入两种传输中的任何一种。

远程中继地址必须使用 `wss://`；明文 `ws://` 只在回环开发服务器上被接受。共享访问令牌保存在操作系统凭据库中，而 `relay-settings.json` 只包含启用标志和公开地址。中继在线状态会被过滤为本地信任库中已经存在的设备。即使服务器发送伪造的设备元数据，后续的 Noise 握手仍然必须匹配已固定的对端公钥，才会接受应用数据。

Noise 静态私钥以应用服务名保存在 macOS Keychain、Windows 凭据管理器、Android Keystore 支持的存储或 iOS Keychain 中。`trusted-devices.json` 只包含对端公钥、指纹、设备元数据和时间戳。

桌面构建在公开应用标识变化后会保留原有的凭据库服务名。在 `com.kure29.neloa` 下首次启动时，Neloa 会从旧的 `app.neloa.desktop` 数据目录复制缺失的 `device-id`、信任库和剪贴板设置，且不覆盖新版本已经创建的任何数据。移动操作系统把新的 bundle/application ID 视为独立的沙盒，因此移动端升级仍需重新安装并重新配对。

首次配对时，两台设备比较由同一份 Noise 握手记录推导出的六位数字。只有在双方都显式接受之后才提交信任。之后的会话必须同时匹配发现的设备 ID 和已保存的 Noise 公钥。

每次 Noise 握手都携带发送方当前的与最低支持的协议版本，以及它的功能能力。版本范围必须有交集，畸形范围按失败处理，会话目的所需的能力必须存在。省略这些字段的对端会被当作协议 `0` 的遗留实现，并收到一条可操作的兼容性错误，而不是进入配对或传输流程。

在发送文件数据之前，发送方会对源文件计算哈希，并提供脱敏后的基本名称、字节大小和 SHA-256 摘要。接收方只写入该传输专用的临时文件，拒绝非法偏移或多余字节，校验最终摘要，刷写到磁盘，并在不替换已有文件的前提下发布。取消和失败只会删除该传输自己的临时文件。

剪贴板同步是失败关闭的，并且默认禁用。启用时会记录当前剪贴板作为本地基线而不传输它。之后的文本更新会获得 UUID，经过大小和强凭据标记检查，并且只发送给当前已发现的已信任设备。远端更新只有在操作系统剪贴板写入成功之后才会被确认。收到的内容会成为新的本地基线，防止它被发回去；CRLF、CR 和 LF 会被归一化，以便在 Windows 与 macOS 之间比较。事件只保留对端、方向、字节数、状态和时间——不保留剪贴板文本。

Android 需要 `CHANGE_WIFI_MULTICAST_STATE` 权限，并在 Activity 存活期间持有一个 `WifiManager.MulticastLock`，以便可靠地收到 mDNS 数据包。iOS 不打开原始多播套接字：`NWBrowser` 浏览 `_neloa._udp`，而 `NetService` 发布并解析指向既有 Rust QUIC 监听器（UDP 48631）的 Bonjour 服务。Swift 会把解析出的 IPv4 地址和 TXT 元数据转发给共享的 Rust 对端存储。这条路径使用已声明的 Bonjour 服务和本地网络隐私提示，不需要受限的多播 entitlement。

## 规划中的边界

```text
React UI
  -> typed Tauri commands/events
Application services
  -> discovery | pairing | transfer | clipboard | history
Platform adapters
  -> mDNS/Bonjour | QUIC | filesystem | system clipboard | secure key store
```

中继服务位于 `relay/`，共享的线协议定义位于 `crates/neloa-relay-protocol`。它用一个服务器令牌为小型单用户部署做鉴权，把在线状态和隧道状态保存在内存中，并转发有界的二进制帧而不检查其中经过 Noise 加密的内容。客户端维护一条 WebSocket 连接，把中继隧道映射为有界的本地字节流，并复用既有的配对、传输、测试消息和剪贴板 Noise 协议，不引入与传输相关的额外信封。中继配对是被有意禁用的：设备必须先在本地建立信任，才有资格出现在中继在线列表或使用中继隧道。

## 后续里程碑

1. 中继验收：部署在真实的 TLS 反向代理之后，在两台物理设备上验证重连与跨网络的文件/剪贴板传输，并增加不记录负载的运维指标。
2. 移动端验收：完成 Android 真机验证，并把 iOS 覆盖扩展到网络切换、长传输、生命周期和前台剪贴板行为。
3. 加固：配对流控、诊断错误分类、迁移回归覆盖，以及平台防火墙与生命周期处理。
4. 打包：已签名的 `.dmg`/`.app`、Windows MSIX 或 NSIS、Android 发布签名，以及 iOS/TestFlight 分发。
