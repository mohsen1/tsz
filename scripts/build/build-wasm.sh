#!/usr/bin/env bash
set -euo pipefail

echo "error: TSZ WASM is intentionally unavailable during the clean-slate rewrite." >&2
echo "The native tsz-core service must stabilize first; WASM returns at roadmap milestone R4." >&2
exit 1
