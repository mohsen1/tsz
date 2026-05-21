# Multi-Agent Launch Plan

Status: launch-control plan for the M1, M4, and Studio Codex sessions. This
directory is intentionally not a replacement for `docs/plan/ROADMAP.md`.
Durable project direction, public metrics, and release gates stay in the
roadmap; these files turn that direction into editable per-session goals.

Snapshot date: 2026-05-21 follow-up. The original launch baseline was
2026-05-20.

## Current Shape

The active roadmap says the phase boundary is project compatibility first:
match `tsc` results on real projects, then tune speed once rows are green or
the first blocker is runtime, OOM, timeout, or residency.

Current GitHub state after the first runway-drain pass:

| Surface | Count |
| --- | ---: |
| Open PRs | 59 |
| Draft PRs | 50 |
| Ready PRs without `WIP` | 9 |
| PRs with `WIP` label | 0 |
| Stacked PR children | 3 |
| Open issues | 117 |
| Open issues with `WIP` | 2 |
| Open urgent issues | 3 |
| Open benchmark/performance issues | 20 |
| Open solver issues | 21 |
| Open checker issues | 25 |
| Open emitter/emit issues | 20 |
| Open LSP issues | 12 |
| Open tech-debt issues | 77 |

Ready PRs are now mostly blocked on CI/review state, not `WIP` labels. Drain
them by fixing the current blocker or letting enabled auto-merge complete; do
not repeatedly re-arm auto-merge without resolving the blocker.

## Agent Labels

Each session owns work through exactly one GitHub label:

| Computer | Sessions |
| --- | --- |
| M1 | `agent:M1-A`, `agent:M1-B`, `agent:M1-C`, `agent:M1-D` |
| M4 | `agent:M4-A`, `agent:M4-B`, `agent:M4-C`, `agent:M4-D` |
| Studio | `agent:Studio-A`, `agent:Studio-B`, `agent:Studio-C`, `agent:Studio-D`, `agent:Studio-E`, `agent:Studio-F` |
| Always-on reviewer | `agent:Reviewer` |

Claude Code and other runner-backed agents may do work in any of these lanes.
Their generated names, for example `claude-sonnet-*`, `dreamy-*`, or
machine/model aliases, are contributor identity only. They are not ownership
labels and must not be turned into new `agent:*` lanes.

Rules:

1. For the initial launch, apply `agent:*` labels to PRs only. Do not label
   issues yet; issues are context until the open PR runway is drained.
2. A labelled PR may have at most one `agent:*` label.
3. The label means "owns the next concrete step", not permanent subsystem
   ownership.
4. Use only the canonical labels in the table above. If a PR has a generated
   runner label such as `agent:claude-sonnet-*`, `agent:dreamy-*`, or a typo
   such as `agnet:*`, replace it with the correct lane before marking the PR
   ready or enabling auto-merge.
5. If a Claude Code session was launched without a lane assignment, it may sign
   comments with its runner-generated name, but it should not claim ownership
   with a new `agent:*` label. A maintainer or coordinator should assign one of
   the canonical lanes first.
6. If a session pauses or abandons work, it comments with `AgentName:`, current
   findings, next action, then removes its `agent:*` label.
7. Every PR body and substantive PR comment includes the same `AgentName`.
8. Draft PRs are coordination state. Do not merge anything draft, labelled
   `WIP`, titled `[WIP]`, or described as blocked/not ready.

Run the label audit before large PR-garden passes:

```bash
scripts/agents/ensure-agent-labels.sh --audit
```

## Source-Of-Truth Goal Loop

Each Codex `/goal` should read its own file from repo source at the start of
each work cycle. Prefer reading `origin/main` so goals can be updated remotely
without forcing the agent to merge into its feature branch:

```bash
git fetch origin main
scripts/agents/show-goal.sh M1-A
```

The session then runs the rest of that goal file's `Start Every Cycle`
commands before starting work. Today every session checks disk and owned work
with `disk-preflight` and `list-owned-work` after refreshing the repo-owned
goal file.

## Worktree And TypeScript Submodule Policy

The TypeScript corpus is slow to populate. Worktree reuse is therefore part of
the operating model, not a convenience.

Before making or switching worktrees, run:

```bash
scripts/agents/disk-preflight.sh <AgentName>
git worktree list
```

Rules:

1. Reuse an existing sister worktree whenever it is inactive and has the right
   cache shape, especially if `TypeScript/`, `.target/`, or `target/` is
   already populated.
