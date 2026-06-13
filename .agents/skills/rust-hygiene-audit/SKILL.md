---
name: rust-hygiene-audit
description: Run a deep DRY + code-hygiene audit of the Rust workspace and turn the findings into verified, deduplicated, hierarchical GitHub tech-debt issues. Use this whenever the user asks to find duplication, code smells, derive/boilerplate bloat, oversized files, or "tech debt"; to tighten the Clippy/lint posture; to mass-file or triage hygiene issues; or to assign Priority/Effort issue Fields across the backlog. Reach for it even when the user only says "clean up the codebase", "find DRY violations", "what should we refactor", or "make issues for the cruft" — it is the right tool for evidence-backed, non-spammy hygiene issue creation.
---

# Rust Hygiene Audit

Turn a fuzzy "clean up the codebase" request into a set of **evidence-backed,
deduplicated, parent/child GitHub issues** — without flooding the tracker with
smells. The guiding principle: every issue must cite real `file:line` evidence
that survived an adversarial check, and must not duplicate an open issue.

This skill is for hygiene/DRY/lint work, **not** behavior changes. It produces
issues and field assignments; it does not edit compiler logic. Keep parity work
(`tsc` matching) in the owner skills.

## The four phases

Run them in order. Phases 1–2 are analysis; 3–4 are writes — confirm scope with
the user before mass-creating issues unless they already said "create them".

### 1. Measure the Clippy posture (prevention evidence)

The cheapest lever against *future* debt is a tighter lint floor, so quantify
what the current floor lets through before proposing changes.

```bash
python3 .agents/skills/rust-hygiene-audit/scripts/measure_clippy.py --worktree . --out /tmp/hygiene
```

This runs `cargo clippy --workspace --lib -- -W clippy::pedantic -W clippy::nursery`
(no files touched), aggregates warnings per lint, and prints a **promote vs
defer** triage. Read `references/lint-triage.md` for the rationale behind which
lints are worth promoting in a compiler (numeric casts and `must_use`/doc
pedantry are mostly intentional; idiom/ownership lints are high-signal). The
count and triage become the body of the clippy-posture epic.

Disk note: a cold workspace clippy build needs ~8–12GB. If low, reclaim merged
worktree caches first (see `tsz-disk-cache-hygiene`).

### 2. Fan out the audit (find + verify)

Drive the bundled workflow, which fans out per-crate DRY auditors and
cross-cutting concern auditors, dedups against open issues, then
**adversarially verifies every candidate against the real source** before
authoring full issue bodies:

```
Workflow({ scriptPath: "scripts/agents/hygiene-audit-workflow.mjs" })
```

Before launching, snapshot the open issues the auditors dedup against:

```bash
gh issue list -R <owner>/<repo> --state open --limit 300 --json number,title,labels > /tmp/hygiene/open-issues.json
```

The workflow returns `{ epics, children, dropped, stats }`. `children` carry
verified evidence and full `body_md`. `dropped` records findings that
overlapped an existing issue — surface these so the user sees the dedup worked.
Tune the auditor list (crates + concerns) in the workflow's `CRATES`/`CONCERNS`
arrays for the target repo. The verify stage defaults to **reject** on weak
evidence — that is the anti-spam gate; do not weaken it.

### 3. Create the issue hierarchy

Author epic `body_md` (summarize the theme + bake in the measured clippy data
for the lint epic), then create epics and children with native sub-issue links:

```bash
python3 .agents/skills/rust-hygiene-audit/scripts/create_issues.py \
  --repo <owner>/<repo> --specs /tmp/hygiene/result.json --apply
```

Match the repo's existing issue conventions. For tsz that is the
`techdebt(scope):` title + the template in `references/issue-template.md`
(`## Summary` with a structural rule → `## Evidence` → `## Why it matters` →
`## Proposed fix` (sized S/M/L) → `## Risks / coordination`). The script writes
a `## Child issue map` into each epic as a durable fallback even when native
linking succeeds.

### 4. Assign Priority / Effort Fields

```bash
python3 .agents/skills/rust-hygiene-audit/scripts/assign_fields.py \
  --repo <owner>/<repo> --specs /tmp/hygiene/result.json --apply
```

Discovers the repo's `Priority`/`Effort` single-select issue Fields by name,
then assigns by rule. Default priority ranking is **correctness > speed >
tech-debt** (map to High/Medium/Low; reserve Urgent for `urgent`/`panic`
labels). Effort comes from the audit `size` (S/M/L → Low/Medium/High) and
epic-scale → the top level. See `references/github-fields-api.md` for the API
gotchas — they are easy to get wrong:

- Sub-issues: `gh api -X POST .../issues/<parent>/sub_issues -F sub_issue_id=<db_id>`
  — `-F` (integer) not `-f` (string); the id is the issue **database id**, not
  its number.
- `setIssueFieldValue` works with `repo` scope, but `createIssueField` /
  `updateIssueField` (changing a field's options) need **`admin:org`**. If you
  must add an option and lack the scope, ask the user to add it in the UI or
  fall back to the existing options.

## What good output looks like

- Issues cite ≥2 real sites for any DRY claim and name the **structural rule**.
- Duplication that already drifted into a parity bug is flagged as such — that
  is the highest-value finding, not a cosmetic one.
- Nothing duplicates an open issue (the `dropped` list proves the check ran).
- A small number of strong epics, each with actionable PR-sized children.

## What to avoid

- Smell-only issues with no `file:line` evidence.
- Mass-creating before the user has agreed to issue creation.
- Architecture-boundary refactors that overlap an active campaign — check open
  issues first and let the auditors dedup.
- Fixture-name / rendered-output string checks in any proposed fix (repo
  anti-hardcoding gate).
