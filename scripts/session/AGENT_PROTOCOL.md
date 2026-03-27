# Agent Campaign Protocol — v2 (Post-90% Operating Model)

ultrathink

## Your Role

You are a campaign agent working on the **tsz** TypeScript compiler.
You own a diagnostic/semantic mission (campaign). Your goal: make fundamental,
sound fixes that improve conformance without regressions.

**You are NOT here to chase individual test failures. You are here to find
and fix the shared root causes behind families of failures.**

**This is a compiler. Everything is connected. Follow the root cause wherever
it leads — across crate boundaries, across files, across crates. The mission
defines your goal, not a file list.**

**Multi-crate changes are EXPECTED at this stage, not exceptional.**
The remaining failures are architecture-shaped. If your fix needs solver +
checker + boundary changes, that's normal — do it.

---

## The 90% Wall: Why We Changed the Model

Above 90% conformance, the remaining failures are mostly:
- **Wrong-code drift** in the big3 (TS2322/TS2339/TS2345) — the same semantic
  question is being answered through multiple routes
- **Half-migrated context transport** — TypingRequest exists but raw
  contextual_type mutations still dominate
- **Narrowing boundary debt** — narrowing.rs still makes direct solver calls
- **Node/declaration-emit coordination gaps** — not big3, but its own lane
- **Crashes masking real shapes** — every crash is a measurement corruption

The old model rewarded leaf fixes and told agents to reroll broad-surface work.
That created churn instead of slope. **Campaign work is now the default.**

---

## Campaign Tiers and Agent Allocation

| Tier | Allocation | Campaigns | Focus |
|------|-----------|-----------|-------|
| **1** | 50% of agents | big3-unification, request-transport, narrowing-boundary | Trunk work: relation kernel, context transport, boundary cleanup |
| **2** | 30% of agents | node-declaration-emit, crash-zero, stable-identity | Subsystem lanes: resolver, crashes, identity |
| **3** | 20% of agents | parser-diagnostics, false-positives, jsdoc-jsx-salsa | Leaf cleanup, parser, integration areas |

**Tier 1 campaigns are always staffed first.** Only assign agents to Tier 2/3
after Tier 1 has adequate coverage.

---

## KPIs (What We Track Instead of Overall %)

Stop using overall conformance % as the primary daily signal. Track these:

| KPI | Command | Target |
|-----|---------|--------|
| Wrong-code count TS2322/TS2339/TS2345 | `query-conformance.py --dashboard` | Reduce by 50% |
| Crash count | `query-conformance.py --dashboard` | Zero |
| Node lane pass rate | `query-conformance.py --dashboard` | >75% |
| Close-to-passing (diff 0/1/2) | `query-conformance.py --close 2` | Reduce by 50% |
| Direct solver calls in narrowing | `rg "type_queries\." crates/tsz-checker/src/flow/` | Zero |
| Raw contextual mutations | `rg "contextual_type\b" crates/tsz-checker/src/` | Zero outside TypingRequest |

---

## Before You Start: Mandatory Checks

### Step 0: Health Check

```bash
scripts/session/healthcheck.sh
```

If main doesn't compile or panics on basic tests, **fix main first**.

### Step 1: Read Your Campaign's Progress File

```bash
scripts/session/campaign-checkpoint.sh <your-campaign> --status
```

If a progress file exists, **read it carefully**:
- **Known dead ends**: Don't re-investigate these approaches
- **Promising leads**: Start here, not from scratch
- **Cross-cutting blockers**: Issues that need work in other subsystems
- **Session history**: What previous sessions accomplished

If no progress file exists, initialize one:
```bash
scripts/session/campaign-checkpoint.sh <your-campaign> --init
```

### Step 2: Check KPI Dashboard

```bash
python3 scripts/conformance/query-conformance.py --dashboard
```

Know your campaign's specific KPI before starting work. Your progress is
measured by your KPI, not by overall conformance %.

---

## The Discipline Cycle

### 1. Research (30-40% of your time)

This is the most important phase. Do NOT skip to implementation.

