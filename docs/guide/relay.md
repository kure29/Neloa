---
title: 自建中继
---

# 自建中继

中继让不在同一网络的设备能够互相发现、配对和传输。它只转发经过 Noise 加密的字节，配对是否成立仍由两端用户核对六位验证码后决定。

中继位于仓库的 `relay/` 目录，面向单用户部署，不需要数据库。它只转发无法解读的加密数据：能看到设备元数据、流量的时间与大小，拿不到 Noise 明文。

## 本机试跑

```bash
export NELOA_RELAY_TOKEN="$(openssl rand -hex 32)"
cargo run --manifest-path relay/Cargo.toml
```

默认监听 `0.0.0.0:8787`。可以用 `NELOA_RELAY_BIND` 改监听地址，用 `NELOA_RELAY_MAX_DEVICES`（1–4096，默认 64）限制在线设备数。

健康检查端点不需要鉴权：

```bash
curl http://127.0.0.1:8787/healthz
```

## 用 Docker Compose 部署

```bash
cp relay/.env.example relay/.env
# 把 relay/.env 里的 NELOA_RELAY_TOKEN 换成：openssl rand -hex 32
docker compose --env-file relay/.env -f relay/compose.yaml up -d --build
```

Compose 只把服务发布到 `127.0.0.1`。你需要在同一台主机上用 Caddy、Nginx 或 1Panel 之类的反向代理终止 HTTPS，把公网 `wss://` 端点转发到 `http://127.0.0.1:8787`。

::: danger 不要把明文 WebSocket 端口暴露到公网
中继自身不提供 TLS。公网必须先有 HTTPS 终止层，客户端填写的地址也必须是 `wss://`。`ws://` 只在回环地址上被接受，用于本机开发。
:::

## 在客户端接入

1. 部署中继并准备好同一个至少 32 字符的令牌。
2. 在每台设备上打开「设置 → 连接 → 自建中继」。
3. 填入公网的 `wss://.../v1/ws` 地址和同一个令牌，打开开关后保存。
4. 设备出现后可直接选择「中继」发起首次配对，也可以在配对后随时切换连接方式。

地址和启用状态保存在应用数据目录里，令牌单独保存在系统凭据库中。令牌输入框留空表示不更换已保存的值。

设备页可以为每台设备选择自动、局域网或中继。关闭中继不会影响局域网传输。

## 部署时值得知道的限制

- 状态只在内存里，重启只会断开当前会话，不影响下次连接。
- 一个部署是一个进程，没有多实例路由。
- 共享令牌是给单个所有者的，不适合互不相识的多个用户。
- TLS、公网边缘的限流、监控和令牌备份都属于部署方的责任。
- 共享令牌只划分同一部署内的设备范围，不会自动建立设备信任；首次配对仍需双方核对验证码。

线协议（`/v1/ws` 的鉴权头、register 控制消息、隧道帧格式）与完整限制列表见[中继服务与线协议](/reference/relay)，其原文是 `relay/README.md`。
