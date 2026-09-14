# syntax=docker/dockerfile:1.7

FROM rust:1.98-slim-bookworm AS builder
WORKDIR /source

COPY crates/neloa-relay-protocol ./crates/neloa-relay-protocol
COPY relay ./relay
RUN cargo build --locked --release --manifest-path relay/Cargo.toml

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install --yes --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system --gid 10001 neloa \
    && useradd --system --uid 10001 --gid neloa --no-create-home neloa

COPY --from=builder /source/relay/target/release/neloa-relay /usr/local/bin/neloa-relay

USER neloa
ENV NELOA_RELAY_BIND=0.0.0.0:8787 \
    NELOA_RELAY_MAX_DEVICES=64 \
    RUST_LOG=neloa_relay=info
EXPOSE 8787
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD ["curl", "--fail", "--silent", "http://127.0.0.1:8787/healthz"]
ENTRYPOINT ["neloa-relay"]
