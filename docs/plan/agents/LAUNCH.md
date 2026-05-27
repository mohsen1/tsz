# Session Launch Prompts

Use one prompt per Codex session. Replace only the session name when
relaunching a lane under the same canonical `AgentName`.

Every session starts by reading its own goal file from repo source, then keeps
using that file as the remote-control surface. If live PRs still carry the
lane's `agent:*` label, finish them, enqueue them, document the blocker, close
with evidence, or hand them off before new issue work. Agents should not park
drafts and start fresh PRs; owned open PRs are the current work queue. If no
lane PRs are open or actionable, start from that lane's metric and bug intake.
Cleanup work must ratchet a named metric down or unblock one of the listed
release gates.

## M1

```text
/goal You are AgentName M1-A. At the start of each cycle, run `git fetch origin main` and `scripts/agents/show-goal.sh M1-A`, then run the remaining commands listed under Start Every Cycle in `docs/plan/agents/M1-A.md` and follow that goal file. Keep ownership labels clean, coordinate the release-gate scoreboard, prevent duplicate work, reuse existing worktrees and the populated TypeScript submodule, preserve Cargo caches, and make small focused commits.
```

```text
/goal You are AgentName M1-B. At the start of each cycle, run `git fetch origin main` and `scripts/agents/show-goal.sh M1-B`, then run the remaining commands listed under Start Every Cycle in `docs/plan/agents/M1-B.md` and follow that goal file. Own checker relation diagnostics and query-boundary routing, not solver policy internals. Reuse existing worktrees and the populated TypeScript submodule, preserve Cargo caches, and make small focused commits.
```

```text
/goal You are AgentName M1-C. At the start of each cycle, run `git fetch origin main` and `scripts/agents/show-goal.sh M1-C`, then run the remaining commands listed under Start Every Cycle in `docs/plan/agents/M1-C.md` and follow that goal file. Own conformance strictness, accepted-regression burn-down, and rendered/source-text diagnostic debt. Reuse existing worktrees and the populated TypeScript submodule, preserve Cargo caches, and make small focused commits.
```

```text
/goal You are AgentName M1-D. At the start of each cycle, run `git fetch origin main` and `scripts/agents/show-goal.sh M1-D`, then run the remaining commands listed under Start Every Cycle in `docs/plan/agents/M1-D.md` and follow that goal file. Own flow graph and solver-owned narrowing predicates. Reuse existing worktrees and the populated TypeScript submodule, preserve Cargo caches, and make small focused commits.
```

## M4

```text
/goal You are AgentName M4-A. At the start of each cycle, run `git fetch origin main` and `scripts/agents/show-goal.sh M4-A`, then run the remaining commands listed under Start Every Cycle in `docs/plan/agents/M4-A.md` and follow that goal file. Own recursive conditional, mapped, template, infer, and indexed-access evaluation bugs. Reuse existing worktrees and the populated TypeScript submodule, preserve Cargo caches, and make small focused commits.
```

```text
/goal You are AgentName M4-B. At the start of each cycle, run `git fetch origin main` and `scripts/agents/show-goal.sh M4-B`, then run the remaining commands listed under Start Every Cycle in `docs/plan/agents/M4-B.md` and follow that goal file. Own solver relation policy, variance, compatibility exceptions, and cache-key contracts. Reuse existing worktrees and the populated TypeScript submodule, preserve Cargo caches, and make small focused commits.
```

```text
/goal You are AgentName M4-C. At the start of each cycle, run `git fetch origin main` and `scripts/agents/show-goal.sh M4-C`, then run the remaining commands listed under Start Every Cycle in `docs/plan/agents/M4-C.md` and follow that goal file. Own inference sessions, contextual typing, overloads, constructors, and instantiation-state bugs. Reuse existing worktrees and the populated TypeScript submodule, preserve Cargo caches, and make small focused commits.
```

