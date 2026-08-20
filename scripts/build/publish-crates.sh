#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CRATES=(tsz-core tsz-cli)

if [[ "${1:-}" == "--dry-run" && "$#" -eq 1 ]]; then
  echo "TSZ rewrite crates are not publishable; inspecting their package contents only."
  for crate in "${CRATES[@]}"; do
    echo "--- ${crate} ---"
    cargo package --manifest-path "$ROOT/Cargo.toml" --list --no-verify -p "$crate"
  done
  exit 0
fi

if [[ "$#" -ne 0 ]]; then
  echo "usage: $0 [--dry-run]" >&2
  exit 2
fi

echo "error: crates.io publication is intentionally disabled during the clean-slate rewrite." >&2
echo "tsz-core and tsz-cli are R0 validation artifacts, not release-ready packages." >&2
exit 1
