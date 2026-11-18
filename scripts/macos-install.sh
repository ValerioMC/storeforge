#!/usr/bin/env bash
set -euo pipefail

bundle=$1
applications=$2
destination="$applications/StoreForge.app"
staging=$(mktemp -d "$applications/.storeforge-install.XXXXXX")

cleanup() {
  if [[ -d "$staging/previous.app" && ! -e "$destination" ]]; then
    mv "$staging/previous.app" "$destination"
  fi
  rm -rf "$staging"
}

trap cleanup EXIT
test -x "$bundle/Contents/MacOS/storeforge"
ditto "$bundle" "$staging/StoreForge.app"
if [[ ! -f "$staging/StoreForge.app/Contents/_CodeSignature/CodeResources" ]]; then
  codesign --force --sign - "$staging/StoreForge.app"
fi
codesign --verify --deep --strict "$staging/StoreForge.app"

if [[ -e "$destination" ]]; then
  mv "$destination" "$staging/previous.app"
fi
mv "$staging/StoreForge.app" "$destination"
printf 'Installed %s\n' "$destination"
