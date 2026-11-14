#!/usr/bin/env bash
# Captures the web screenshots: a fresh nsh-mail-server serving <site> (the
# `nsh package --target web` output), GreenMail seeded by seed.py, shoot.mjs.
# Usage: tools/screenshots/run.sh <site> <out dir>
# Needs: GreenMail with auth disabled (ports from GREENMAIL_*_PORT, default
# 3025/3143/3993/3465); target/debug/nsh-mail-server built with
# `--features insecure-test-tls` (or SERVER_BIN); `npm ci` run here.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
site=$(cd "$1" && pwd)
mkdir -p "$2" && out=$(cd "$2" && pwd)
server=${SERVER_BIN:-$here/../../target/debug/nsh-mail-server}
port=${SERVER_PORT:-18090}
data=$(mktemp -d)
trap 'kill $(jobs -p) 2>/dev/null || true; rm -rf "$data"' EXIT

[ -f "$site/index.html" ] || { echo "no index.html in $site: pass the web site directory" >&2; exit 1; }
[ -x "$server" ] || { echo "no server at $server: cargo build -p nsh-mail-server --features insecure-test-tls" >&2; exit 1; }
for p in "${GREENMAIL_SMTP_PORT:-3025}" "${GREENMAIL_IMAPS_PORT:-3993}"; do
  for _ in $(seq 60); do (echo > "/dev/tcp/127.0.0.1/$p") 2>/dev/null && break; sleep 1; done
  (echo > "/dev/tcp/127.0.0.1/$p") 2>/dev/null || { echo "GreenMail is not listening on port $p" >&2; exit 1; }
done

# The failure shots kill the server at the end of the run: shoot.mjs gets its pid.
NSHMAIL_MASTER_KEY=$(head -c 32 /dev/urandom | base64) \
  NSHMAIL_DATA_DIR="$data" \
  NSHMAIL_BIND="127.0.0.1:$port" \
  NSHMAIL_PUBLIC_URL="http://127.0.0.1:$port" \
  NSHMAIL_WEB_DIR="$site" \
  "$server" > "$out/server.log" 2>&1 &
for _ in $(seq 60); do (echo > "/dev/tcp/127.0.0.1/$port") 2>/dev/null && break; sleep 1; done

server_pid=$!
python3 "$here/seed.py"
SERVER_PID=$server_pid node "$here/shoot.mjs" "http://127.0.0.1:$port" "$out"
