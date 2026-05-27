# Multi-Agent Launch Plan

Status: launch-control plan for M1, M4, Studio, and Reviewer Codex sessions.
This directory is not a replacement for `docs/plan/ROADMAP.md`; it turns the
roadmap's release gates into editable per-session goals.

Snapshot date: 2026-05-26. The next launch assumes the old PR runway is closed
or explicitly handed off before sessions start. Treat all counts below as
orientation only; live GitHub, checked-in artifacts, and CI outputs are the
source of truth.

## Next Launch Goal

The next multi-agent launch is an end-state push:

1. Diagnostic conformance stays at `100%`, and the accepted-regression
   strictness list goes to zero or every remaining entry has fresh CI evidence
   and an owner.
2. JavaScript emit reaches `100%` against TypeScript baselines.
3. Declaration emit reaches `100%` against TypeScript baselines.
4. Open bugs are fixed, closed as duplicates/superseded with evidence, or
   clustered behind an active owning PR that states the structural rule.
5. Required project rows are green against `tsc`; green timed rows are at least
   `2x` faster than `tsgo` in timing mode.
6. Architecture cleanup advances the gates above by reducing measured boundary
   debt: query-boundary quarantine, accepted-regression allowlists,
   rendered/source-text diagnostic decisions, emit reach-through, output
   surgery, oversized modules, cache-key ambiguity, and guardrail caps.

## Current Orientation

Recent local/live evidence on 2026-05-26:

| Surface | Current read |
| --- | ---: |
| Open PRs | live query: `gh pr list --state open` |
| Draft/WIP PRs | live query: `gh pr list --state open` plus WIP labels/titles |
| PR label hygiene | clean |
| Diagnostic conformance detail | `12,582 / 12,582` |
| Accepted-regression list | live query: `python3 scripts/conformance/query-conformance.py --dashboard` |
| JavaScript emit snapshot | `13,094 / 13,530` |
| Declaration emit snapshot | `1,606 / 1,669` |
| Open issues | live query: `gh issue list --state open` |
| Open bug issues | live query: `gh issue list --state open --label bug` |
| Open performance issues | live query: `gh issue list --state open --label performance` |
| Open tech-debt issues | live query: `gh issue list --state open --label tech-debt` |
| Output-surgery audit | live query: `python3 scripts/emit/audit-output-surgery.py` |

Do not copy these numbers into PR bodies as proof. Re-run the commands in the
owning lane and cite the resulting artifact, issue, or CI URL.

## Agent Labels

Each session owns work through exactly one GitHub label:

| Computer | Sessions |
| --- | --- |
| M1 | `agent:M1-A`, `agent:M1-B`, `agent:M1-C`, `agent:M1-D` |
| M4 | `agent:M4-A`, `agent:M4-B`, `agent:M4-C`, `agent:M4-D` |
| Studio | `agent:Studio-A`, `agent:Studio-B`, `agent:Studio-C`, `agent:Studio-D`, `agent:Studio-E`, `agent:Studio-F` |
| Always-on reviewer | `agent:Reviewer` |

Generated runner names such as `claude-*`, `dreamy-*`, machine aliases, or
model aliases are contributor identity only. They are not ownership labels.

Rules:

1. A labelled PR has at most one `agent:*` owner.
2. The label means "owns the next concrete step", not permanent subsystem
   ownership.
3. Use only the canonical labels above. Replace generated runner labels or
   `agnet:*` typos before marking work ready.
4. Every PR body and substantive PR comment includes `AgentName`.
5. Draft PRs are coordination state. Do not merge work that is draft, labelled
   `WIP`, titled `[WIP]`, or described as blocked/not ready.
6. If no open PR runway remains, issues may be used as intake context, but
   durable ownership should still become an early draft PR with a real body.
7. If a session pauses or abandons work, leave a signed comment with findings,
   blocker or reason, verification already run, and next owner/action.

## Live Intake Rule

Every implementation lane starts with its live PRs:

1. Run the lane's `Start Every Cycle` commands.
2. If open PRs carry the lane label, complete, mark ready, close as
   duplicate/superseded with evidence, or hand off with a signed comment before
   starting new issue work.
3. If no lane PRs are open, choose the next issue or metric row from that
   lane's current assignment. Cluster by structural invariant rather than
   starting one branch per issue.
