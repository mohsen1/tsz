# Multi-Agent Launch Plan

Status: launch-control plan for M1, M4, Studio, Opus, and Studio-manager
Codex sessions. This directory is not a replacement for
`docs/plan/ROADMAP.md`; it turns the roadmap's release gates into editable
per-session `/goal` prompts.

Snapshot date: 2026-06-03. Treat all counts below as orientation only. Live
GitHub state, checked-in artifacts, benchmark outputs, and CI results are the
source of truth.

## Overall Project Goal

The launch succeeds only when the work has landed on `main` and the repository
can prove all of these gates:

1. All tests required by the release gate pass.
2. All benchmark/project rows that TypeScript accepts are green for `tsz`.
3. Every eligible green timed benchmark row is at least `2x` faster than
   `tsgo` in the canonical timing artifact.
4. JavaScript emit and declaration emit match TypeScript baselines.
5. Diagnostic conformance stays at `100%`, and accepted regressions are empty
   or have fresh, owned evidence.
6. Open bugs and release-gate issues are fixed, closed as
   duplicate/superseded/upstream/non-release with evidence, or clustered behind
   an active owning PR.
7. Tech debt that blocks those gates is reduced with measurable counters,
   architecture guards, or explicit debt burn-down artifacts.

Every lane keeps working until its changes are landed in `main`, not merely
until a branch exists or a draft PR is open.

## Agent Labels

Each session owns work through exactly one GitHub label:

| Group | Sessions |
| --- | --- |
| M1 | `agent:M1-A`, `agent:M1-B`, `agent:M1-D`, `agent:M1-Opus` |
| M4 | `agent:M4-A`, `agent:M4-B`, `agent:M4-C`, `agent:M4-Opus` |
| Studio | `agent:Studio-A`, `agent:Studio-B`, `agent:Studio-C`, `agent:Studio-Opus` |
| PR manager/reviewer | `agent:Studio-manager` |

Generated runner names, computer aliases, model aliases, or branch nicknames
are contributor identity only. They are not ownership labels.

Rules:

1. A labelled PR has at most one `agent:*` owner.
2. The label means "owns the next concrete step", not permanent subsystem
   ownership.
3. Use only the canonical labels above. Replace generated runner labels or
   `agnet:*` typos before marking work ready.
4. Every PR body and substantive PR comment includes `AgentName`.
5. Draft PRs are active runway, not storage. Do not merge work that is draft,
   labelled `WIP`, titled `[WIP]`, or described as blocked/not ready.
6. A session drains owned PRs before opening unrelated new PRs. Valid runway
   outcomes are: landed on `main`, native merge queue, ready with verified PR-head
   checks, refreshed draft/WIP with a signed blocker, signed handoff, or
   evidence-linked duplicate/superseded closure.
7. Keep draft runway small: at most two unstacked draft PRs per `agent:*`
   owner unless the extras are intentional stack children or carry fresh signed
   blocker comments.
8. If no open PR runway remains, issues may be used as intake context, but
   durable ownership should still become an early draft PR with a real body.
9. If a session pauses or abandons work, leave a signed comment with findings,
   blocker or reason, verification already run, and next owner/action.

Comment budget:

- Use the PR body as the routine state surface: scope, current blocker,
  verification, Project Corpus Impact, and Coordination Notes.
- Do not leave heartbeat comments for "checking", "waiting", "still running",
  or unchanged CI/queue state. The ownership report and GitHub checks already
  carry that state.
- Leave at most one signed PR conversation comment for a state transition:
  draft/WIP change, blocker, handoff/takeover, closure/superseded evidence,
  queue failure root cause, or readiness risk.
- Prefer submitted review comments for code findings. Use a PR conversation
  comment only for cross-file scope, metadata, queue, blocker, or handoff
  decisions.

## Live Intake Rule

Every lane starts with its live PRs. Owned PRs are the work queue; issues are
intake only after that queue is drained, queued, or explicitly blocked.

1. Run the lane's `Start Every Cycle` commands.
2. If open PRs carry the lane label, inspect each one and move it to the next
   concrete state before starting new issue work: fix/rebase it, mark it ready,
   queue it with `gh pr merge <pr> --match-head-commit <sha>`, restore
   draft/WIP with a signed blocker, hand it off, or close it only as
   duplicate/superseded with evidence.
3. If an owned ready `main` PR has passing PR-head `CI Summary`, is not
   dirty/conflicting, and is not WIP or blocked, queue it with
   `gh pr merge <pr> --match-head-commit <sha>` or ask `Studio-manager` to
   queue it.
4. Treat stale drafts as live debt. Drafts older than 24 hours without fresh
   commits/comments, and owners over two unstacked drafts, must be refreshed,
   handed off, marked help-wanted, or documented as blocked before new PRs.
5. If no lane PRs are open or actionable, choose the next issue or metric row
   from that lane's current assignment. Cluster by structural invariant rather
   than starting one branch per issue.
6. Open or update a draft PR early. The PR body is the live coordination state.
7. Keep issue labels, PR labels, and PR body `AgentName` aligned.

Useful live checks:

```bash
scripts/agents/ensure-agent-labels.sh --audit --json-report /tmp/tsz-agent-label-audit.json
scripts/agents/list-owned-work.sh --all
scripts/agents/list-owned-work.sh --pr-state Studio-manager
node scripts/ci/pr-ownership-report.mjs
node scripts/ci/pr-ownership-report.mjs --json /tmp/tsz-pr-ownership.json
gh issue list --repo tsz-org/tsz --state open --limit 200 --json number,title,labels,updatedAt,url
```

