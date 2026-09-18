---
title: Cloudflare Workers relay
---

# Cloudflare Workers relay

Neloa includes a Cloudflare Workers implementation compatible with relay protocol v1. It uses a Durable Object to coordinate long-lived connections and hibernatable WebSockets to retain device and tunnel routing state. Existing clients only need the deployed `wss://.../v1/ws` address; the wire protocol is unchanged.

Noise XX still encrypts file and clipboard content end to end on the clients. The Worker stores no files, clipboard content, accounts, or offline messages. It can observe online-device metadata, traffic timing, and encrypted payload sizes.

## Prerequisites

You need a Cloudflare account and Node.js 20+ locally. Install the isolated deployment dependencies after cloning the repository:

```bash
cd relay-worker
npm install
```

Generate a random token of at least 32 characters and save it as a Wrangler Secret. Never place the token in `wrangler.jsonc` or commit it to Git:

```bash
openssl rand -hex 32
npx wrangler secret put NELOA_RELAY_TOKEN
```

Paste the generated value when prompted, then deploy:

```bash
npm run deploy
```

The first deployment creates a SQLite-backed Durable Object. No transfer content is stored in a database; SQLite-backed is simply this Durable Object's deployment type.

## Connecting clients

Wrangler prints the Worker URL after deployment, for example:

```text
https://neloa-relay.<your-subdomain>.workers.dev
```

On every Neloa device, open Settings → Connection → Self-hosted relay and enter:

```text
wss://neloa-relay.<your-subdomain>.workers.dev/v1/ws
```

Use the same token saved during deployment. Once the status is Connected, select **Relay** on the device screen to use this route strictly for pairing or transfer. A failed connection does not silently fall back to the LAN.

The health endpoint needs no token:

```bash
curl https://neloa-relay.<your-subdomain>.workers.dev/healthz
```

A correctly configured deployment returns:

```json
{"status":"ok","relayProtocolVersion":1}
```

The endpoint returns HTTP 503 when the Secret is absent or invalid.

## Configuration and verification

`NELOA_RELAY_MAX_DEVICES` in `relay-worker/wrangler.jsonc` defaults to `64` and accepts 1–4096. One deployment and shared token are for one owner. Deploy separate Workers and tokens to isolate unrelated users.

Verify before deployment:

```bash
npm test
npm run check
npm run dev
```

The outer Worker validates the bearer token before invoking the Durable Object. Connected-device metadata and active tunnels live in each WebSocket attachment, so healthy connections survive object hibernation and restoration. Disconnecting removes associated tunnels and notifies the peer.

## Compared with the server build

| Item | Cloudflare Workers | Rust + Docker |
| --- | --- | --- |
| TLS | Provided by Cloudflare | Requires a reverse proxy such as Caddy or Nginx |
| Runtime state | Durable Object and WebSocket attachments | Single-process memory |
| File content | Never stored; ciphertext is forwarded | Never stored; ciphertext is forwarded |
| Deployment source | `relay-worker/` | `relay/` |
| Best fit | Personal deployment without server maintenance | Operator-controlled host, network, and monitoring |

Both implementations use the same protocol. Do not connect one set of devices to two deployments at the same time; each deployment is a separate presence domain.
