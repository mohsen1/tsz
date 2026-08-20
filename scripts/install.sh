#!/usr/bin/env bash
set -euo pipefail

echo "error: TSZ installation is unavailable during the clean-slate rewrite." >&2
echo "The current R0 compiler is a validation artifact and is not published for installation." >&2
exit 1
