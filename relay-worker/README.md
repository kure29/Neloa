# Neloa relay for Cloudflare Workers

This is a wire-compatible Cloudflare Workers deployment of Neloa relay protocol
v1. It uses a single SQLite-backed Durable Object and hibernatable WebSockets;
file and clipboard payloads remain opaque Noise ciphertext.

## Deploy

```bash
cd relay-worker
npm install
npx wrangler secret put NELOA_RELAY_TOKEN
npm run deploy
```

`NELOA_RELAY_TOKEN` must contain 32–512 non-whitespace characters. Generate one
with `openssl rand -hex 32`. The optional `NELOA_RELAY_MAX_DEVICES` variable in
`wrangler.jsonc` accepts 1–4096 and defaults to 64.

After deployment, configure each client with:

```text
wss://<worker-host>/v1/ws
```

The unauthenticated health endpoint is `https://<worker-host>/healthz`. It
returns HTTP 503 until the secret has been configured.

## Develop and verify

```bash
npm test
npm run check
npm run dev
```

With `wrangler dev` running, a two-client WebSocket smoke test is also
available:

```bash
npm run test:integration
```

The Worker validates the bearer token before invoking the Durable Object. Per
connection metadata and active tunnel routes are stored in WebSocket
attachments so they survive hibernation. No file body, clipboard body, account,
or offline queue is stored.

This deployment is for one owner and one shared token, matching the Rust relay
in `relay/`. Run separate Workers when unrelated owners need separate trust and
presence domains.