4. Open or update a draft PR early. The PR body is the live coordination state.
5. Keep issue labels, PR labels, and PR body `AgentName` aligned.

Useful live checks:

```bash
scripts/agents/ensure-agent-labels.sh --audit
scripts/agents/list-owned-work.sh --all
scripts/agents/list-owned-work.sh --pr-state Studio-F
node scripts/ci/pr-ownership-report.mjs
gh issue list --repo mohsen1/tsz --state open --limit 200 --json number,title,labels,updatedAt,url
```

## Source-Of-Truth Goal Loop

Each `/goal` reads its own file from repo source at the start of each work
cycle. Prefer reading `origin/main` so sessions can be redirected without
merging main into an in-progress feature branch:

```bash
git fetch origin main
scripts/agents/show-goal.sh M1-A
```

When reviewing or developing a branch that edits a lane goal file, use
`scripts/agents/show-goal.sh <AgentName> --local` to preview the branch-local
file. The default command still prefers `origin/main` so launch sessions can be
redirected without first merging the in-progress branch. If the branch-local
file differs from the printed `origin/main` goal, `show-goal.sh` warns on
stderr; treat that as a cue to inspect `--local` before acting on branch-local
coordination.

Then run the remaining commands listed in that lane's `Start Every Cycle`
section.

## GitHub Actions Outages

When GitHub Actions is unavailable or checkout/action-download failures are
clearly infrastructure-wide, do not rerun jobs as a watcher and do not add
`merge-queue`. Keep the lane moving with local, cheap evidence:

1. Confirm the branch is clean and synced with `origin/main`.
2. Run the lane's local guardrail commands and any narrow script tests that
   answer the PR's risk.
3. Leave a signed PR comment naming the external blocker, the exact head SHA,
   the local verification, and the next action after Actions recovers.

Resume CI only after the external outage clears, and re-check the exact head
before changing draft/ready state or adding `merge-queue`. `Queue Tested` is
produced after the label is added, so it is not evidence to wait on before
enqueue.

## Worktree And Cache Policy

Before making or switching worktrees, run:

```bash
scripts/agents/disk-preflight.sh <AgentName>
git worktree list
```

Rules:

1. Reuse an existing sister worktree whenever it is inactive and has useful
   `TypeScript/`, `.target/`, or `target/` state.
2. Do not create a new worktree when the disk guard reports low disk. Reuse or
   clean first.
3. New worktrees go beside the repo under `/Users/mohsen/code`, never nested
   inside the primary checkout.
4. In sibling worktrees, prefer `scripts/setup/link-ts-submodule.sh` so
   `TypeScript/` is shared from a populated checkout.
5. Do not mutate the `TypeScript/` submodule. It is read-only test data.
6. Do not use `cargo clean` for routine hygiene. Prefer
   `scripts/setup/disk-worktree-guard.sh --auto-prune` and
   `scripts/setup/clean.sh --quiet`.

## Lane Assignments

| Agent | Track | Next-launch focus |
| --- | --- | --- |
| `M1-A` | Release control | Live ownership hygiene, release-gate scoreboard, duplicate work prevention |
| `M1-B` | Tracks 4, 10 | Checker relation diagnostics through `RelationRequest` and query-boundary gateways |
| `M1-C` | Tracks 8, 10 | Conformance strictness, accepted-regression burn-down, rendered/source-text diagnostic debt |
| `M1-D` | Track 6 | Flow graph and solver-owned narrowing predicates |
| `M4-A` | Track 2 | Recursive conditional, mapped, template, `infer`, indexed-access evaluation |
| `M4-B` | Tracks 3, 4, 10 | Relation policy, variance, compatibility exceptions, relation/cache contracts |
| `M4-C` | Track 3 | Inference sessions, contextual typing, overloads, constructor/generic instantiation |
| `M4-D` | Track 7 | Symbol, lib, module, `DefId`, and cross-file stable identity |
| `Studio-A` | Track 1 | Project corpus and release metric truth across conformance, emit, bugs, and perf |
| `Studio-B` | Track 10 | Green-row performance and residency until every timed row is `2x` faster than `tsgo` |
| `Studio-C` | Track 9 | JavaScript emit 100% by transform family, starting with largest JS buckets |
| `Studio-D` | Track 9 | Declaration emit 100% through declaration/public API summary boundaries |
| `Studio-E` | Track 9, LSP appendix | JSDoc/JS declaration emit and LSP/WASM compiler-service consumer boundaries |
| `Studio-F` | Track 10 | Launch infrastructure, architecture guardrails, output-surgery and disk/worktree hygiene |
| `Reviewer` | Review | High-level review of parity, architecture, metrics, duplicate ownership, and readiness |

