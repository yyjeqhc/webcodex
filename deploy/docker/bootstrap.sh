#!/bin/sh
set -eu

usage() {
    echo "Usage: $0 <public-https-url>" >&2
    echo "Example: $0 https://webcodex.example.com" >&2
    exit 2
}

[ "$#" -eq 1 ] || usage
PUBLIC_URL=${1%/}

case "$PUBLIC_URL" in
    https://*) ;;
    *)
        echo "public URL must start with https://" >&2
        exit 2
        ;;
esac

ORIGIN=${PUBLIC_URL#https://}
case "$ORIGIN" in
    ""|*/*)
        echo "public URL must be an HTTPS origin without a path" >&2
        exit 2
        ;;
esac

if [ -e .env ]; then
    echo ".env already exists; refusing to overwrite it" >&2
    exit 1
fi

if ! command -v docker >/dev/null 2>&1; then
    echo "docker is required" >&2
    exit 1
fi

if ! docker compose version >/dev/null 2>&1; then
    echo "docker compose v2 is required" >&2
    exit 1
fi

if command -v openssl >/dev/null 2>&1; then
    TOKEN=$(openssl rand -hex 32)
else
    TOKEN=$(od -An -N32 -tx1 /dev/urandom | tr -d ' \n')
fi

umask 077
cat > .env <<EOF_ENV
WEBCODEX_PUBLIC_URL=$PUBLIC_URL
WEBCODEX_TOKEN=$TOKEN
WEBCODEX_HOST_IP=127.0.0.1
WEBCODEX_HOST_PORT=8080
RUST_LOG=info
WEBCODEX_MCP_MODEL_SURFACE=local-coding-v1
EOF_ENV
chmod 600 .env

docker compose up -d --build

cat <<EOF_DONE

WebCodex server container started.

Reverse-proxy upstream: http://127.0.0.1:8080
Public URL:            $PUBLIC_URL
Console:               $PUBLIC_URL/console
OpenAPI:               $PUBLIC_URL/openapi.json
MCP:                   $PUBLIC_URL/mcp

This Compose stack runs webcodex-server only. It does not run webcodex-runner
and does not mount any source repository.

After the reverse proxy is ready, create a short-lived pairing code with:
  docker compose exec webcodex sh -lc 'webcodex pairing create --server-url "\$WEBCODEX_PUBLIC_URL" --username admin --ttl-secs 600'

Keep .env private. It contains the bootstrap administrator token.
EOF_DONE
