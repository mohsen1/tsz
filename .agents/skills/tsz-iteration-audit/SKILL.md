---
name: tsz-iteration-audit
description: Audit TSZ repo iteration speed and quality. Use when asked to deep-dive repo health, identify avoidable delays, improve agent workflows, reduce CI/PR churn, add guardrails, choose skills to install or create, or turn recurring TSZ process failures into focused tooling/docs PRs.
---

# TSZ Iteration Audit

Use this skill to turn repo-health observations into small, useful changes.
The output should be evidence-backed and PR-sized, not a new roadmap unless the
durable project direction truly changes.

## Audit Loop

1. Read the current direction and coordination state:

   ```bash
   sed -n '1,320p' docs/plan/ROADMAP.md
   gh pr list --state open --limit 100 --json number,title,isDraft,headRefName,baseRefName,labels,updatedAt,url
   gh pr list --state merged --limit 30 --json number,title,headRefName,mergedAt,url
   gh issue list --state open --limit 100 --json number,title,labels,updatedAt,url
   ```

2. Inventory existing guardrails before proposing new ones:

   ```bash
   find .agents/skills .claude/skills scripts/agents scripts/ci scripts/arch scripts/bench scripts/conformance scripts/emit -maxdepth 3 -type f | sort
   ```

3. Look for recurring iteration costs:

   ```bash
   find crates scripts .agents .claude .codex docs -type f \
     \( -name '*.rs' -o -name '*.py' -o -name '*.mjs' -o -name '*.js' -o -name '*.sh' -o -name '*.md' \) \
     -not -path '*/target/*' -not -path '*/.target/*' -print0 | xargs -0 wc -l | sort -nr | sed -n '1,60p'
   rg -n "source_text\\.contains|rewrite_.*fingerprints|format_type_diagnostic|TypeData|CompatChecker|is_assignable_to" crates scripts
   rg -n "WIP|stale|drift|queue|worktree|disk|allowlist|fingerprint" docs scripts .agents .claude .codex
   ```

4. Classify each finding:

   - `workflow`: agents repeatedly start from stale state or lose coordination.
   - `guardrail`: architecture, file-size, WIP, PR-body, or CI checks miss a
     preventable class.
   - `visibility`: useful data exists but is hard to find at decision time.
   - `duplication`: the same policy lives in multiple scripts/docs/skills.
   - `behavior debt`: compiler code violates an architecture rule and needs a
     focused owner, not process cleanup.

5. Choose the smallest aligned change:

   - Create or refine a skill when the failure is procedural and repeatable.
   - Add a script or test when the failure needs deterministic enforcement.
   - Update an existing script when good tooling exists but misses one field.
   - File an issue when the fix is real but outside the current PR scope.

Use `references/2026-05-26-findings.md` as a starting evidence map. Replace it
with fresher evidence when the repo state changes.

## What Not To Do

- Do not add a new plan file for routine status.
- Do not update `docs/plan/ROADMAP.md` for ordinary cleanup or PR bookkeeping.
- Do not broaden a process PR into compiler behavior changes.
- Do not hide a behavior regression with snapshots, allowlists, or test skips.
- Do not create another checker/solver workaround when the audit finds a
  semantic bug. Route that to the owning TSZ skill and a focused issue or PR.

## PR Shape

For iteration-audit PRs, write:

- the recurring cost found,
- the evidence source,
- the specific skill/script/doc change,
- why it does not change compiler behavior,
- the validation command,
- the `AgentName`.

The best audit PRs make the next agent faster within the first five minutes of
their task.
