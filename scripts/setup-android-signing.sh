#!/usr/bin/env bash
set -euo pipefail

repository="${1:-kure29/Neloa}"
signing_dir="${NELOA_SIGNING_DIR:-${HOME}/Documents/Neloa-signing}"
keystore_path="${signing_dir}/neloa-release.p12"

for command_name in openssl gh; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    echo "Missing required command: $command_name" >&2
    exit 1
  fi
done

if [[ -e "$keystore_path" ]]; then
  echo "Refusing to overwrite existing key: $keystore_path" >&2
  echo "Back it up or set NELOA_SIGNING_DIR to a new directory." >&2
  exit 1
fi

gh auth status >/dev/null

read -r -s -p "Choose Android signing password (at least 12 characters): " keystore_password
printf '\n'
read -r -s -p "Repeat password: " password_confirmation
printf '\n'

if [[ ${#keystore_password} -lt 12 ]]; then
  echo "Password must contain at least 12 characters." >&2
  exit 1
fi
if [[ "$keystore_password" != "$password_confirmation" ]]; then
  echo "Passwords do not match." >&2
  exit 1
fi

mkdir -p "$signing_dir"
chmod 700 "$signing_dir"
temporary_dir="$(mktemp -d "${TMPDIR:-/tmp}/neloa-android-signing.XXXXXX")"
trap 'rm -rf "$temporary_dir"' EXIT

export NELOA_ANDROID_KEY_PASSWORD="$keystore_password"
openssl req \
  -x509 \
  -newkey rsa:3072 \
  -sha256 \
  -days 10000 \
  -subj "/CN=Neloa Android Release/O=Neloa/C=CN" \
  -keyout "$temporary_dir/private-key.pem" \
  -out "$temporary_dir/certificate.pem" \
  -passout env:NELOA_ANDROID_KEY_PASSWORD

openssl pkcs12 \
  -export \
  -name neloa \
  -inkey "$temporary_dir/private-key.pem" \
  -in "$temporary_dir/certificate.pem" \
  -out "$temporary_dir/neloa-release.p12" \
  -passin env:NELOA_ANDROID_KEY_PASSWORD \
  -passout env:NELOA_ANDROID_KEY_PASSWORD

install -m 600 "$temporary_dir/neloa-release.p12" "$keystore_path"

base64 < "$keystore_path" | tr -d '\n' | gh secret set ANDROID_KEYSTORE_BASE64 --repo "$repository"
printf '%s' "$keystore_password" | gh secret set ANDROID_KEYSTORE_PASSWORD --repo "$repository"

unset NELOA_ANDROID_KEY_PASSWORD keystore_password password_confirmation

echo
echo "Android release signing is configured for $repository."
echo "Permanent key: $keystore_path"
echo "Back up this file and its password. Every future Android update must use the same key."
