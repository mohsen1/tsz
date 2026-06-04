#!/usr/bin/env bash
#
# Create or refresh the GitHub labels used by multi-agent sessions.

set -euo pipefail

AGENTS=(
  M1-A M1-B M1-D M1-Opus
  M4-A M4-B M4-C M4-Opus
  Studio-A Studio-B Studio-C Studio-Opus
  Studio-manager
)

COLOR="ededed"

usage() {
  cat <<'USAGE'
usage: scripts/agents/ensure-agent-labels.sh [--audit] [--strict] [--json-report PATH]

Create or refresh the GitHub labels used by multi-agent sessions.

With --audit, list noncanonical agent ownership labels; open PRs whose
agent ownership labels are missing, duplicated, or noncanonical; and open
release-triage issues whose agent ownership labels are missing, duplicated, or
noncanonical. Open PRs whose body explicitly says no canonical agent lane was
assigned are reported separately. The audit does not edit labels.

With --audit --strict, exit nonzero when the audit has actionable findings.

With --audit --json-report PATH, also write a machine-readable audit report.
USAGE
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

AUDIT=false
STRICT_AUDIT=false
JSON_REPORT=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --audit)
      AUDIT=true
      ;;
    --strict)
      STRICT_AUDIT=true
      ;;
    --json-report)
      shift
      if [[ $# -eq 0 ]]; then
        echo "--json-report requires a path (try --help)" >&2
        exit 2
      fi
      JSON_REPORT="$1"
      ;;
    --json-report=*)
      JSON_REPORT="${1#--json-report=}"
      if [[ -z "$JSON_REPORT" ]]; then
        echo "--json-report requires a path (try --help)" >&2
        exit 2
      fi
      ;;
    *)
      echo "Unknown option: $1 (try --help)" >&2
      exit 2
      ;;
  esac
  shift
done

if [[ "$STRICT_AUDIT" == true && "$AUDIT" != true ]]; then
  echo "--strict requires --audit (try --help)" >&2
  exit 2
fi

if [[ -n "$JSON_REPORT" && "$AUDIT" != true ]]; then
  echo "--json-report requires --audit (try --help)" >&2
  exit 2
fi

is_canonical_agent_label() {
  local label="$1"
  case "$label" in
    agent:M1-A|agent:M1-B|agent:M1-D|agent:M1-Opus|\
    agent:M4-A|agent:M4-B|agent:M4-C|agent:M4-Opus|\
    agent:Studio-A|agent:Studio-B|agent:Studio-C|agent:Studio-Opus|\
    agent:Studio-manager)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

collect_git_context() {
  GIT_HEAD="$(git rev-parse HEAD 2>/dev/null || echo unknown)"
  GIT_BRANCH="$(git symbolic-ref --short -q HEAD 2>/dev/null || true)"
  GIT_DETACHED=false
  if [[ -z "$GIT_BRANCH" ]]; then
    GIT_DETACHED=true
    if [[ "$GIT_HEAD" == "unknown" ]]; then
      GIT_BRANCH="detached:unknown"
    else
      GIT_BRANCH="detached:${GIT_HEAD:0:12}"
    fi
  fi
  GIT_UPSTREAM="$(git rev-parse --abbrev-ref --symbolic-full-name '@{upstream}' 2>/dev/null || true)"
}

existing="$(gh label list --limit 300 --json name --jq '.[].name')"

if [[ "$AUDIT" == true ]]; then
  prs_json="$(gh pr list --state open --limit 500 --json number,title,isDraft,labels,body,url)"
  issues_json="$(gh issue list --state open --limit 500 --json number,title,labels,url)"
  agents_json="$(
    printf '%s\n' "${AGENTS[@]}" | node -e '
      const fs = require("fs");
      process.stdout.write(JSON.stringify(fs.readFileSync(0, "utf8").trim().split(/\n/).filter(Boolean)));
    '
  )"
  if [[ -n "$JSON_REPORT" ]]; then
    collect_git_context
  fi
  AGENTS_JSON="$agents_json" \
  LABELS_TEXT="$existing" \
  PRS_JSON="$prs_json" \
  ISSUES_JSON="$issues_json" \
  STRICT_AUDIT="$STRICT_AUDIT" \
  JSON_REPORT="$JSON_REPORT" \
  GIT_HEAD="${GIT_HEAD:-}" \
  GIT_BRANCH="${GIT_BRANCH:-}" \
  GIT_DETACHED="${GIT_DETACHED:-false}" \
  GIT_UPSTREAM="${GIT_UPSTREAM:-}" \
  node <<'NODE'