Architecture cleanup is not a separate permission slip for broad refactors.
Every cleanup PR must name the release gate it supports and the metric it
ratchets down.

## Architecture Cleanup Ratchet

Cleanup lanes support release gates by making boundary debt measurable and
smaller:

| Debt Category | Owner | Gate Supported | Counter Or Command |
| --- | --- | --- | --- |
| Ownership and duplicate-work hygiene | `M1-A` | all gates | `node scripts/ci/pr-ownership-report.mjs`; `scripts/agents/ensure-agent-labels.sh --audit` |
| Checker relation gateway debt | `M1-B` | bug closure, conformance strictness | `scripts/arch/check-checker-boundaries.sh`; `python3 scripts/arch/arch_guard.py` |
| Accepted-regression and diagnostic hardcoding debt | `M1-C` | conformance strictness | `python3 scripts/conformance/query-conformance.py --dashboard`; accepted-regression entry count |
| Flow/narrowing boundary debt | `M1-D` | bug closure, project rows | focused checker/solver tests plus `python3 scripts/arch/arch_guard.py` |
| Solver evaluation substrate debt | `M4-A` | bug closure, project rows, conformance strictness | focused solver tests; oversized helper issues such as evaluation shard splits |
| Relation policy/cache-key debt | `M4-B` | bug closure, conformance strictness, perf correctness | cache-on/cache-off targeted tests; relation policy guardrails |
| Inference-session transaction debt | `M4-C` | bug closure, project rows | repeated-call/order tests; inference cache/session tests |
| Stable identity/name allowlist debt | `M4-D` | bug closure, project rows, DTS | identity-focused checker/binder tests; well-known-name inventory updates |
| Project row and metric drift | `Studio-A` | project rows, public metrics | `node scripts/bench/project-row-summary.mjs --markdown`; `node scripts/bench/validate-project-metadata.mjs` |
| Residency and cache visibility debt | `Studio-B` | `2x` perf target | `scripts/bench/perf-hotspots.sh --quick`; `scripts/bench/tsgo-winner-report.mjs <bench.json> <out.json>` |
| JS emit transform debt | `Studio-C` | JS emit 100% | `python3 scripts/emit/query-emit.py --families`; targeted `scripts/emit/run.sh` filters |
| DTS summary/reach-through debt | `Studio-D` | DTS emit 100% | `python3 scripts/emit/query-emit.py --dts-failures --top 25` |
| JSDoc declaration and consumer boundary debt | `Studio-E` | DTS emit 100%, LSP/WASM stability | narrow DTS/LSP/WASM tests |
| Guardrail, output-surgery, and launch-script debt | `Studio-F` | all gates | `python3 scripts/arch/arch_guard.py`; `python3 scripts/emit/audit-output-surgery.py` |
| Review enforcement | `Reviewer` | all gates | signed PR review findings and readiness checks |

## Launch Checklist

1. Merge this coordination update or tell sessions to read this branch.
2. Confirm live PR runway state with `node scripts/ci/pr-ownership-report.mjs`.
3. Confirm labels with `scripts/agents/ensure-agent-labels.sh --audit`.
4. Confirm cheap release metrics with the owning lane commands:
   conformance dashboard, emit families, project row summary, and architecture
   guard.
5. For each session, run `scripts/agents/disk-preflight.sh <AgentName>`.
6. Start each Codex session with the prompt from `docs/plan/agents/LAUNCH.md`.
7. Each session opens or updates a draft PR early and keeps the PR body current
   with root cause, scope changes, verification, and handoff notes.
8. Reviewer stays ongoing. It reviews changed PRs and waits when the queue is
   empty; its goal is not completed merely because no PR is currently
   reviewable.
