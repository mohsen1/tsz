#!/usr/bin/env bash
# Remove stale offline self-hosted runners from GitHub.
# Safe: only removes runners that are offline (not online/busy).
# Usage: scripts/ci/cleanup-stale-runners.sh [--dry-run]
set -euo pipefail

REPO="${GITHUB_REPO:-mohsen1/tsz}"
DRY_RUN=false

for arg in "$@"; do
  case "$arg" in
    --dry-run) DRY_RUN=true ;;
    *) echo "usage: $0 [--dry-run]" >&2; exit 1 ;;
  esac
done

echo "Fetching runner list for ${REPO} ..."
runners=$(gh api "repos/${REPO}/actions/runners" --paginate --jq '.runners[]' 2>/dev/null)

offline_ids=$(echo "$runners" | python3 -c "
import json, sys
for line in sys.stdin:
    line = line.strip()
    if not line: continue
    r = json.loads(line)
    if r.get('status') == 'offline':
        print(r['id'], r['name'])
" 2>/dev/null)

if [[ -z "$offline_ids" ]]; then
  echo "No offline runners found."
  exit 0
fi

echo "Offline runners to remove:"
echo "$offline_ids"

if $DRY_RUN; then
  echo "(dry-run: no runners deleted)"
  exit 0
fi

echo ""
echo "$offline_ids" | while read -r id name; do
  echo "Removing runner #${id} (${name}) ..."
  gh api -X DELETE "repos/${REPO}/actions/runners/${id}" && echo "  ✓ removed" || echo "  ✗ failed (may already be gone)"
done

echo ""
echo "Done. Remaining runners:"
gh api "repos/${REPO}/actions/runners" --jq \
  '.runners | group_by(.status) | map({status: .[0].status, count: length}) | .[] | "\(.status): \(.count)"'
