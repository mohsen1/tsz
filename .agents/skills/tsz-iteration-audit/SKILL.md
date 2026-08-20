---
name: tsz-iteration-audit
description: Audit TSZ repo iteration speed and quality. Use when asked to deep-dive repo health, identify avoidable delays, improve agent workflows, reduce CI/PR churn, add guardrails, choose skills to install or create, or turn recurring TSZ process failures into focused tooling/docs PRs.
---

# TSZ Iteration Audit

Use to turn recurring workflow/context/guardrail costs into small,
evidence-backed PRs. Do not create roadmap/status docs unless durable project
direction changes.

## Audit

```bash
sed -n '1,320p' docs/plan/ROADMAP.md
gh pr list --state open --limit 100 --json number,title,isDraft,headRefName,baseRefName,labels,updatedAt,url
gh pr list --state merged --limit 30 --json number,title,headRefName,mergedAt,url
gh issue list --state open --limit 100 --json number,title,labels,updatedAt,url
scripts/agents/llm-context-audit.py
find .agents/skills scripts/agents scripts/ci scripts/bench scripts/conformance scripts/emit -maxdepth 3 -type f | sort
```

Use targeted searches only when needed:

```bash
rg -n "source_text\\.contains|format.*diagnostic|TypeId|force_|materialize|cache|memo" crates scripts
rg -n "WIP|stale|drift|queue|worktree|disk|allowlist|fingerprint|context" docs scripts .agents .claude .codex
```

## Classify

- `workflow`: stale starts, lost coordination.
- `guardrail`: missing architecture/file-size/WIP/PR-body/CI check.
- `visibility`: data exists but is hard to find.
- `duplication`: policy repeated across scripts/docs/skills.
- `behavior debt`: compiler bug; route to owner skill/issue.
- `context debt`: startup hooks/settings/prompts waste tokens.

## Choose Smallest Fix

- Refine a skill for repeatable procedure.
- Add/update a script/test for deterministic enforcement.
- Update docs only when they change future behavior.
- File an issue if the real fix is outside the PR.

Use the measured reset evidence in `docs/plan/ROADMAP.md`; do not revive
retired checker/solver guardrails as historical ceremony.

## PR Shape

Include recurring cost, evidence source, changed skill/script/doc, why compiler
behavior is unchanged, validation command, and the roadmap goal the fix
unblocks (`Goal: hold` when it protects the parity floor).