const fs = require("fs");
const path = require("path");
const canonical = new Set(JSON.parse(process.env.AGENTS_JSON).map((agent) => `agent:${agent}`));
const labels = process.env.LABELS_TEXT.split(/\n/).filter(Boolean);
const prs = JSON.parse(process.env.PRS_JSON);
const issues = JSON.parse(process.env.ISSUES_JSON);
const releaseTriageIssueLabels = new Set([
  "accepted-regression",
  "bug",
  "false-negative",
  "false-positive",
  "urgent",
  "WIP",
]);

const ownershipLabel = (label) => label.startsWith("agent:") || label.startsWith("agnet:");
const noncanonicalLabels = labels
  .filter((label) => ownershipLabel(label) && !canonical.has(label))
  .sort();
const missingCanonicalLabels = [...canonical].filter((label) => !labels.includes(label)).sort();

const missingPrs = [];
const intentionallyUnassignedPrs = [];
const multiplePrs = [];
const noncanonicalPrs = [];
const missingIssues = [];
const multipleIssues = [];
const noncanonicalIssues = [];
for (const pr of prs) {
  const agentLabels = pr.labels.map((label) => label.name).filter(ownershipLabel);
  if (agentLabels.length === 0) {
    if (/\bno canonical agent lane was assigned\b/i.test(pr.body ?? "")) {
      intentionallyUnassignedPrs.push(pr);
      continue;
    }
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
const readyIntentionallyUnassignedPrs = intentionallyUnassignedPrs.filter(
  (pr) => pr.isDraft !== true,
);
const draftIntentionallyUnassignedPrs = intentionallyUnassignedPrs.filter(
  (pr) => pr.isDraft === true,
);

for (const issue of issues) {
  const labels = issue.labels.map((label) => label.name);
  const agentLabels = labels.filter(ownershipLabel);
  const needsOwner = labels.some((label) => releaseTriageIssueLabels.has(label));
  if (needsOwner && agentLabels.length === 0) {
    missingIssues.push(issue);
  }
  if (agentLabels.length > 1) {
    multipleIssues.push({ ...issue, agentLabels });
  }
  const generated = agentLabels.filter((label) => !canonical.has(label));
  if (generated.length > 0) {
    noncanonicalIssues.push({ ...issue, agentLabels: generated });
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
console.log(`open_prs_intentionally_unassigned=${intentionallyUnassignedPrs.length}`);
console.log(
  `open_ready_prs_intentionally_unassigned=${readyIntentionallyUnassignedPrs.length}`,
);
console.log(
  `open_draft_prs_intentionally_unassigned=${draftIntentionallyUnassignedPrs.length}`,
);
console.log(`open_prs_multiple_agent_labels=${multiplePrs.length}`);
console.log(`open_prs_noncanonical_agent_label=${noncanonicalPrs.length}`);
console.log(`open_release_issues_missing_agent_label=${missingIssues.length}`);
console.log(`open_issues_multiple_agent_labels=${multipleIssues.length}`);
console.log(`open_issues_noncanonical_agent_label=${noncanonicalIssues.length}`);
const findingCount =
  missingCanonicalLabels.length +
  noncanonicalLabels.length +
  missingPrs.length +
  multiplePrs.length +
  noncanonicalPrs.length +
  missingIssues.length +
  multipleIssues.length +
  noncanonicalIssues.length;
const ok = findingCount === 0;
const warningCount = readyIntentionallyUnassignedPrs.length;
const metrics = {
  missing_canonical_labels: missingCanonicalLabels.length,
  noncanonical_agent_labels: noncanonicalLabels.length,
  open_prs_missing_agent_label: missingPrs.length,
  open_prs_intentionally_unassigned: intentionallyUnassignedPrs.length,
  open_ready_prs_intentionally_unassigned: readyIntentionallyUnassignedPrs.length,
  open_draft_prs_intentionally_unassigned: draftIntentionallyUnassignedPrs.length,
  open_prs_multiple_agent_labels: multiplePrs.length,
  open_prs_noncanonical_agent_label: noncanonicalPrs.length,
  open_release_issues_missing_agent_label: missingIssues.length,
  open_issues_multiple_agent_labels: multipleIssues.length,
  open_issues_noncanonical_agent_label: noncanonicalIssues.length,
  agent_label_audit_findings: findingCount,
  agent_label_audit_warnings: warningCount,
};
console.log(`agent_label_audit_findings=${findingCount}`);
console.log(`agent_label_audit_warnings=${warningCount}`);
console.log(`agent_label_audit_status=${ok ? "pass" : "fail"}`);

const summarizeWorkItem = (item) => ({
  number: item.number,
  title: item.title,
  url: item.url ?? null,
});
const prAgentName = (pr) => {
  const match = /^AgentName:[^\S\r\n]*(\S+)?/m.exec(pr.body ?? "");
  return match?.[1]?.replace(/^`+|`+$/g, "") ?? null;
};
const summarizeMissingPrWorkItem = (item) => ({
  ...summarizeWorkItem(item),
  agent_name: prAgentName(item),
});
const summarizePrWorkItem = (item) => ({
  ...summarizeWorkItem(item),
  is_draft: item.isDraft === true,
});
const summarizeLabeledWorkItem = (item) => ({
  ...summarizeWorkItem(item),
  agent_labels: item.agentLabels,
});
if (process.env.JSON_REPORT) {
  const report = {
    ok,
    status: ok ? "pass" : "fail",
    agent_label_audit_status: ok ? "pass" : "fail",
    warning_count: warningCount,
    warning_status: warningCount > 0 ? "warn" : "clear",
    git_context: {
      head: process.env.GIT_HEAD,
      branch: process.env.GIT_BRANCH,
      detached: process.env.GIT_DETACHED === "true",
      upstream: process.env.GIT_UPSTREAM || null,
    },
    metrics,
    missing_canonical_labels: missingCanonicalLabels,
    noncanonical_agent_labels: noncanonicalLabels,
    open_prs_missing_agent_label: missingPrs.map(summarizeMissingPrWorkItem),
    open_prs_intentionally_unassigned: intentionallyUnassignedPrs.map(summarizePrWorkItem),
    open_ready_prs_intentionally_unassigned: readyIntentionallyUnassignedPrs.map(
      summarizePrWorkItem,
    ),
    open_draft_prs_intentionally_unassigned: draftIntentionallyUnassignedPrs.map(
      summarizePrWorkItem,
    ),
    open_prs_multiple_agent_labels: multiplePrs.map(summarizeLabeledWorkItem),
    open_prs_noncanonical_agent_label: noncanonicalPrs.map(summarizeLabeledWorkItem),
    open_release_issues_missing_agent_label: missingIssues.map(summarizeWorkItem),
    open_issues_multiple_agent_labels: multipleIssues.map(summarizeLabeledWorkItem),
    open_issues_noncanonical_agent_label: noncanonicalIssues.map(summarizeLabeledWorkItem),
  };
  fs.mkdirSync(path.dirname(process.env.JSON_REPORT), { recursive: true });
  fs.writeFileSync(process.env.JSON_REPORT, `${JSON.stringify(report, null, 2)}\n`);
}

printRows("Missing Canonical Labels", missingCanonicalLabels, (label) => `- ${label}`);
printRows("Noncanonical Agent Labels", noncanonicalLabels, (label) => `- ${label}`);
const prRow = (pr) => `- #${pr.number} ${pr.title}${pr.url ? ` ${pr.url}` : ""}`;
const missingPrRow = (pr) => {
  const agentName = prAgentName(pr);
  return `${prRow(pr)}${agentName ? ` AgentName=${agentName}` : ""}`;
};

printRows("Open PRs Missing Agent Label", missingPrs, missingPrRow);
printRows(
  "Open PRs Intentionally Unassigned",
  intentionallyUnassignedPrs,
  prRow,
);
printRows(
  "Open Ready PRs Intentionally Unassigned",
  readyIntentionallyUnassignedPrs,
  prRow,
);
printRows(
  "Open Draft PRs Intentionally Unassigned",
  draftIntentionallyUnassignedPrs,
  prRow,
);
printRows(
  "Open PRs With Multiple Agent Labels",
  multiplePrs,
  (pr) => `- #${pr.number} ${pr.agentLabels.join(", ")} ${pr.title}${pr.url ? ` ${pr.url}` : ""}`,
);
printRows(
  "Open PRs With Noncanonical Agent Labels",
  noncanonicalPrs,
  (pr) => `- #${pr.number} ${pr.agentLabels.join(", ")} ${pr.title}${pr.url ? ` ${pr.url}` : ""}`,
);
printRows("Open Release Issues Missing Agent Label", missingIssues, prRow);
printRows(
  "Open Issues With Multiple Agent Labels",
  multipleIssues,
  (issue) => `- #${issue.number} ${issue.agentLabels.join(", ")} ${issue.title}${issue.url ? ` ${issue.url}` : ""}`,
);
printRows(
  "Open Issues With Noncanonical Agent Labels",
  noncanonicalIssues,
  (issue) => `- #${issue.number} ${issue.agentLabels.join(", ")} ${issue.title}${issue.url ? ` ${issue.url}` : ""}`,
);
if (process.env.STRICT_AUDIT === "true" && findingCount > 0) {
  process.exitCode = 1;
}
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
