# Session Launch Prompts

Use one prompt per Codex session. Replace nothing except the session if you are
relaunching a lane under the same name.

Every session starts by reading its own goal file from repo source, then keeps
using that file as the remote-control surface.

## M1

```text
/goal You are AgentName M1-A. At the start of each cycle, run `git fetch origin main` and `scripts/agents/show-goal.sh M1-A`, then run the remaining commands listed under Start Every Cycle in `docs/plan/agents/M1-A.md` and follow that goal file. Keep `agent:M1-A` labels current on PRs only for now, reuse existing worktrees and the populated TypeScript submodule, preserve Cargo caches, and make small focused commits.
```

```text
/goal You are AgentName M1-B. At the start of each cycle, run `git fetch origin main` and `scripts/agents/show-goal.sh M1-B`, then run the remaining commands listed under Start Every Cycle in `docs/plan/agents/M1-B.md` and follow that goal file. Keep `agent:M1-B` labels current on PRs only for now, reuse existing worktrees and the populated TypeScript submodule, preserve Cargo caches, and make small focused commits.
```

```text
/goal You are AgentName M1-C. At the start of each cycle, run `git fetch origin main` and `scripts/agents/show-goal.sh M1-C`, then run the remaining commands listed under Start Every Cycle in `docs/plan/agents/M1-C.md` and follow that goal file. Keep `agent:M1-C` labels current on PRs only for now, reuse existing worktrees and the populated TypeScript submodule, preserve Cargo caches, and make small focused commits.
```

```text
/goal You are AgentName M1-D. At the start of each cycle, run `git fetch origin main` and `scripts/agents/show-goal.sh M1-D`, then run the remaining commands listed under Start Every Cycle in `docs/plan/agents/M1-D.md` and follow that goal file. Keep `agent:M1-D` labels current on PRs only for now, reuse existing worktrees and the populated TypeScript submodule, preserve Cargo caches, and make small focused commits.
```

## M4

```text
/goal You are AgentName M4-A. At the start of each cycle, run `git fetch origin main` and `scripts/agents/show-goal.sh M4-A`, then run the remaining commands listed under Start Every Cycle in `docs/plan/agents/M4-A.md` and follow that goal file. Keep `agent:M4-A` labels current on PRs only for now, reuse existing worktrees and the populated TypeScript submodule, preserve Cargo caches, and make small focused commits.
```

```text
/goal You are AgentName M4-B. At the start of each cycle, run `git fetch origin main` and `scripts/agents/show-goal.sh M4-B`, then run the remaining commands listed under Start Every Cycle in `docs/plan/agents/M4-B.md` and follow that goal file. Keep `agent:M4-B` labels current on PRs only for now, reuse existing worktrees and the populated TypeScript submodule, preserve Cargo caches, and make small focused commits.
```

```text
/goal You are AgentName M4-C. At the start of each cycle, run `git fetch origin main` and `scripts/agents/show-goal.sh M4-C`, then run the remaining commands listed under Start Every Cycle in `docs/plan/agents/M4-C.md` and follow that goal file. Keep `agent:M4-C` labels current on PRs only for now, reuse existing worktrees and the populated TypeScript submodule, preserve Cargo caches, and make small focused commits.
```

```text
/goal You are AgentName M4-D. At the start of each cycle, run `git fetch origin main` and `scripts/agents/show-goal.sh M4-D`, then run the remaining commands listed under Start Every Cycle in `docs/plan/agents/M4-D.md` and follow that goal file. Keep `agent:M4-D` labels current on PRs only for now, reuse existing worktrees and the populated TypeScript submodule, preserve Cargo caches, and make small focused commits.
```

## Studio

```text
/goal You are AgentName Studio-A. At the start of each cycle, run `git fetch origin main` and `scripts/agents/show-goal.sh Studio-A`, then run the remaining commands listed under Start Every Cycle in `docs/plan/agents/Studio-A.md` and follow that goal file. Keep `agent:Studio-A` labels current on PRs only for now, reuse existing worktrees and the populated TypeScript submodule, preserve Cargo caches, and make small focused commits.
```

```text
/goal You are AgentName Studio-B. At the start of each cycle, run `git fetch origin main` and `scripts/agents/show-goal.sh Studio-B`, then run the remaining commands listed under Start Every Cycle in `docs/plan/agents/Studio-B.md` and follow that goal file. Keep `agent:Studio-B` labels current on PRs only for now, reuse existing worktrees and the populated TypeScript submodule, preserve Cargo caches, and make small focused commits.
```

```text
/goal You are AgentName Studio-C. At the start of each cycle, run `git fetch origin main` and `scripts/agents/show-goal.sh Studio-C`, then run the remaining commands listed under Start Every Cycle in `docs/plan/agents/Studio-C.md` and follow that goal file. Keep `agent:Studio-C` labels current on PRs only for now, reuse existing worktrees and the populated TypeScript submodule, preserve Cargo caches, and make small focused commits.
```

```text
/goal You are AgentName Studio-D. At the start of each cycle, run `git fetch origin main` and `scripts/agents/show-goal.sh Studio-D`, then run the remaining commands listed under Start Every Cycle in `docs/plan/agents/Studio-D.md` and follow that goal file. Keep `agent:Studio-D` labels current on PRs only for now, reuse existing worktrees and the populated TypeScript submodule, preserve Cargo caches, and make small focused commits.
```

```text
/goal You are AgentName Studio-E. At the start of each cycle, run `git fetch origin main` and `scripts/agents/show-goal.sh Studio-E`, then run the remaining commands listed under Start Every Cycle in `docs/plan/agents/Studio-E.md` and follow that goal file. Keep `agent:Studio-E` labels current on PRs only for now, reuse existing worktrees and the populated TypeScript submodule, preserve Cargo caches, and make small focused commits.
```

```text
/goal You are AgentName Studio-F. At the start of each cycle, run `git fetch origin main` and `scripts/agents/show-goal.sh Studio-F`, then run the remaining commands listed under Start Every Cycle in `docs/plan/agents/Studio-F.md` and follow that goal file. Keep `agent:Studio-F` labels current on PRs only for now, reuse existing worktrees and the populated TypeScript submodule, preserve Cargo caches, and make small focused commits.
```

## Reviewer

This is an always-on review session. Its goal is intentionally almost never
achieved. If there are no PRs ready for useful review, it waits, periodically
refreshes GitHub state, and reviews the next PR that appears.

```text
/goal You are AgentName Reviewer. At the start of each cycle, run `git fetch origin main` and `scripts/agents/show-goal.sh Reviewer`, then run the remaining commands listed under Start Every Cycle in `docs/plan/agents/Reviewer.md` and follow that goal file. Review open PRs from a high level of abstraction: roadmap fit, tsc parity risk, architecture boundaries, duplicate work, tests, CI readiness, and PR hygiene. Post signed GitHub PR comments with actionable findings. If there are zero reviewable PRs, wait for new PRs to appear and keep checking; this goal is ongoing rather than completable.
```
