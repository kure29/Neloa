# Neloa self-hosted relay

This directory contains Neloa's online-only WebSocket relay for a small,
single-user deployment:

- one shared bearer token controls access;
- connected device metadata and tunnel routes live only in memory;
- binary tunnel bodies are forwarded as opaque bytes;
- there is no offline queue, content storage, account system, or database;
- bounded queues close an overloaded tunnel instead of growing without limit.

Current clients keep a persistent authenticated connection to this service.
They try LAN QUIC first and use the relay only when the direct route fails or is
unavailable. Relay discovery and tunnels are limited to devices that were
already paired locally, and every relayed session still performs the existing
Noise XX identity check and end-to-end encryption.

## Run locally

```bash
export NELOA_RELAY_TOKEN="$(openssl rand -hex 32)"
cargo run --manifest-path relay/Cargo.toml
```

The default listener is `0.0.0.0:8787`. Override it with
`NELOA_RELAY_BIND`, and set `NELOA_RELAY_MAX_DEVICES` between 1 and 4096 if
the default limit of 64 is not appropriate.

Check the unauthenticated health endpoint:

```bash
curl http://127.0.0.1:8787/healthz
```

## Run with Docker Compose

```bash
cp relay/.env.example relay/.env
# Replace NELOA_RELAY_TOKEN in relay/.env with: openssl rand -hex 32
docker compose --env-file relay/.env -f relay/compose.yaml up -d --build
```

The Compose service publishes only to `127.0.0.1`. Put Caddy, Nginx, or
another reverse proxy on the same host, terminate HTTPS there, and proxy the
public `wss://` endpoint to `http://127.0.0.1:8787`. Do not publish the plain
WebSocket listener directly to the internet.

## Connect Neloa clients

1. Pair the devices once while they can reach each other over the same LAN.
2. Deploy the relay behind HTTPS and keep the generated
   `NELOA_RELAY_TOKEN` available on each device.
3. On every client, open **设置 → 连接 → 自建中继**.
4. Enter the public `wss://.../v1/ws` URL and the same token, enable the
   switch, then save.

The URL and enable flag are stored in app data. The token is stored separately
in Keychain, Credential Manager, Android Keystore-backed storage, or iOS
Keychain. A blank token field preserves the previously saved value. Public
relay URLs must use `wss://`; `ws://` is accepted only for loopback testing.

When an already paired device is reachable only through the relay, its radar
status reads **已配对 · 中继**. Closing the relay does not disable LAN transfer.

## Wire protocol v1

Clients connect to `/v1/ws` with this header:

```text
Authorization: Bearer <NELOA_RELAY_TOKEN>
```

The first WebSocket message must be a JSON `register` control message. Further
JSON messages open and close tunnels or carry keepalive nonces. Binary messages
use a 16-byte tunnel UUID header followed by at most 256 KiB of opaque payload.
The shared Rust definitions and validation rules are in
`crates/neloa-relay-protocol`.

The relay can observe connection metadata, device metadata, traffic timing, and
payload sizes. It must never receive Noise plaintext. Pairing decisions remain
client-side, so a relay connection alone does not make another device trusted.

## Current limitations

- State is lost on restart, which only disconnects active sessions.
- A deployment is a single process; multi-instance routing is not implemented.
- The shared token is intended for one owner, not unrelated users.
- TLS, rate limiting at the public edge, monitoring, and backups of the token
  belong to the operator's reverse-proxy/deployment configuration.
- Relay pairing is intentionally unsupported; bootstrap trust over LAN first.
- Real-device validation across public networks is the next acceptance step.
