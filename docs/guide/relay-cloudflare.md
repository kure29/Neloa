---
title: Cloudflare Workers 中继
---

# Cloudflare Workers 中继

Neloa 提供一份与中继协议 v1 兼容的 Cloudflare Workers 实现。它使用 Durable Objects 协调长连接，并使用可休眠 WebSocket 保存在线设备和隧道路由。现有客户端无需更换协议，只需填写部署后的 `wss://.../v1/ws` 地址。

文件与剪贴板内容仍由客户端上的 Noise XX 端到端加密。Worker 不保存文件、剪贴板、账号或离线消息，但能看到在线设备元数据、流量时间和加密负载大小。

## 准备

需要一个 Cloudflare 账号和本机 Node.js 20+。克隆仓库后安装独立依赖：

```bash
cd relay-worker
npm install
```

生成至少 32 字符的随机令牌，并通过 Wrangler Secret 保存。不要把令牌写进 `wrangler.jsonc` 或提交到 Git：

```bash
openssl rand -hex 32
npx wrangler secret put NELOA_RELAY_TOKEN
```

按照提示粘贴刚生成的值，然后部署：

```bash
npm run deploy
```

首次部署会创建一个 SQLite-backed Durable Object。它不使用数据库保存传输内容；SQLite backend 只是这个 Durable Object 的部署类型。

## 接入客户端

Wrangler 完成后会输出 Worker 地址，例如：

```text
https://neloa-relay.<你的子域>.workers.dev
```

在每台 Neloa 设备打开「设置 → 连接 → 自建中继」，填写：

```text
wss://neloa-relay.<你的子域>.workers.dev/v1/ws
```

令牌填写部署时保存的同一个值。保存并确认状态变为“已连接”后，设备页选择「中继」即可严格使用这条路径完成配对或传输；连接失败时不会自动改走局域网。

健康检查无需令牌：

```bash
curl https://neloa-relay.<你的子域>.workers.dev/healthz
```

正确配置时返回：

```json
{"status":"ok","relayProtocolVersion":1}
```

若 Secret 未设置或格式不正确，端点返回 HTTP 503。

## 配置与验证

`relay-worker/wrangler.jsonc` 中的 `NELOA_RELAY_MAX_DEVICES` 默认是 `64`，可设置为 1–4096。单个部署和令牌只面向一个所有者；需要隔离不同用户时，应分别部署 Worker 和令牌。

提交部署前可以本地验证：

```bash
npm test
npm run check
npm run dev
```

Worker 在进入 Durable Object 前校验 Bearer Token。在线设备信息和活动隧道保存在每条 WebSocket 的 attachment 中，因此对象休眠和唤醒不会丢失仍然健康的连接状态。断线会删除相关隧道并通知另一端。

## 与服务器版本的区别

| 项目 | Cloudflare Workers | Rust + Docker |
| --- | --- | --- |
| TLS | Cloudflare 自动提供 | 需要 Caddy、Nginx 等反向代理 |
| 运行状态 | Durable Object 与 WebSocket attachment | 单进程内存 |
| 文件内容 | 不保存，只转发密文 | 不保存，只转发密文 |
| 部署入口 | `relay-worker/` | `relay/` |
| 适用场景 | 不想维护服务器的个人部署 | 自己掌控主机、网络和监控 |

两种实现使用同一套协议。不要让同一组设备同时连接两个部署；它们是两个独立的在线设备范围。
