#!/usr/bin/env bash
# Runs the cloud-mode end-to-end test (.e2e/cloud.test.tsx): nsh-mail-server,
# built with `insecure-test-tls` to accept GreenMail's certificate, in a
# fresh data directory, then `nsh test .e2e/cloud.test.tsx`. GreenMail must listen on 3993 /
# 3465, or GREENMAIL_JAR names greenmail-standalone.jar to start it with java.
set -euo pipefail
cd "$(dirname "$0")/.."
root=$(cd .. && pwd)
port=18080 # where cloud.test.tsx looks for the server
data=$(mktemp -d)
pids=()
cleanup() {
  [ ${#pids[@]} -eq 0 ] || kill "${pids[@]}" 2>/dev/null || true
  rm -rf "$data"
}
trap cleanup EXIT
listening() { (echo > "/dev/tcp/127.0.0.1/$1") 2>/dev/null; }
wait_for() {
  for _ in $(seq 90); do listening "$1" && return; sleep 1; done
  echo "nothing listens on port $1: $2" >&2
  exit 1
}

if ! listening 3993; then
  : "${GREENMAIL_JAR:?no GreenMail on port 3993: start one, or set GREENMAIL_JAR to greenmail-standalone.jar}"
  java -Dgreenmail.setup.test.all -Dgreenmail.auth.disabled -jar "$GREENMAIL_JAR" > "$data/greenmail.log" 2>&1 &
  pids+=($!)
fi
cargo build --manifest-path "$root/Cargo.toml" -p nsh-mail-server --features insecure-test-tls
NSHMAIL_MASTER_KEY=$(head -c 32 /dev/urandom | base64) \
  NSHMAIL_DATA_DIR="$data" \
  NSHMAIL_BIND="127.0.0.1:$port" \
  NSHMAIL_PUBLIC_URL="http://127.0.0.1:$port" \
  "$root/target/debug/nsh-mail-server" > "$data/server.log" 2>&1 &
pids+=($!)
wait_for 3993 "GreenMail"
wait_for "$port" "nsh-mail-server (log: $data/server.log)"

nsh test .e2e/cloud.test.tsx "$@" || {
  echo "--- server log" >&2
  cat "$data/server.log" >&2
  exit 1
}
