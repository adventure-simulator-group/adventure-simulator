# Generates a self-signed certificate valid for 14 days, to use for webtransport.
# Can be run from any directory. Pass SANs as comma-separated arg, e.g.:
# ./generate.sh "127.0.0.1,localhost"
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUT="$SCRIPT_DIR"
SANS_RAW="${1:-127.0.0.1,localhost}"

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