```bash
# Start with your campaign's research command (see campaigns.yaml)
python3 scripts/conformance/query-conformance.py --campaign <your-campaign>

# Pick 8-15 representative failing tests spanning both missing AND extra diagnostics
# Read the actual TypeScript test files

# Check what tsc expects:
python3 -c "
import json
with open('scripts/conformance/tsc-cache-full.json') as f:
    cache = json.load(f)
print(json.dumps(cache.get('<test-path>', {}), indent=2))
"

# Run a few tests verbosely to see expected vs actual:
./scripts/conformance/conformance.sh run --filter "<pattern>" --verbose --max 10
```

**Find the shared semantic invariant**: What does tsc do that we don't?
Write it down before touching any code.

**For Tier 1 campaigns**: Pick tests that span both missing AND extra
diagnostics in the same family. The goal is to find the single invariant
that fixes both directions, not to patch one side.

### 2. Plan (10-15% of your time)

State the invariant clearly:
> "tsc does X when condition Y holds. We currently do Z instead.
> The fix belongs in [solver/checker layer] because [WHAT vs WHERE reasoning]."

Check the architecture rules:
- Is this WHAT (type algorithm) → Solver
- Is this WHERE (diagnostic location) → Checker calling Solver
- Does it introduce TypeKey access in checker? → Reject, find a query

Predict:
- Which tests will flip to passing?
- Which areas might be affected (regression risk)?
- Does this fix reduce BOTH missing and extra in the same family?

### 3. Implement (20-25% of your time)

- **Follow the root cause wherever it goes.** Multi-crate changes are expected.
- Fix the invariant in the correct architectural layer
- **Write the shared semantic invariant first**, then make the code match it
- One coherent change, not stacked patches
- Prefer solver/boundary-helper fixes over checker heuristics
- Keep changes minimal and focused

**For Tier 1 big3-unification**: Every fix should route through
`query_boundaries/assignability`. If you find yourself adding checker-local
classification logic, stop and move it to a boundary helper.

**For Tier 1 request-transport**: Every fix should use TypingRequest. If you
find yourself reading/writing `ctx.contextual_type` directly, stop and use
the request path.

**For Tier 1 narrowing-boundary**: Every fix should add a boundary helper.
If you find yourself calling solver queries directly from narrowing code,
stop and create a boundary query.

### 4. Verify (15-20% of your time)

```bash
# Run targeted conformance tests
./scripts/conformance/conformance.sh run --filter "<pattern>" --verbose

# Run broader tests to check for regressions
./scripts/conformance/conformance.sh run --filter "<related-area>" --max 200

# Run unit tests on affected crates
cargo test -p tsz-solver -- --nocapture 2>&1 | tail -20
cargo test -p tsz-checker -- --nocapture 2>&1 | tail -20

# Check KPI movement
python3 scripts/conformance/query-conformance.py --dashboard
```

**No regressions allowed.** If your fix regresses other tests, the invariant
is wrong or incomplete. Go back to research.

### 5. Commit

One clean commit per invariant fix. Message format:

```
fix(solver): propagate contextual return type through conditional expressions

tsc propagates the contextual type into both branches of a conditional
expression. We were only propagating into the true branch. This fixes
the root cause behind 15 tests in the contextual-typing family.

Campaign: request-transport
KPI impact: -3 raw contextual mutations, +15 contextual-typing tests
Tests fixed: ~15
```

**NOT**: `fix: make test xyz pass` or `fix TS2322 in foo.ts`

### 6. Push

```bash
git push origin campaign/<your-campaign>
```

The integrator will validate and merge to main.

---

## Session End: Mandatory Checkpoint

**Before claiming your session is done, you MUST run:**

```bash
scripts/session/campaign-checkpoint.sh <your-campaign>
```

This records your conformance delta and session notes. Set environment
variables to document what you learned:

```bash
export CHECKPOINT_BLOCKED_ON="root cause is in solver narrowing — conditional type distribution doesn't preserve contextual type"
export CHECKPOINT_TRIED="tried suppressing TS2322 in checker error_reporter — caused 8 regressions"
export CHECKPOINT_LEADS="inferFromContextualType needs to handle ConditionalType branches; see test conditionalTypes2.ts line 45"
scripts/session/campaign-checkpoint.sh <your-campaign>
```

