#!/usr/bin/env bash
# Runs private_ca.rs: a CA of its own, GreenMail (IMAPS 4993, SMTPS 4465) serving a certificate
# for localhost signed by it, then the test with and without the CA in SSL_CERT_FILE.
# GreenMail runs in Docker, or with `java -jar "$GREENMAIL_JAR"` when that is set.
set -euo pipefail
cd "$(dirname "$0")/../../.."
dir=$(mktemp -d)
trap 'docker rm -f greenmail-private-ca >/dev/null 2>&1 || true; [ -n "${pid:-}" ] && kill "$pid" 2>/dev/null; rm -rf "$dir"' EXIT

openssl req -x509 -newkey rsa:2048 -nodes -days 2 -subj "/CN=nsh-mail test CA" \
  -addext "basicConstraints=critical,CA:TRUE" -addext "keyUsage=critical,keyCertSign,cRLSign" \
  -keyout "$dir/ca.key" -out "$dir/ca.pem" 2>/dev/null
openssl req -newkey rsa:2048 -nodes -subj "/CN=localhost" -keyout "$dir/server.key" -out "$dir/server.csr" 2>/dev/null
printf 'subjectAltName=DNS:localhost\nextendedKeyUsage=serverAuth\nbasicConstraints=CA:FALSE\n' > "$dir/ext"
openssl x509 -req -days 2 -in "$dir/server.csr" -CA "$dir/ca.pem" -CAkey "$dir/ca.key" -CAcreateserial \
  -extfile "$dir/ext" -out "$dir/server.pem" 2>/dev/null
openssl pkcs12 -export -name greenmail -passout pass:changeit -in "$dir/server.pem" -inkey "$dir/server.key" \
  -certfile "$dir/ca.pem" -out "$dir/server.p12"
chmod a+r "$dir"/*

opts="-Dgreenmail.auth.disabled -Dgreenmail.imaps.port=4993 -Dgreenmail.smtps.port=4465
  -Dgreenmail.tls.keystore.password=changeit -Dgreenmail.tls.key.password=changeit"
if [ -n "${GREENMAIL_JAR:-}" ]; then
  # shellcheck disable=SC2086
  java $opts -Dgreenmail.hostname=127.0.0.1 -Dgreenmail.tls.keystore.file="$dir/server.p12" -jar "$GREENMAIL_JAR" >/dev/null 2>&1 &
  pid=$!
else
  docker run -d --name greenmail-private-ca -p 4993:4993 -p 4465:4465 -v "$dir:/certs:ro" \
    -e GREENMAIL_OPTS="$(echo $opts) -Dgreenmail.hostname=0.0.0.0 -Dgreenmail.tls.keystore.file=/certs/server.p12" \
    greenmail/standalone:2.1.15 >/dev/null
fi
for _ in $(seq 60); do (echo > /dev/tcp/127.0.0.1/4993) 2>/dev/null && break; sleep 1; done

cargo test -p mail-core --test private_ca --no-run
PRIVATE_CA_TRUSTED=1 SSL_CERT_FILE="$dir/ca.pem" cargo test -p mail-core --test private_ca -- --ignored
PRIVATE_CA_TRUSTED=0 cargo test -p mail-core --test private_ca -- --ignored
