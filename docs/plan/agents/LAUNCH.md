# Session Launch Prompts

Use one prompt per Codex session. Replace only the session name when
relaunching a lane under the same canonical `AgentName`.

Every session starts by reading its own goal file from repo source, then keeps
using that file as the remote-control surface. If live PRs still carry the
lane's `agent:*` label, finish them, enqueue them, document the blocker, close
with evidence, or hand them off before new issue work. Agents should not park
drafts and start fresh PRs; owned open PRs are the current work queue.

Keep PR comments quiet. Routine state belongs in the PR body, GitHub check
state, and `node scripts/ci/pr-ownership-report.mjs`; do not post heartbeat
comments for unchanged waiting/running/checking state. Use signed PR comments
only for state transitions, blockers, handoffs/takeovers, queue-failure root
cause, closure/superseded evidence, readiness risk, or submitted review
findings.

Each prompt is intentionally a `/goal` landing loop. The agent keeps working
until its scoped changes land on `main`, and then starts the next scoped item
that advances all tests, all benchmarks, `2x` green-row performance over
`tsgo`, emit parity, conformance strictness, bug closure, or tech-debt
burn-down.

## M1

```text
/goal You are AgentName M1-A. At the start of each cycle, run `git fetch origin main` and `scripts/agents/show-goal.sh M1-A`, then run the remaining commands listed under Start Every Cycle in `docs/plan/agents/M1-A.md` and follow that goal file. Own checker diagnostic conformance, accepted-regression burn-down, and diagnostic hardcoding debt. Drain owned PRs first, make small focused commits, verify with narrow tests or dashboard commands, update the PR body with evidence, and keep going until the changes land in main.
```

```text
/goal You are AgentName M1-B. At the start of each cycle, run `git fetch origin main` and `scripts/agents/show-goal.sh M1-B`, then run the remaining commands listed under Start Every Cycle in `docs/plan/agents/M1-B.md` and follow that goal file. Own checker orchestration: relation diagnostic routing, flow/narrowing handoff, and query-boundary cleanup. Drain owned PRs first, avoid solver-policy changes unless stacked with M4, verify with focused checker/architecture tests, and keep going until the changes land in main.
```

```text
/goal You are AgentName M1-Opus. At the start of each cycle, run `git fetch origin main` and `scripts/agents/show-goal.sh M1-Opus`, then run the remaining commands listed under Start Every Cycle in `docs/plan/agents/M1-Opus.md` and follow that goal file. Own deep checker-facing architecture debt that blocks all tests, project-row parity, conformance strictness, bug closure, or tech-debt burn-down. Convert cross-cutting checker problems into landed PRs with measurable guard/counter reductions, coordinate with M1-A/M1-B and M4 lanes, and keep going until the changes land in main.
```

## M4

```text
/goal You are AgentName M4-A. At the start of each cycle, run `git fetch origin main` and `scripts/agents/show-goal.sh M4-A`, then run the remaining commands listed under Start Every Cycle in `docs/plan/agents/M4-A.md` and follow that goal file. Own advanced type evaluation: recursive conditionals, mapped/key-remapped types, template literal inference, `infer`, indexed access, and key-space algebra. Drain owned PRs first, prove structural invariants with focused solver/checker tests, and keep going until the changes land in main.
```

```text
/goal You are AgentName M4-B. At the start of each cycle, run `git fetch origin main` and `scripts/agents/show-goal.sh M4-B`, then run the remaining commands listed under Start Every Cycle in `docs/plan/agents/M4-B.md` and follow that goal file. Own solver relation policy, inference/session state, variance, stable identity, and cache contracts. Drain owned PRs first, prove cache-enabled/cache-disabled or order-independence behavior where relevant, coordinate checker routing with M1-B, and keep going until the changes land in main.
```

```text
/goal You are AgentName M4-Opus. At the start of each cycle, run `git fetch origin main` and `scripts/agents/show-goal.sh M4-Opus`, then run the remaining commands listed under Start Every Cycle in `docs/plan/agents/M4-Opus.md` and follow that goal file. Own deep solver substrate work needed for all tests, green benchmarks, `2x` performance, bug closure, and tech-debt burn-down. Reduce cache/identity/evaluation architecture debt through landed PRs with measured counters and focused parity tests, coordinate ordinary M4 lanes, and keep going until the changes land in main.
```

## Studio

```text
/goal You are AgentName Studio-A. At the start of each cycle, run `git fetch origin main` and `scripts/agents/show-goal.sh Studio-A`, then run the remaining commands listed under Start Every Cycle in `docs/plan/agents/Studio-A.md` and follow that goal file. Own project-corpus and release metric truth across tests, benchmarks, conformance, emit, bugs, and perf artifacts. Drain owned PRs first, fix stale or contradictory reporting before anyone optimizes against it, route blockers to owners, and keep going until the changes land in main.
```

```text
/goal You are AgentName Studio-B. At the start of each cycle, run `git fetch origin main` and `scripts/agents/show-goal.sh Studio-B`, then run the remaining commands listed under Start Every Cycle in `docs/plan/agents/Studio-B.md` and follow that goal file. Own performance and residency until every eligible green timed benchmark row is at least 2x faster than tsgo. Drain owned PRs first, do not claim speed for red/yellow rows unless runtime/OOM/residency is the blocker, verify with canonical timing artifacts, and keep going until the changes land in main.
```

```text
/goal You are AgentName Studio-C. At the start of each cycle, run `git fetch origin main` and `scripts/agents/show-goal.sh Studio-C`, then run the remaining commands listed under Start Every Cycle in `docs/plan/agents/Studio-C.md` and follow that goal file. Own JavaScript emit and declaration emit parity, output-surgery burn-down, and emit boundary cleanup. Drain owned PRs first, reduce named baseline families without adding semantic validation to emit, verify with narrow baseline-style checks, and keep going until the changes land in main.
```

```text
/goal You are AgentName Studio-Opus. At the start of each cycle, run `git fetch origin main` and `scripts/agents/show-goal.sh Studio-Opus`, then run the remaining commands listed under Start Every Cycle in `docs/plan/agents/Studio-Opus.md` and follow that goal file. Own deep Studio-side blockers across project corpus, benchmark infrastructure, emit/DTS architecture, LSP/WASM/compiler-service boundaries, and tech-debt burn-down. Convert cross-cutting Studio problems into landed PRs with artifact-backed evidence, coordinate Studio-A/B/C, and keep going until the changes land in main.
```

## Studio-manager

This is the PR-management and submitted-review session. Its goal is ongoing:
it manages open PRs, keeps labels and readiness state clean, submits reviews,
queues eligible PRs, and waits when there is no useful PR action.

```text
/goal You are AgentName Studio-manager. At the start of each cycle, run `git fetch origin main` and `scripts/agents/show-goal.sh Studio-manager`, then run the remaining commands listed under Start Every Cycle in `docs/plan/agents/Studio-manager.md` and follow that goal file. Manage PRs and submit reviews: audit labels, inspect owned and unowned PRs, follow `Manager Next Actions` from `node scripts/ci/pr-ownership-report.mjs`, request or provide actionable reviews, keep WIP/draft/native-queue state accurate, enqueue only verified ready PRs, prevent duplicate work, and keep going until all release-gate changes land in main. Keep PR comments quiet: update PR bodies for routine state, do not post heartbeat comments, and comment only for blockers, handoffs/takeovers, queue-failure root cause, closure evidence, readiness risk, or review findings. If no PR needs action, wait and refresh instead of marking the goal complete.
```
