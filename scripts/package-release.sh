#!/usr/bin/env bash
set -euo pipefail
shopt -s nullglob

package_asset() {
  local bundle_dir=$1 format=$2 asset=$3 magic
  local files=("$bundle_dir"/*."$format")
  if [[ "${#files[@]}" != 1 ]]; then
    printf 'Expected one .%s in %s, found %s\n' "$format" "$bundle_dir" "${#files[@]}" >&2
    return 1
  fi
  case "$format" in
    dmg) magic=$(tail -c 512 "${files[0]}" | dd bs=4 count=1 2>/dev/null); [[ "$magic" = koly ]] ;;
    exe) magic=$(head -c 2 "${files[0]}"); [[ "$magic" = MZ ]] ;;
    deb) magic=$(head -c 7 "${files[0]}"); [[ "$magic" = '!<arch>' ]] ;;
    AppImage) magic=$(od -An -tx1 -j 8 -N 3 "${files[0]}" | tr -d ' \n'); [[ "$magic" = 414902 ]] ;;
    *) printf 'Unsupported package type: %s\n' "$format" >&2; return 1 ;;
  esac
  mkdir -p upload
  cp "${files[0]}" "upload/$asset"
}

package_asset "$@"
