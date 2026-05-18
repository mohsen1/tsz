#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

target="${TSZ_SANITIZER_TARGET:-x86_64-unknown-linux-gnu}"
sanitizer="${TSZ_SANITIZER:-address}"
package="${TSZ_SANITIZER_PACKAGE:-tsz-scanner}"

case "$(uname -s)" in
  Linux) ;;
  *)
    echo "Sanitizer smoke tests are intended for Linux CI runners; got $(uname -s)." >&2
    exit 0
    ;;
esac

RUSTFLAGS="-Zsanitizer=${sanitizer}" scripts/safe-run.sh --limit "${TSZ_SANITIZER_MEMORY_LIMIT:-75%}" -- \
  rustup run nightly cargo test -Zbuild-std --target "$target" -p "$package" --lib
