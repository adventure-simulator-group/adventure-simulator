#!/usr/bin/env bash
# 
# Generate a locally trusted TLS certificate using mkcert.
#
# This is intended for local development (e.g. wss://127.0.0.1:6000).
#
# What this does:
# 1. Verifies mkcert is installed
# 2. Installs the local mkcert if needed (one-time per machine)
# 3. Generates a certificate valid for:
#    - localhost
#    - 127.0.0.1
#    - ::1
#
# Output files:
#   certs/ca_cert.pem   (certificate authority)
#   certs/cert.pem      (certificate chain)
#   certs/key.pem       (private key)
#
# NOTE:
# - mkcert certificates are trusted ONLY on this machine
# - Do NOT use in production
# - uninstall with `mkcert -uninstall`
 
set -euo pipefail

echo "🔐 Generating local TLS certificate with mkcert for local environment..."

if ! command -v mkcert >/dev/null 2>&1; then
    echo "❌ mkcert is not installed, get it from https://github.com/FiloSottile/mkcert"
    exit 1
fi

SCRIPT_DIR="$(cd -- "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CERT_DIR="$SCRIPT_DIR/certs"
mkdir -p "$CERT_DIR"

# install local CA if needed
if ! mkcert -CAROOT >/dev/null 2>&1; then
    echo "🔧 Installing mkcert local CA (one-time operation)"
    mkcert -install
fi

CA_CERT_FILE="$CERT_DIR/ca_cert.pem"
CERT_FILE="$CERT_DIR/cert.pem"
KEY_FILE="$CERT_DIR/key.pem"
mkcert \
    -cert-file "$CERT_FILE" \
    -key-file  "$KEY_FILE" \
    localhost 127.0.0.1 ::1

rm "$CA_CERT_FILE"
cp "$(mkcert -CAROOT)/rootCA.pem" "$CA_CERT_FILE"