The `Manager Next Actions` section from `node scripts/ci/pr-ownership-report.mjs`
is the default triage queue. Follow it before posting new PR comments; when an
entry says `comment: none`, act through queueing, CI inspection, rebasing, or
PR-body updates instead.

## Source-Of-Truth Goal Loop

Each `/goal` reads its own file from repo source at the start of each work
cycle. Prefer reading `origin/main` so sessions can be redirected without
merging main into an in-progress feature branch:

```bash
git fetch origin main
scripts/agents/show-goal.sh M1-A
```

When developing a branch that edits a lane goal file, use
`scripts/agents/show-goal.sh <AgentName> --local` to preview the branch-local
file. The default command still prefers `origin/main`.

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
| `M1-A` | Checker diagnostics | Diagnostic conformance, accepted-regression burn-down, rendered/source-text diagnostic debt |
| `M1-B` | Checker orchestration | Relation diagnostic routing, flow/narrowing handoff, query-boundary cleanup |
| `M1-D` | Flow narrowing | Solver-owned narrowing predicates, flow graph parity, and Kysely/Zod guard reductions |
| `M1-Opus` | M1 deep debt | Cross-cutting checker architecture debt that blocks tests, bugs, project rows, or conformance strictness |
| `M4-A` | Solver evaluation | Recursive conditional, mapped, template, `infer`, indexed-access, and key-space evaluation |
| `M4-B` | Solver relations | Relation policy, inference/session state, stable identity, variance, and cache contracts |
| `M4-C` | Inference sessions | Contextual typing, overloads, constructors, and instantiation-state project blockers |
| `M4-Opus` | M4 deep debt | Solver substrate rewrites and cache/identity architecture needed for parity plus speed |
| `Studio-A` | Project corpus | Release metric truth, project-row green status, benchmark artifact validity, bug intake routing |
| `Studio-B` | Performance | Green-row residency and timing until every eligible row is at least `2x` faster than `tsgo` |
| `Studio-C` | Emit | JavaScript emit and declaration emit parity, output-surgery burn-down, emit boundary cleanup |
| `Studio-Opus` | Studio deep debt | Cross-cutting Studio infrastructure, project corpus, emit/perf blockers, LSP/WASM/compiler-service boundaries |
| `Studio-manager` | PR management/review | PR queue management, label hygiene, submitted reviews, merge readiness, duplicate-work prevention |

Architecture cleanup is not a separate permission slip for broad refactors.
Every cleanup PR must name the release gate it supports and the metric,
counter, guard, or allowlist it ratchets down.

## Architecture Cleanup Ratchet

| Debt Category | Owner | Gate Supported | Counter Or Command |
| --- | --- | --- | --- |
| Diagnostic hardcoding and accepted-regression debt | `M1-A` | conformance strictness, bug closure | `python3 scripts/conformance/query-conformance.py --dashboard`; accepted-regression entry count |
| Checker relation and query-boundary debt | `M1-B` | bug closure, conformance strictness, project rows | `scripts/arch/check-checker-boundaries.sh`; `python3 scripts/arch/arch_guard.py` |
| Checker cross-cutting architecture debt | `M1-Opus` | all checker-facing gates | boundary guard counts, oversized checker module counts, accepted-regression burn-down |
| Solver evaluation substrate debt | `M4-A` | bug closure, project rows, conformance strictness | focused solver tests; accepted-regression and issue-cluster reductions |
| Relation policy/inference/cache-key debt | `M4-B` | bug closure, conformance strictness, perf correctness | cache-on/cache-off targeted tests; relation policy guardrails |
| Solver stable identity and cache architecture debt | `M4-Opus` | project rows, `2x` perf target, bug closure | cache-key contracts, residency counters, stable identity tests |
| Project row and metric drift | `Studio-A` | project rows, public metrics | `node scripts/bench/project-row-summary.mjs --markdown`; benchmark/guard artifacts |
| Residency and cache visibility debt | `Studio-B` | `2x` perf target | `scripts/bench/perf-hotspots.sh --quick`; `scripts/bench/tsgo-winner-report.mjs <bench.json> <out.json>` |
| JS/DTS emit and output-surgery debt | `Studio-C` | emit 100%, DTS 100% | `python3 scripts/emit/query-emit.py --families`; `python3 scripts/emit/audit-output-surgery.py` |
| Studio infrastructure and consumer boundary debt | `Studio-Opus` | all Studio gates | project-row metadata validation, LSP/WASM/compiler-service tests, output-surgery audit |
| PR queue, label, and review debt | `Studio-manager` | all gates | `node scripts/ci/pr-ownership-report.mjs`; signed submitted reviews; label audit |

## Launch Checklist

1. Merge this coordination update or tell sessions to read this branch with
   `scripts/agents/show-goal.sh <AgentName> --local`.
2. Confirm live PR runway state, draft parking risks, and queue candidates with
   `node scripts/ci/pr-ownership-report.mjs`.
3. Confirm labels with
   `scripts/agents/ensure-agent-labels.sh --audit --json-report /tmp/tsz-agent-label-audit.json`.
4. Confirm cheap release metrics with the owning lane commands.
5. Start each lane with the matching prompt from `docs/plan/agents/LAUNCH.md`.
6. `Studio-manager` stays ongoing. It manages PRs, submits reviews, and waits
   when no PR needs action.
