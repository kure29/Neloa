---
title: Self-hosted relay
---

# Self-hosted relay

The relay solves one problem: how two already paired devices reach each other when they are not on the same network. It does **not** solve pairing — cross-network use starts with a pairing over the local network.

The relay lives in the `relay/` directory of the repository, targets a small single-user deployment, and needs no database. It only forwards encrypted data it cannot interpret: it can see device metadata and the timing and size of traffic, never Noise plaintext.

## Trying it locally

```bash
export NELOA_RELAY_TOKEN="$(openssl rand -hex 32)"
cargo run --manifest-path relay/Cargo.toml
```

The default listener is `0.0.0.0:8787`. Use `NELOA_RELAY_BIND` to change the address, and `NELOA_RELAY_MAX_DEVICES` (1–4096, default 64) to cap online devices.

The health endpoint requires no authentication:

```bash
curl http://127.0.0.1:8787/healthz
```

## Deploying with Docker Compose

```bash
cp relay/.env.example relay/.env
# Replace NELOA_RELAY_TOKEN in relay/.env with: openssl rand -hex 32
docker compose --env-file relay/.env -f relay/compose.yaml up -d --build
```

Compose publishes the service to `127.0.0.1` only. On the same host, put Caddy, Nginx, or 1Panel in front of it, terminate HTTPS there, and proxy the public `wss://` endpoint to `http://127.0.0.1:8787`.

::: danger Never expose the plaintext WebSocket port to the internet
The relay does not provide TLS itself. A public deployment needs an HTTPS termination layer, and the address entered in the client must be `wss://`. `ws://` is accepted only on loopback, for local development.
:::

## Connecting clients

1. Pair the two devices over the local network first.
2. Deploy the relay and have the same token of at least 32 characters ready.
3. On every device, open Settings → Connection → Self-hosted relay.
4. Enter the public `wss://.../v1/ws` URL and the same token, switch it on, and save.

The URL and enable flag are stored in the application data directory; the token is stored separately in the system credential store. Leaving the token field blank keeps the saved value.

When a paired device is only reachable through the relay, its radar status reads **paired · relay**. Turning the relay off does not affect local transfers.

## Limits worth knowing before you deploy

- State lives in memory; a restart only disconnects active sessions.
- One deployment is one process, with no multi-instance routing.
- The shared token is meant for a single owner, not for unrelated users.
- TLS, rate limiting at the public edge, monitoring, and token backups belong to whoever operates the deployment.
- Relay pairing is unsupported by design.

Authentication headers, the `register` control message, tunnel frame formats, and the full limitations list are in [Relay service and wire protocol](/en/reference/relay), taken from `relay/README.md`.
