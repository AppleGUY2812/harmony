#!/usr/bin/env bash
# Demo the app to a friend. Usage: ./demo.sh [--internet | down | logs]
set -euo pipefail
cd "$(dirname "$0")"

die() { echo "error: $*" >&2; exit 1; }
# --env-file keeps compose from reading the real secrets in ./.env
compose() { docker compose -f docker-compose.demo.yml --env-file demo/.env "$@"; }

docker info >/dev/null 2>&1 || die "Docker isn't running. Start Docker Desktop and try again."

[ -f demo/.env ] || echo "JWT_SECRET=$(openssl rand -hex 32)" > demo/.env

case "${1:-}" in
  down) compose --profile internet down; exit ;;
  logs) compose --profile internet logs -f --tail 100; exit ;;
  "" | --internet) ;;
  *) die "usage: ./demo.sh [--internet | down | logs]" ;;
esac

lan_ip=$(ipconfig getifaddr en0 || ipconfig getifaddr en1 || true)
[ -n "$lan_ip" ] || die "no LAN IP on en0/en1. Are you on Wi-Fi?"

export DEMO_HOST="$lan_ip" WEBRTC_PUBLIC_IP="$lan_ip"
# a restarted old tunnel would still have the previous (dead) URL in its logs
compose --profile internet rm -sf tunnel >/dev/null 2>&1 || true
if [ "${1:-}" = --internet ]; then
  WEBRTC_PUBLIC_IP=$(curl -fsS --max-time 10 https://api.ipify.org) || die "couldn't get your public IP"
  compose --profile internet up -d --build
else
  compose up -d --build
fi

if [ "${1:-}" = --internet ]; then
  url=""
  for _ in $(seq 30); do
    url=$(compose --profile internet logs tunnel 2>/dev/null | grep -oE 'https://[a-z0-9-]+\.trycloudflare\.com' | tail -1 || true)
    [ -n "$url" ] && break
    sleep 1
  done
  [ -n "$url" ] || die "tunnel URL not found yet, check ./demo.sh logs"
  cat <<EOF

================================================================
  Harmony demo is up (internet mode)

  Send your friend:   $url

  Voice/video calls need ONE router step:
    forward UDP ports 50000-50199 -> $lan_ip (this Mac)
  No TURN server is included, so calls can fail on strict networks.
  Chat works without the router step.

  Same Wi-Fi: https://$lan_ip gives chat only in this mode.
    For calls on the same Wi-Fi, run ./demo.sh (no --internet).

  Stop:  ./demo.sh down      Logs:  ./demo.sh logs
================================================================
EOF
else
  cat <<EOF

================================================================
  Harmony demo is up (same Wi-Fi mode)

  Open on any device on this Wi-Fi:   https://$lan_ip

  First visit: click Advanced -> Proceed (self-signed cert).
  Camera/mic only work on this https:// URL.

  Friend not on your Wi-Fi?  ./demo.sh --internet
  Stop:  ./demo.sh down      Logs:  ./demo.sh logs
================================================================
EOF
fi
