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

usage() {
  cat <<'USAGE'
usage: scripts/agents/ensure-agent-labels.sh [--audit]

Create or refresh the GitHub labels used by multi-agent sessions.

With --audit, list noncanonical agent ownership labels such as generated
runner/model aliases or typo labels. The audit does not edit labels.
USAGE
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

AUDIT=false
if [[ $# -eq 1 && "${1:-}" == "--audit" ]]; then
  AUDIT=true
elif [[ $# -ne 0 ]]; then
  echo "Unknown option: $1 (try --help)" >&2
  exit 2
fi

is_canonical_agent_label() {
  local label="$1"
  case "$label" in
    agent:M1-A|agent:M1-B|agent:M1-C|agent:M1-D|\
    agent:M4-A|agent:M4-B|agent:M4-C|agent:M4-D|\
    agent:Studio-A|agent:Studio-B|agent:Studio-C|agent:Studio-D|agent:Studio-E|agent:Studio-F|\
    agent:Reviewer)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

existing="$(gh label list --limit 300 --json name --jq '.[].name')"

if [[ "$AUDIT" == true ]]; then
  found=0
  while IFS= read -r label; do
    case "$label" in
      agent:*|agnet:*)
        if ! is_canonical_agent_label "$label"; then
          echo "noncanonical $label"
          found=1
        fi
        ;;
    esac
  done <<< "$existing"

  if [[ "$found" -eq 0 ]]; then
    echo "no noncanonical agent labels found"
  fi
  exit 0
fi

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