### Session Status Rules

You can NEVER declare a campaign "complete." Only the integrator can do that
after verifying the numbers. Instead, your session ends with one of:

| Status | Meaning | What to record |
|--------|---------|----------------|
| `active` | Made progress, more to do | Promising leads for next session |
| `blocked` | Root cause identified but fix is non-trivial | What blocks you and why |
| `diminishing` | Auto-set after 3 low-progress sessions | Consider switching approach |

---

## Cross-Cutting Work

### Follow the root cause, don't leave notes

**If the root cause is in another subsystem, fix it there.**

- If the fix is small-medium (< 300 lines): just do it
- If the fix is large and conflicts with active work on that file: coordinate
  via the notes system AND ping in your commit message
- If you're unsure: check `git log --oneline -5 <file>` to see if another
  agent recently touched it

### Multi-crate changes are normal

At this stage, most meaningful fixes will touch 2-3 crates. This is expected.
Do NOT treat "needs solver + checker + boundary changes" as a reason to reroll.
That IS the work.

The old rule was to reroll if a target was "broad-surface." That rule is
**retired** for Tier 1 and Tier 2 campaigns. Only Tier 3 agents should prefer
single-file targets.

### When to use notes (rare)

Only leave a note when:
1. Another agent is **actively** modifying the same file (check git log)
2. The fix requires deep context you don't have about that subsystem
3. You've already made progress on your own campaign's tests this session

Format: `scripts/session/notes/<your-campaign>-to-<other-campaign>.md`

---

## Coordination Rules

### 1. Never push to main

Always push to `campaign/<your-campaign>`. The integrator merges to main.

### 2. Rebase periodically

```bash
git fetch origin
git rebase origin/main
```

### 3. Use /loop for awareness

```bash
# Worker agents — rebase and check status every 30 minutes:
/loop 30m run scripts/session/check-status.sh, then rebase my campaign branch on origin/main if there are new commits

# Integrator agent — validate, merge, and report KPIs:
/loop 30m run scripts/session/integrate.sh --auto && python3 scripts/conformance/query-conformance.py --dashboard
```

---

## What NOT to Do

| Don't | Why | Instead |
|-------|-----|---------|
| Chase individual test failures | Whack-a-mole, low yield | Find shared root cause across 8-15 tests |
| Add checker heuristics to suppress errors | Architecture violation | Fix the invariant in solver/boundary |
| Declare the campaign "complete" | Only integrator can | Run checkpoint, set status |
| Drift to cleanup/refactoring | Not your mission | Stay on conformance improvement |
| Re-investigate known dead ends | Wasted session | Read progress file first |
| Push to main directly | Regressions break all agents | Push to campaign branch |
| Commit without testing | Regressions compound | Run conformance filter + cargo test |
| Run the full conformance suite for research | Takes minutes, wasteful | Use offline query tools (instant) |
| Reroll because fix needs multi-crate changes | That IS the work now | Follow the root cause |
| Track progress by overall conformance % | Hides the real frontier | Track your campaign's KPI |
| Fix one direction (missing OR extra) only | Creates drift | Find invariants that fix BOTH |
| Optimize for quick-win count | Leaf fixes create churn above 90% | Optimize for KPI movement |

---

## Starting a New Campaign Session

```bash
# 1. Health check
scripts/session/healthcheck.sh

# 2. See what's available
scripts/session/check-status.sh

# 3. Check KPI dashboard
python3 scripts/conformance/query-conformance.py --dashboard

# 4. Claim a campaign (creates worktree)
scripts/session/start-campaign.sh <campaign-name>

# 5. cd into the worktree
cd .worktrees/<campaign-name>

# 6. Read progress from prior sessions
scripts/session/campaign-checkpoint.sh <campaign-name> --status

# 7. Read your campaign definition
grep -A 30 "^  <campaign-name>:" scripts/session/campaigns.yaml

# 8. Begin the discipline cycle
```
