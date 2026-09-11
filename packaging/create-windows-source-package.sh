#!/bin/sh
set -eu

project_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
version=$(node -p "require('${project_root}/package.json').version")
stage_root=$(mktemp -d "${TMPDIR:-/tmp}/neloa-package.XXXXXX")
stage_dir="${stage_root}/Neloa-Windows-Source-${version}"
archive="${project_root}/releases/Neloa-Windows-Source-${version}.zip"

trap 'rm -rf "$stage_root"' EXIT
mkdir -p "$stage_dir" "${project_root}/releases"

rsync -a \
  --exclude '.DS_Store' \
  --exclude '.git' \
  --exclude 'node_modules' \
  --exclude 'dist' \
  --exclude 'releases' \
  --exclude 'src-tauri/target' \
  "$project_root/" "$stage_dir/"

rm -f "$archive"
(
  cd "$stage_root"
  COPYFILE_DISABLE=1 zip -qr "$archive" "Neloa-Windows-Source-${version}"
)

printf '%s\n' "$archive"
