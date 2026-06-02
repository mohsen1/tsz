#!/bin/bash
# Smoke-test native npm packaging copies from the same Cargo target root it
# builds into.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
tmp_dir="$(mktemp -d)"
cleanup() {
  rm -rf "$tmp_dir" "$PROJECT_ROOT/npm"
}
trap cleanup EXIT

case "$(uname -s)" in
  Darwin) os="darwin" ;;
  Linux) os="linux" ;;
  MINGW*|MSYS*|CYGWIN*) os="win32" ;;
  *) echo "unsupported OS: $(uname -s)" >&2; exit 1 ;;
esac

case "$(uname -m)" in
  x86_64|amd64) arch="x64" ;;
  arm64|aarch64) arch="arm64" ;;
  *) echo "unsupported arch: $(uname -m)" >&2; exit 1 ;;
esac

suffix="$os-$arch"
ext=""
if [[ "$os" == "win32" ]]; then
  ext=".exe"
fi

rm -rf "$PROJECT_ROOT/npm"
CARGO_TARGET_DIR="$tmp_dir/target" \
  "$PROJECT_ROOT/scripts/build/build-npm-packages.sh" --local --native-only

pkg_bin="$PROJECT_ROOT/npm/@mohsen-azimi/tsz-$suffix/bin"
for bin in tsz tsz-server try-tsz; do
  if [[ ! -x "$pkg_bin/$bin$ext" ]]; then
    echo "missing packaged native binary: $pkg_bin/$bin$ext" >&2
    exit 1
  fi
done

try_pkg_bin="$PROJECT_ROOT/npm/try-tsz-$suffix/bin"
if [[ ! -x "$try_pkg_bin/try-tsz$ext" ]]; then
  echo "missing packaged try-tsz native binary: $try_pkg_bin/try-tsz$ext" >&2
  exit 1
fi

echo "native npm copy smoke passed for $suffix"
