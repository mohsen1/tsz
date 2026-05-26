# Agent Goal: Studio-F

AgentName: Studio-F
Computer: Studio
Session: F
GitHub label: `agent:Studio-F`

## Mission

Own launch infrastructure, architecture guardrails, output-surgery burn-down,
disk/worktree reuse, TypeScript submodule reuse, Cargo-cache-preserving
cleanup, and cheap evidence plumbing for the end-state push.

## Start Every Cycle

```bash
git fetch origin main
scripts/agents/show-goal.sh Studio-F
scripts/agents/disk-preflight.sh Studio-F
scripts/agents/list-owned-work.sh Studio-F
python3 scripts/arch/arch_guard.py --json-report /tmp/tsz-arch-guard.json
python3 scripts/emit/audit-output-surgery.py --json-report /tmp/tsz-output-surgery.json
```

## Current Assignment

- Primary gate: architecture cleanup and launch mechanics that support
  conformance strictness, emit 100%, bug closure, project rows, and `2x`
  performance.
- Bug or metric families: output-surgery audit, guardrail caps, direct solver
  imports outside solver/checker, checker diagnostic/source-text ratchets,
  oversized files, disk pressure, stale worktrees, TypeScript submodule reuse,
  and scripts that make cheap evidence reliable.
- Architecture cleanup metric: every cleanup PR must ratchet a named guard
  down, remove an allowlist entry, split a file over a documented ceiling, or
  make a release-gate artifact harder to misread.
- Current known debt: `python3 scripts/emit/audit-output-surgery.py` reports
  `2` real unallowlisted semantic rewrites, `0` over-allowlist files, and
  `0` stale allowlist entries.
- First live command: run the start-cycle commands and inspect guard failures
  before choosing cleanup work.
- Next concrete step: pick one measurable guardrail or launch-script gap and
  keep it behavior-preserving unless it directly fixes a release blocker.

## Existing Work To Inspect First

- `scripts/emit/audit-output-surgery.py` and
  `scripts/emit/output-surgery-allowlist.txt`.
- `scripts/arch/arch_guard_shared.py` and `scripts/arch/arch_guard_policy.toml`.
- Tech-debt issues `#8276`, `#8278`, `#9403`, `#9447`, `#10068`, and `#10079`.
- Disk/worktree guidance in `AGENTS.md` and this directory.

## Non-Overlap Rules

- Do not run broad disk archaeology commands as routine status.
- Do not recommend `cargo clean` for ordinary cleanup.
- Do not delete worktrees unless their owner/branch/PR status is understood.
- Do not change roadmap direction for routine launch bookkeeping.
- Architecture cap bumps need a signed rationale and removal condition.

## Verification

- Test helper scripts directly with `--help`, harmless listing modes, or narrow
  unit tests.
- Use `scripts/setup/clean.sh --dry-run` before changing cleanup guidance.
- Use `python3 scripts/arch/arch_guard.py` and focused arch tests for guardrail
  edits.
- No compiler suite is needed for docs/script launch changes.
