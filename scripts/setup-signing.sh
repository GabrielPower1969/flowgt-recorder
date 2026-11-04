#!/usr/bin/env bash
# setup-signing.sh: create a local self-signed code-signing identity in its own keychain under
# .signing/ (git-ignored). macOS ties privacy grants to the signature; an ad-hoc signature
# changes on every build, so each rebuild silently loses them. This identity stays the same.
# Development only: other Macs still need a Developer ID.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIR="$ROOT/.signing"
KC="$DIR/flowgt-dev.keychain-db"
if [ -f "$KC" ]; then echo "Signing identity already exists"; exit 0; fi
mkdir -p "$DIR"; chmod 700 "$DIR"
WORK="$(mktemp -d)"; trap 'rm -rf "$WORK"' EXIT
BEFORE="$(security list-keychains -d user | tr -d '"')"
PW="$(openssl rand -hex 24)"; printf '%s' "$PW" > "$DIR/keychain-password"; chmod 600 "$DIR/keychain-password"
cat > "$WORK/c.cnf" <<'CNF'
[req]
distinguished_name=dn
x509_extensions=ext
prompt=no
[dn]
CN=FlowGT Recorder Local Dev
[ext]
basicConstraints=critical,CA:false
keyUsage=critical,digitalSignature
extendedKeyUsage=critical,codeSigning
CNF
openssl req -x509 -newkey rsa:2048 -nodes -days 3650 -config "$WORK/c.cnf" -keyout "$WORK/k.pem" -out "$WORK/c.pem" 2>/dev/null
LEGACY=""; openssl version | grep -q '^OpenSSL 3' && LEGACY="-legacy"
openssl pkcs12 -export $LEGACY -inkey "$WORK/k.pem" -in "$WORK/c.pem" -out "$WORK/i.p12" -passout pass:x
security create-keychain -p "$PW" "$KC"
security set-keychain-settings "$KC"
security unlock-keychain -p "$PW" "$KC"
security import "$WORK/i.p12" -k "$KC" -P x -T /usr/bin/codesign >/dev/null
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$PW" "$KC" >/dev/null
# Leave the user's keychain search list as it was.
# shellcheck disable=SC2086
security list-keychains -d user -s $BEFORE
openssl x509 -in "$WORK/c.pem" -noout -fingerprint -sha1 | sed 's/.*=//; s/://g' > "$DIR/identity"
echo "Created identity $(cat "$DIR/identity")"
