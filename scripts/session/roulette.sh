#!/usr/bin/env bash
# roulette.sh — pick a random conformance failure and print a ready-to-run
# verbose command for investigating it.
#
# Usage:
#   scripts/session/roulette.sh                 # pick any failure
#   scripts/session/roulette.sh fingerprint     # only fingerprint-only
#   scripts/session/roulette.sh wrong           # only wrong-code
#   scripts/session/roulette.sh missing         # only all-missing (incl. crashes)
#   scripts/session/roulette.sh fp TS2322       # fingerprint-only with code
#   SEED=42 scripts/session/roulette.sh         # reproducible pick
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"

category="any"
code=""
case "${1:-}" in
    fingerprint|fp) category="fingerprint-only" ;;
    wrong|wrong-code) category="wrong-code" ;;
    missing|all-missing) category="all-missing" ;;
    false-positive|fp-pos) category="false-positive" ;;
    only-missing) category="only-missing" ;;
    only-extra) category="only-extra" ;;
    any|"") category="any" ;;
    *) echo "unknown category: $1" >&2; exit 1 ;;
esac
if [[ -n "${2:-}" ]]; then
    code="$2"
fi

args=(--category "$category" --count 1)
if [[ -n "$code" ]]; then
    args+=(--code "$code")
fi
if [[ -n "${SEED:-}" ]]; then
    args+=(--seed "$SEED")
fi

pick="$(python3 "$SCRIPT_DIR/pick-random-failure.py" "${args[@]}" 2>/dev/null)"
if [[ -z "$pick" ]]; then
    echo "no failure matched (category=$category code=$code)" >&2
    exit 1
fi

path="$(awk '/^path:/{print $2; exit}' <<<"$pick")"
base="$(basename "$path" .ts)"
base="${base%.tsx}"

echo "$pick"
echo "----"
echo "next commands:"
echo "  ./scripts/conformance/conformance.sh run --filter '$base' --verbose"
echo "  python3 scripts/conformance/query-conformance.py --code \$(awk '/^expected/{print \$2}' <<<\"\$PICK\" | cut -d, -f1)"
