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

# WebPi server-only image includes its own CLI for pairing/admin.
# The local WebPi Runner and Pi bridge remain a separate deployment. Git metadata is
# supplied as build args because .git is intentionally outside the build context.
ARG WEBPI_GIT_COMMIT
ARG WEBPI_GIT_DIRTY
ARG WEBPI_BUILT_AT
RUN WEBPI_GIT_COMMIT="$WEBPI_GIT_COMMIT" \
    WEBPI_GIT_DIRTY="$WEBPI_GIT_DIRTY" \
    WEBPI_BUILT_AT="$WEBPI_BUILT_AT" \
    cargo build --locked --release --bins -p webcodex -p webcodex-cli

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        curl \
        libgcc-s1 \
        libstdc++6 \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system --gid 10001 webpi \
    && useradd --system --uid 10001 --gid webpi \
        --home-dir /var/lib/webpi webpi \
    && install -d -o webpi -g webpi -m 0700 /var/lib/webpi

COPY --from=builder /src/target/release/webpi-server /usr/local/bin/webpi-server
COPY --from=builder /src/target/release/webpi /usr/local/bin/webpi

ENV WEBPI_ADDR=0.0.0.0:8080 \
    WEBPI_DATA=/var/lib/webpi \
    RUST_LOG=info

USER webpi:webpi
WORKDIR /var/lib/webpi

EXPOSE 8080
VOLUME ["/var/lib/webpi"]

HEALTHCHECK --interval=15s --timeout=5s --start-period=10s --retries=5 \
    CMD curl -fsS http://127.0.0.1:8080/openapi.json >/dev/null || exit 1

ENTRYPOINT ["/usr/local/bin/webpi-server"]
