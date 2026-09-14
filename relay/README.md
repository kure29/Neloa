# Neloa self-hosted relay

This directory contains the first relay foundation for Neloa. It is an
online-only WebSocket relay for a small, single-user deployment:

- one shared bearer token controls access;
- connected device metadata and tunnel routes live only in memory;
- binary tunnel bodies are forwarded as opaque bytes;
- there is no offline queue, content storage, account system, or database;
- bounded queues close an overloaded tunnel instead of growing without limit.

The app does not connect to this service yet. The next milestone is a client
transport abstraction that keeps the existing Noise XX session and selects LAN
QUIC first, then the configured relay when direct discovery is unavailable.

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