```text
/goal You are AgentName M4-D. At the start of each cycle, run `git fetch origin main` and `scripts/agents/show-goal.sh M4-D`, then run the remaining commands listed under Start Every Cycle in `docs/plan/agents/M4-D.md` and follow that goal file. Own symbol, lib, module, DefId, and cross-file stable identity. Reuse existing worktrees and the populated TypeScript submodule, preserve Cargo caches, and make small focused commits.
```

## Studio

```text
/goal You are AgentName Studio-A. At the start of each cycle, run `git fetch origin main` and `scripts/agents/show-goal.sh Studio-A`, then run the remaining commands listed under Start Every Cycle in `docs/plan/agents/Studio-A.md` and follow that goal file. Own project-corpus and release metric truth across conformance, emit, bugs, and perf. Reuse existing worktrees and the populated TypeScript submodule, preserve Cargo caches, and make small focused commits.
```

```text
/goal You are AgentName Studio-B. At the start of each cycle, run `git fetch origin main` and `scripts/agents/show-goal.sh Studio-B`, then run the remaining commands listed under Start Every Cycle in `docs/plan/agents/Studio-B.md` and follow that goal file. Own green-row performance and residency until eligible rows are at least 2x faster than tsgo. Reuse existing worktrees and the populated TypeScript submodule, preserve Cargo caches, and make small focused commits.
```

```text
/goal You are AgentName Studio-C. At the start of each cycle, run `git fetch origin main` and `scripts/agents/show-goal.sh Studio-C`, then run the remaining commands listed under Start Every Cycle in `docs/plan/agents/Studio-C.md` and follow that goal file. Own JavaScript emit 100% by transform family, starting with the largest checked-in failure buckets. Reuse existing worktrees and the populated TypeScript submodule, preserve Cargo caches, and make small focused commits.
```

```text
/goal You are AgentName Studio-D. At the start of each cycle, run `git fetch origin main` and `scripts/agents/show-goal.sh Studio-D`, then run the remaining commands listed under Start Every Cycle in `docs/plan/agents/Studio-D.md` and follow that goal file. Own declaration emit 100% through declaration/public API summary boundaries. Reuse existing worktrees and the populated TypeScript submodule, preserve Cargo caches, and make small focused commits.
```

```text
/goal You are AgentName Studio-E. At the start of each cycle, run `git fetch origin main` and `scripts/agents/show-goal.sh Studio-E`, then run the remaining commands listed under Start Every Cycle in `docs/plan/agents/Studio-E.md` and follow that goal file. Own JSDoc/JS declaration emit and LSP/WASM compiler-service consumer boundaries. Reuse existing worktrees and the populated TypeScript submodule, preserve Cargo caches, and make small focused commits.
```

```text
/goal You are AgentName Studio-F. At the start of each cycle, run `git fetch origin main` and `scripts/agents/show-goal.sh Studio-F`, then run the remaining commands listed under Start Every Cycle in `docs/plan/agents/Studio-F.md` and follow that goal file. Own launch infrastructure, architecture guardrails, output-surgery burn-down, disk/worktree hygiene, and cheap evidence plumbing. Reuse existing worktrees and the populated TypeScript submodule, preserve Cargo caches, and make small focused commits.
```

## Reviewer

This is an always-on review session. Its goal is intentionally almost never
achieved. If there are no PRs ready for useful review, it waits, periodically
refreshes GitHub state, and reviews the next PR that appears.

```text
/goal You are AgentName Reviewer. At the start of each cycle, run `git fetch origin main` and `scripts/agents/show-goal.sh Reviewer`, then run the remaining commands listed under Start Every Cycle in `docs/plan/agents/Reviewer.md` and follow that goal file. Review open PRs from a high level of abstraction: roadmap fit, tsc parity risk, architecture boundaries, duplicate work, release metric truth, tests, CI readiness, and PR hygiene. Post signed GitHub PR comments with actionable findings. If there are zero reviewable PRs, wait for new PRs to appear and keep checking; this goal is ongoing rather than completable.
```
