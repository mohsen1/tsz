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

With --audit, list noncanonical agent ownership labels and open PRs whose
agent ownership labels are missing, duplicated, or noncanonical. The audit
does not edit labels.
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
  prs_json="$(gh pr list --state open --limit 500 --json number,title,labels)"
  agents_json="$(
    printf '%s\n' "${AGENTS[@]}" | node -e '
      const fs = require("fs");
      process.stdout.write(JSON.stringify(fs.readFileSync(0, "utf8").trim().split(/\n/).filter(Boolean)));
    '
  )"
  AGENTS_JSON="$agents_json" LABELS_TEXT="$existing" PRS_JSON="$prs_json" node <<'NODE'
const canonical = new Set(JSON.parse(process.env.AGENTS_JSON).map((agent) => `agent:${agent}`));
const labels = process.env.LABELS_TEXT.split(/\n/).filter(Boolean);
const prs = JSON.parse(process.env.PRS_JSON);

const ownershipLabel = (label) => label.startsWith("agent:") || label.startsWith("agnet:");
const noncanonicalLabels = labels
  .filter((label) => ownershipLabel(label) && !canonical.has(label))
  .sort();
const missingCanonicalLabels = [...canonical].filter((label) => !labels.includes(label)).sort();

const missingPrs = [];
const multiplePrs = [];
const noncanonicalPrs = [];
for (const pr of prs) {
  const agentLabels = pr.labels.map((label) => label.name).filter(ownershipLabel);
  if (agentLabels.length === 0) {
    missingPrs.push(pr);
    continue;
  }
  if (agentLabels.length > 1) {
    multiplePrs.push({ ...pr, agentLabels });
  }
  const generated = agentLabels.filter((label) => !canonical.has(label));
  if (generated.length > 0) {
    noncanonicalPrs.push({ ...pr, agentLabels: generated });
  }
}

function printRows(title, rows, format) {
  console.log(`\n## ${title}`);
  if (rows.length === 0) {
    console.log("- none");
    return;
  }
  for (const row of rows) {
    console.log(format(row));
  }
}

console.log("# Agent Label Audit");
console.log("");
console.log(`missing_canonical_labels=${missingCanonicalLabels.length}`);
console.log(`noncanonical_agent_labels=${noncanonicalLabels.length}`);
console.log(`open_prs_missing_agent_label=${missingPrs.length}`);
console.log(`open_prs_multiple_agent_labels=${multiplePrs.length}`);
console.log(`open_prs_noncanonical_agent_label=${noncanonicalPrs.length}`);

printRows("Missing Canonical Labels", missingCanonicalLabels, (label) => `- ${label}`);
printRows("Noncanonical Agent Labels", noncanonicalLabels, (label) => `- ${label}`);
printRows("Open PRs Missing Agent Label", missingPrs, (pr) => `- #${pr.number} ${pr.title}`);
printRows(
  "Open PRs With Multiple Agent Labels",
  multiplePrs,
  (pr) => `- #${pr.number} ${pr.agentLabels.join(", ")} ${pr.title}`,
);
printRows(
  "Open PRs With Noncanonical Agent Labels",
  noncanonicalPrs,
  (pr) => `- #${pr.number} ${pr.agentLabels.join(", ")} ${pr.title}`,
);
NODE
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
