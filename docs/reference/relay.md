---
title: 中继服务与线协议
---

# Neloa 自建中继

本页是 `relay/README.md` 的中文版。**英文原文是唯一真源**，两者出现分歧时以原文为准；切换到 English 可以看到原文。面向部署的操作步骤另见[自建中继](/guide/relay)。

这个目录包含 Neloa 仅在线使用的 WebSocket 中继，面向小型单用户部署：

- 用一个共享 bearer 令牌控制访问；
- 已连接设备元数据与隧道路由只存在于内存中；
- 二进制隧道正文作为不透明字节转发；
- 没有离线队列、内容存储、账号系统或数据库；
- 有界队列会关闭过载的隧道，而不是无限制增长。

当前客户端与这个服务保持一条经过认证的持久连接。每台设备可以选择自动、局域网或中继；自动模式在存在局域网地址时使用 QUIC，否则使用中继。中继会公布同一令牌下的在线设备以支持首次配对，每条经由中继的会话仍执行 Noise XX 身份校验和端到端加密。

## 在本机运行

```bash
export NELOA_RELAY_TOKEN="$(openssl rand -hex 32)"
cargo run --manifest-path relay/Cargo.toml
```

默认监听 `0.0.0.0:8787`。可以用 `NELOA_RELAY_BIND` 覆盖它；如果默认上限 64 不合适，可以用 `NELOA_RELAY_MAX_DEVICES` 在 1 到 4096 之间设置。

检查不需要鉴权的健康端点：

```bash
curl http://127.0.0.1:8787/healthz
```

## 使用 Docker Compose 运行

```bash
cp relay/.env.example relay/.env
# 将 relay/.env 中的 NELOA_RELAY_TOKEN 替换为：openssl rand -hex 32
docker compose --env-file relay/.env -f relay/compose.yaml up -d --build
```

Compose 服务只发布到 `127.0.0.1`。请在同一台主机上放置 Caddy、Nginx 或其他反向代理，在那里终止 HTTPS，并把公网 `wss://` 端点代理到 `http://127.0.0.1:8787`。不要直接把明文 WebSocket 监听器发布到互联网。

## 连接 Neloa 客户端

1. 把中继部署在 HTTPS 之后，并在每台设备上准备好生成的 `NELOA_RELAY_TOKEN`。
2. 在每台客户端上打开 **设置 → 连接 → 自建中继**。
3. 填入公网 `wss://.../v1/ws` 地址和同一个令牌，打开开关，然后保存。
4. 设备出现后可选择 **中继** 直接配对，或使用 **自动** 让客户端按可用地址选择路径。

URL 和启用标志保存在应用数据中。令牌单独保存在 Keychain、凭据管理器、Android Keystore 支持的存储或 iOS Keychain 中。令牌字段留空会保留此前保存的值。公网中继地址必须使用 `wss://`；`ws://` 只在回环测试时被接受。

设备页可以为每台设备选择自动、局域网或中继。关闭中继不会禁用局域网传输。

## 线协议 v1

客户端用这个请求头连接 `/v1/ws`：

```text
Authorization: Bearer <NELOA_RELAY_TOKEN>
```

第一条 WebSocket 消息必须是一条 JSON `register` 控制消息。后续 JSON 消息用于打开和关闭隧道，或携带 keepalive nonce。二进制消息使用 16 字节的隧道 UUID 头，后接最多 256 KiB 的不透明负载。共享的 Rust 定义与校验规则位于 `crates/neloa-relay-protocol`。

中继可以观察到连接元数据、设备元数据、流量时间和负载大小。它绝不能收到 Noise 明文。配对决策始终在客户端进行，因此仅仅建立中继连接并不会让另一台设备变得可信。

## 当前的限制

- 状态在重启后丢失，只会断开当前会话。
- 一个部署是单个进程；没有实现多实例路由。
- 共享令牌面向单一所有者，不适合互不相关的用户。
- TLS、公网边缘的限流、监控，以及令牌备份，都属于运维方反向代理/部署配置的责任。
- 共享令牌不是设备信任凭据；首次配对仍必须在两端比较并确认六位验证码。
