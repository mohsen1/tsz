# Workflow Simplification: Four Goals, Provenance, No Lanes

Date: 2026-06-10
Status: approved

## Problem

The multi-computer launch system (13 `agent:*` lane labels, 15 per-lane goal
files, a manager/reviewer lane, a 705-line ten-track roadmap, and an
`AgentName` PR-body identity) was built for a launch phase that is over.
`AgentName` is misleading: it is a made-up stable nickname that answers none of
the real provenance questions. The roadmap's track structure no longer matches
how work is picked (benchmark rows and issues).

## Decisions

1. **Four goals** replace tracks/phases as the only top-line structure:
   - **Green** — every required benchmark row compiles with the same result as
     `tsc`.
   - **Fast** — every green row is at least `2x` faster than `tsgo`.
   - **Grow** — add new real-world projects to the corpus; a new row counts
     when it reaches Green + Fast.
   - **Hold** — conformance, JS emit, DTS emit, and fourslash stay exact; no
     accepted-regression drift.
2. **Provenance replaces AgentName.** Every PR body carries a `## Provenance`
   block answering: which computer (`Machine:`), which coding assistant
   (`Assistant:` — `claude-code` or `codex`), which model (`Model:`), and what
   effort level (`Effort:` — `low|medium|high|max`). CI enforces non-empty
   values, plus a `Goal:` line naming the goal the PR serves.
3. **No manager, no reviewer lane.** The PR author lands their own PR: when
   exact-head `CI Summary` passes, the author queues with
   `gh pr merge <pr> --match-head-commit <sha>`.
4. **Land-and-continue.** Agents do not idle-wait on CI. Push, start the next
   task, return to queue the merge when CI resolves.
5. **Labels**: delete the 13 `agent:*` lanes and `codex`, `codex-automation`,
   `automated`. Keep bug-family, area, and standard labels.
6. **Docs**: `docs/plan/ROADMAP.md` rewritten around the four goals (~200
   lines). `docs/plan/agents/` deleted. Tech debt lives in issues.

## Deletions

- `docs/plan/agents/` (16 files)
- `scripts/agents/show-goal.sh`, `ensure-agent-labels.sh`,
  `list-owned-work.sh` and their tests
- `scripts/ci/pr-ownership-report.mjs` and its test (manager tooling)
- The `AgentName` / `## Project Corpus Impact` requirements in the CI PR-body
  gate (job renamed `pr-body-gate`, now checks `Goal:` + Provenance fields)

## Kept

- `scripts/ci/check-pr-ready-state.mjs` (WIP/blocker detection), with the
  signed-comment convention now satisfied by a `Machine:` or provenance line
- `scripts/agents/disk-preflight.sh`, `llm-context-audit.py`
- Plan appendices: `PERFORMANCE_PLAN.md`, `LSP_ROADMAP.md`, `SOUND_MODE.md`
- All bug-family and area labels

## Post-merge follow-ups (gh, not in-repo)

1. Delete the 16 labels.
2. Rewrite the open PR bodies (6 at design time) to the new format so the
   flipped gate passes on them.
