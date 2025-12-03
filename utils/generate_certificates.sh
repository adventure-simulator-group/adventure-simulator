#!/usr/bin/env bash
# 
# Generates a self-signed certificate valid for 14 days, to use for webtransport.
# Can be run from any directory. Pass SANs as comma-separated arg, e.g.:
# ./generate_certificates.sh "127.0.0.1,localhost"
 
DEFAULT_DOMAINS="127.0.0.1,localhost"
set -eo pipefail

# Show usage message
usage() {
    cat << EOF
Usage: $0 [DOMAIN1, DOMAIN2, ...]

Generate certificates for a comma-separated list of domains.
If no domain list is provided, the following defaults are used:
  $DEFAULT_DOMAINS

Examples:
  $0
  $0 "192.168.0.1,www.example.com"

Options:
  -h, --help    Show this help message and exit.
EOF
}

case "${1:-}" in
    -h|--help)
        usage
        exit 0
        ;;
esac

SCRIPT_DIR="$(cd -- "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUT="$SCRIPT_DIR"
SANS_RAW="${1:-$DEFAULT_DOMAINS}"

echo "Generating certificates for ${SANS_RAW}..."

IFS=',' read -ra SAN_ITEMS <<< "$SANS_RAW"
ALT_NAMES=""
dns_idx=1
ip_idx=1
for item in "${SAN_ITEMS[@]}"; do
  if [[ "$item" =~ ^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    ALT_NAMES+="IP.${ip_idx} = ${item}"$'\n'
    ((ip_idx++))
  else
    ALT_NAMES+="DNS.${dns_idx} = ${item}"$'\n'
    ((dns_idx++))
  fi
done

CFG="$(mktemp)"
cat > "$CFG" <<EOF
[req]
distinguished_name = dn
prompt = no
req_extensions = req_ext
[dn]
CN = ${SAN_ITEMS[0]}
[req_ext]
subjectAltName = @alt_names
[alt_names]
${ALT_NAMES}
EOF

openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -keyout "$OUT/key.pem" -out "$OUT/cert.pem" -days 14 -nodes -config "$CFG" -extensions req_ext
rm -f "$CFG"

FINGERPRINT=$(openssl x509 -in "$OUT/cert.pem" -noout -sha256 -fingerprint | sed 's/^.*=//' | sed 's/://g')
printf '%s' "$FINGERPRINT" > "$OUT/digest.txt"

echo "Wrote new fingerprint $FINGERPRINT to $OUT/digest.txt"
