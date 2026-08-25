#!/bin/sh
set -eu

usage() {
    echo "Usage: $0 <public-https-url> [--build-from-source]" >&2
    echo "Example: $0 https://webcodex.example.com" >&2
    echo "Source build: $0 https://webcodex.example.com --build-from-source" >&2
    exit 2
}

[ "$#" -ge 1 ] && [ "$#" -le 2 ] || usage
PUBLIC_URL=${1%/}
BUILD_FROM_SOURCE=false
if [ "$#" -eq 2 ]; then
    [ "$2" = "--build-from-source" ] || usage
    BUILD_FROM_SOURCE=true
fi
COMPOSE_FILE=${COMPOSE_FILE:-compose.yaml}
SERVER_IMAGE=${WEBCODEX_SERVER_IMAGE:-}

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

case "$COMPOSE_FILE" in
    ""|/*|-*|../*|*/../*|*/..|*[!A-Za-z0-9._/-]*)
        echo "invalid COMPOSE_FILE: expected a safe relative path" >&2
        exit 2
        ;;
esac
if [ ! -f "$COMPOSE_FILE" ]; then
    echo "compose file not found: $COMPOSE_FILE" >&2
    exit 1
fi

compose() {
    docker compose -f "$COMPOSE_FILE" "$@"
}

if ! docker compose version >/dev/null 2>&1; then
    echo "docker compose v2 is required" >&2
    exit 1
fi

if [ "$BUILD_FROM_SOURCE" = false ]; then
    if [ -z "$SERVER_IMAGE" ]; then
        SERVER_IMAGE=$(WEBCODEX_TOKEN=0000000000000000000000000000000000000000000000000000000000000000 \
            WEBCODEX_PUBLIC_URL="$PUBLIC_URL" \
            compose config --images)
    fi
    case "$SERVER_IMAGE" in
        ""|*[!A-Za-z0-9._/:@-]*)
            echo "invalid WEBCODEX_SERVER_IMAGE: expected a Docker image reference" >&2
            exit 2
            ;;
    esac
    if ! WEBCODEX_TOKEN=0000000000000000000000000000000000000000000000000000000000000000 \
        WEBCODEX_PUBLIC_URL="$PUBLIC_URL" \
        WEBCODEX_SERVER_IMAGE="$SERVER_IMAGE" \
        compose pull webcodex; then
        echo "could not pull the published WebCodex Server image: $SERVER_IMAGE" >&2
        echo "If the official image is not published/public yet, retry with --build-from-source." >&2
        exit 1
    fi
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
COMPOSE_FILE=$COMPOSE_FILE
EOF_ENV
if [ "$BUILD_FROM_SOURCE" = false ]; then
    printf '%s\n' "WEBCODEX_SERVER_IMAGE=$SERVER_IMAGE" >> .env
fi
chmod 600 .env

if [ "$BUILD_FROM_SOURCE" = true ]; then
    WEBCODEX_GIT_COMMIT=
    WEBCODEX_GIT_DIRTY=
    if command -v git >/dev/null 2>&1 && git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
        WEBCODEX_GIT_COMMIT=$(git rev-parse --short=12 HEAD)
        if [ -n "$(git status --porcelain --untracked-files=normal)" ]; then
            WEBCODEX_GIT_DIRTY=true
        else
            WEBCODEX_GIT_DIRTY=false
        fi
    fi
    WEBCODEX_BUILT_AT=$(date +%s)
    export WEBCODEX_GIT_COMMIT WEBCODEX_GIT_DIRTY WEBCODEX_BUILT_AT
    if [ ! -f compose.build.yaml ]; then
        echo "compose.build.yaml is required for --build-from-source" >&2
        exit 1
    fi
    docker compose -f "$COMPOSE_FILE" -f compose.build.yaml up -d --build
    DEPLOYMENT_SOURCE="local source build"
else
    # The image was pulled before .env was created, so startup cannot strand a
    # fresh bootstrap merely because the registry becomes briefly unavailable.
    compose up -d --no-build --pull never
    DEPLOYMENT_SOURCE="$SERVER_IMAGE"
fi

cat <<EOF_DONE

WebCodex server container started.
Deployment source:       $DEPLOYMENT_SOURCE

Reverse-proxy upstream: http://127.0.0.1:8080
Public URL:            $PUBLIC_URL
Console:               $PUBLIC_URL/console
OpenAPI:               $PUBLIC_URL/openapi.json
MCP:                   $PUBLIC_URL/mcp

This Compose stack runs webcodex-server only. It does not run webcodex-runner
and does not mount any source repository.

After the reverse proxy is ready, create a short-lived pairing code with:
  docker compose -f "$COMPOSE_FILE" exec webcodex sh -lc 'webcodex pairing create --server-url "\$WEBCODEX_PUBLIC_URL" --username admin --ttl-secs 600'

Keep .env private. It contains the bootstrap administrator token.
EOF_DONE
