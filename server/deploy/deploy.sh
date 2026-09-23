#!/usr/bin/env bash
# Deploy Todora's leaderboard server.
#
#   server/deploy/deploy.sh root@your-hetzner-host          # over SSH
#   server/deploy/deploy.sh local                           # this machine's Docker, for trying it
#
# Builds the image here for the target's architecture, copies it over SSH with
# `docker save | docker load`, and (re)starts one container listening on
# 127.0.0.1:8787 only, with its data in the `todora-data` volume and restarts
# unless stopped. TLS and the public name belong to the reverse proxy already
# on the host; see Caddyfile.example and nginx.example beside this script.
#
# The admin token is made once on the target, kept in /etc/todora/admin-token
# (or ~/.todora-admin-token locally), and never leaves it.
#
# ARCH=arm64 for Hetzner's Ampere (CAX) machines; the default is amd64.
set -euo pipefail

TARGET="${1:?usage: deploy.sh <user@host | local>}"
ARCH="${ARCH:-amd64}"
IMAGE="todora-server:$(git rev-parse --short HEAD)"
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"

echo "building $IMAGE for linux/$ARCH"
docker build --platform "linux/$ARCH" -f "$ROOT/server/Dockerfile" -t "$IMAGE" "$ROOT"

# What runs on the target: keep a token, replace the container, check health.
read -r -d '' REMOTE <<SCRIPT || true
set -euo pipefail
token_file=\${TODORA_TOKEN_FILE:-/etc/todora/admin-token}
if [ ! -s "\$token_file" ]; then
  mkdir -p "\$(dirname "\$token_file")"
  head -c 32 /dev/urandom | od -An -tx1 | tr -d ' \n' > "\$token_file"
  chmod 600 "\$token_file"
fi
docker rm -f todora-server >/dev/null 2>&1 || true
docker run -d --name todora-server --restart unless-stopped \
  -p 127.0.0.1:\${TODORA_PORT:-8787}:8787 \
  -v todora-data:/data \
  -e TODORA_ADMIN_TOKEN="\$(cat "\$token_file")" \
  --memory 512m --cpus 1.5 \
  $IMAGE >/dev/null
for i in \$(seq 1 30); do
  if curl -sf http://127.0.0.1:\${TODORA_PORT:-8787}/v1/health; then echo; exit 0; fi
  sleep 1
done
echo "the server did not come up" >&2
docker logs --tail 50 todora-server >&2
exit 1
SCRIPT

if [ "$TARGET" = local ]; then
  TODORA_TOKEN_FILE="$HOME/.todora-admin-token" TODORA_PORT="${TODORA_PORT:-18787}" bash -c "$REMOTE"
else
  echo "copying $IMAGE to $TARGET"
  docker save "$IMAGE" | gzip | ssh "$TARGET" 'gunzip | docker load'
  ssh "$TARGET" "bash -s" <<<"$REMOTE"
fi
echo "deployed $IMAGE to $TARGET"
