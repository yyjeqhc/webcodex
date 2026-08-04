# syntax=docker/dockerfile:1

FROM rust:bookworm AS builder

WORKDIR /src

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential \
        cmake \
        perl \
        pkg-config \
    && rm -rf /var/lib/apt/lists/*

COPY . .

# The server image also contains the webcodex CLI so server-side pairing and
# administration can be run with `docker compose exec webcodex webcodex ...`.
# webcodex-runner is intentionally not built into this image.
RUN cargo build --locked --release -p webcodex --bin webcodex-server \
    && cargo build --locked --release -p webcodex-cli --bin webcodex

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        curl \
        libgcc-s1 \
        libstdc++6 \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system --gid 10001 webcodex \
    && useradd --system --uid 10001 --gid webcodex \
        --home-dir /var/lib/webcodex webcodex \
    && install -d -o webcodex -g webcodex -m 0700 /var/lib/webcodex

COPY --from=builder /src/target/release/webcodex-server /usr/local/bin/webcodex-server
COPY --from=builder /src/target/release/webcodex /usr/local/bin/webcodex

ENV WEBCODEX_ADDR=0.0.0.0:8080 \
    WEBCODEX_DATA=/var/lib/webcodex \
    RUST_LOG=info

USER webcodex:webcodex
WORKDIR /var/lib/webcodex

EXPOSE 8080
VOLUME ["/var/lib/webcodex"]

HEALTHCHECK --interval=15s --timeout=5s --start-period=10s --retries=5 \
    CMD curl -fsS http://127.0.0.1:8080/openapi.json >/dev/null || exit 1

ENTRYPOINT ["/usr/local/bin/webcodex-server"]
