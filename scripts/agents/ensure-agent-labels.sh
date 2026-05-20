#!/usr/bin/env bash
#
# Create or refresh the GitHub labels used by multi-agent sessions.

set -euo pipefail

AGENTS=(
  M1-A M1-B M1-C M1-D
  M4-A M4-B M4-C M4-D
  Studio-A Studio-B Studio-C Studio-D Studio-E Studio-F
  Reviewer
)

COLOR="ededed"

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  echo "usage: scripts/agents/ensure-agent-labels.sh"
  exit 0
fi

existing="$(gh label list --limit 300 --json name --jq '.[].name')"

for agent in "${AGENTS[@]}"; do
  label="agent:${agent}"
  description="Active ownership lane for ${agent}; exactly one agent label per owned issue or PR"
  if printf '%s\n' "$existing" | grep -Fxq "$label"; then
    gh label edit "$label" --description "$description" --color "$COLOR" >/dev/null
    echo "updated $label"
  else
    gh label create "$label" --description "$description" --color "$COLOR" >/dev/null
    echo "created $label"
  fi
done