2. Do not create a new worktree when the disk guard reports low disk. Reuse or
   clean first.
3. New worktrees go beside the repo under `/Users/mohsen/code`, never nested
   inside the primary checkout.
4. Initialize the real `TypeScript/` submodule once in the primary checkout.
   In sibling worktrees, prefer `scripts/setup/link-ts-submodule.sh` so
   `TypeScript/` is a symlink to the primary checkout's populated submodule.
   If another worktree has the populated corpus instead, pass
   `--source <repo-or-TypeScript-dir>`.
5. Do not mutate the `TypeScript/` submodule. It is read-only test data.

Recommended new-worktree path:

```bash
git worktree add ../tsz-<agent>-<short-scope> -b <branch> origin/main
cd ../tsz-<agent>-<short-scope>
scripts/setup/link-ts-submodule.sh
```

## Cargo And Disk Hygiene

Do not use `cargo clean` as routine hygiene. It destroys useful compile state
and slows every session down.

Preferred cleanup ladder:

1. Run `scripts/setup/disk-worktree-guard.sh`.
2. If disk is low, run `scripts/setup/disk-worktree-guard.sh --auto-prune`.
3. Run `scripts/setup/clean.sh --quiet`.
4. Only after confirming a worktree is abandoned, delete that worktree.
5. Use `scripts/setup/clean.sh --full` only as a deliberate last resort on an
   abandoned worktree or after confirming the cache loss is acceptable.

`scripts/setup/clean.sh` without `--full` preserves `.target/`,
`.target-bench/`, and `target/`, while pruning stale Cargo incremental
directories older than seven days. That is the default safe path for avoiding a
full disk without throwing away build caches.

## Lane Assignments

Initial priority for every implementation lane is to land, close, or clearly
handoff existing PRs in that lane before claiming issue backlog. Issue numbers
inside the per-agent files are context only until the PR runway is under
control.

| Agent | Track | Initial focus |
| --- | --- | --- |
| `M1-A` | Coordination | PR garden, ready/WIP cleanup, ownership label hygiene |
| `M1-B` | Tracks 4 and 10 | Checker relation gateway and `RelationRequest` migration |
| `M1-C` | Tracks 8 and 10 | Rendered-type/source-text decision burn-down |
| `M1-D` | Track 6 | Narrowing and flow predicate parity |
| `M4-A` | Track 2 | Recursive conditional/mapped evaluation identity |
| `M4-B` | Tracks 3, 4, and 10 | Relation policy/cache-key stack consolidation |
| `M4-C` | Track 3 | Generic inference, contextual typing, constructor inference |
| `M4-D` | Track 7 | Symbol, lib, module, and cross-file identity |
| `Studio-A` | Track 1 | Project corpus dashboard and fixture truth |
| `Studio-B` | Tracks 2 and 10 | Project-row performance, ts-toolbelt/type-fest residency |
| `Studio-C` | Track 9 | JavaScript emit failure-family recovery |
| `Studio-D` | Track 9 | DTS failure-family recovery and declaration summary direction |
| `Studio-E` | LSP companion, Track 9 | Low-bandwidth LSP/WASM smoke and hover work |
| `Studio-F` | Track 10 | Disk/worktree hygiene, launch scripts, stalled-CI/runway work |
| `Reviewer` | Review | High-level PR review, architecture/parity risk comments, waits for new PRs |

Each file in this directory expands the lane with concrete PRs and issues to
inspect, non-overlap notes, and a launch checklist.

## Launch Checklist

1. Merge this coordination PR or explicitly tell sessions to read this branch.
2. Run `scripts/agents/ensure-agent-labels.sh` and
   `scripts/agents/ensure-agent-labels.sh --audit`.
3. For each session, run `scripts/agents/disk-preflight.sh <AgentName>`.
4. Give each Codex session the `/goal` prompt from
   `docs/plan/agents/LAUNCH.md`.
5. Each session labels the existing PR it is landing with its `agent:*` label
   before writing code. Do not apply `agent:*` labels to issues yet.
6. Each session opens or updates a draft PR early, then keeps the PR body
   current with root cause, scope changes, verification, and handoff notes.
7. Launch `Reviewer` as a standing `/goal` session when review bandwidth is
   available. Its goal intentionally does not complete; it reviews open PRs,
   posts signed high-level comments, and waits for new PRs when the queue is
   empty.
