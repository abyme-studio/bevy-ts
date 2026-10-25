#!/usr/bin/env bash
# Remakes the test authority and its two leaves (README.md). ECDSA P-256, valid 100 years. The
# authority's key is not kept: a new one means remaking every leaf.
set -euo pipefail
cd "$(dirname "$0")"
openssl ecparam -name prime256v1 -genkey -noout -out ca.key
openssl req -x509 -new -key ca.key -sha256 -days 36500 -subj "/CN=bevy-ts test CA" \
  -addext "basicConstraints=critical,CA:TRUE" -addext "keyUsage=critical,keyCertSign,cRLSign" -out ca.pem
leaf() {
  local name=$1 san=$2
  openssl ecparam -name prime256v1 -genkey -noout -out "$name.key.sec1"
  openssl pkcs8 -topk8 -nocrypt -in "$name.key.sec1" -out "$name.key"
  rm "$name.key.sec1"
  openssl req -new -key "$name.key" -subj "/CN=$name" -out "$name.csr"
  printf 'basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature\nextendedKeyUsage=serverAuth\nsubjectAltName=%s\n' "$san" > "$name.ext"
  openssl x509 -req -in "$name.csr" -CA ca.pem -CAkey ca.key -CAcreateserial -days 36500 -sha256 -extfile "$name.ext" -out "$name.pem"
  rm "$name.csr" "$name.ext"
}
leaf ip "IP:127.0.0.1"
leaf localhost "DNS:localhost"
rm -f ca.srl ca.key
