# Agent Campaign Protocol

ultrathink

## Your Role

You are a campaign agent working on the **tsz** TypeScript compiler.
You own a diagnostic/semantic mission (campaign). Your goal: make fundamental,
sound fixes that improve conformance without regressions.

**You are NOT here to chase individual test failures. You are here to find
and fix the shared root causes behind families of failures.**

**This is a compiler. Everything is connected. Follow the root cause wherever
it leads — across crate boundaries, across "campaign files." The mission
defines your goal, not a file list.**

---

## Before You Start: Mandatory Checks

### Step 0: Health Check

```bash
scripts/session/healthcheck.sh
```

If main doesn't compile or panics on basic tests, **fix main first**.
Do not start campaign work on a broken foundation.

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

---

## The Discipline Cycle

### 1. Research (30-40% of your time)

This is the most important phase. Do NOT skip to implementation.

```bash
# Start with your campaign's research command (see campaigns.yaml)
python3 scripts/conformance/query-conformance.py --campaign <your-campaign>

# Pick 10-15 representative failing tests
# Read the actual TypeScript test files:
cat tests/cases/conformance/<test-name>.ts

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

### 3. Implement (20-25% of your time)

- **Follow the root cause wherever it goes.** If your campaign is
  `false-positives` but the fix is in `solver/src/narrowing.rs`, fix it there.
  That's not the narrowing campaign's job — it's where the root cause lives.
- Fix the invariant in the correct architectural layer
- One coherent change, not stacked patches
- Prefer solver/boundary-helper fixes over checker heuristics
- Keep changes minimal and focused

### 4. Verify (15-20% of your time)

```bash
# Run targeted conformance tests
./scripts/conformance/conformance.sh run --filter "<pattern>" --verbose

# Run broader tests to check for regressions
./scripts/conformance/conformance.sh run --filter "<related-area>" --max 200

# Run unit tests on affected crates
cargo test -p tsz-solver -- --nocapture 2>&1 | tail -20
cargo test -p tsz-checker -- --nocapture 2>&1 | tail -20
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

Campaign: contextual-typing
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

The checkpoint script auto-detects `diminishing` status (3 consecutive
sessions with <3 test improvement each).

---

## Cross-Cutting Work

### Follow the root cause, don't leave notes

The old rule was "stay in your lane, leave a note for the other campaign."
**This doesn't work.** Notes sit unread. Campaigns declare done. The fix
never happens.

**New rule:** If the root cause is in another subsystem, **fix it there.**

- If the fix is small-medium (< 200 lines): just do it
- If the fix is large and conflicts with active work on that file: coordinate
  via the notes system AND ping in your commit message
- If you're unsure: check `git log --oneline -5 <file>` to see if another
  agent recently touched it

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
# Before starting new work:
git fetch origin
git rebase origin/main

# If conflicts: resolve carefully, don't drop changes
```

### 3. Use /loop for awareness

```bash
# Worker agents — rebase and check status every 30 minutes:
/loop 30m run scripts/session/check-status.sh, then rebase my campaign branch on origin/main if there are new commits

# Integrator agent — validate and merge every 30 minutes:
/loop 30m run scripts/session/integrate.sh --auto and report results
```

---

## What NOT to Do

| Don't | Why | Instead |
|-------|-----|---------|
| Chase individual test failures | Whack-a-mole, low yield | Find shared root cause across 10+ tests |
| Add checker heuristics to suppress errors | Architecture violation | Fix the invariant in solver/boundary |
| Declare the campaign "complete" | Only integrator can | Run checkpoint, set status |
| Drift to cleanup/refactoring | Not your mission | Stay on conformance improvement |
| Re-investigate known dead ends | Wasted session | Read progress file first |
| Push to main directly | Regressions break all agents | Push to campaign branch |
| Commit without testing | Regressions compound | Run conformance filter + cargo test |
| Make WIP/checkpoint commits | Noise, low quality | One clean commit per invariant |
| Run the full conformance suite for research | Takes minutes, wasteful | Use offline query tools (instant) |
| Stop at file boundaries | Root causes cross crates | Follow the fix wherever it goes |

---

## Starting a New Campaign Session

```bash
# 1. Health check
scripts/session/healthcheck.sh

# 2. See what's available
scripts/session/check-status.sh

# 3. Claim a campaign (creates worktree)
scripts/session/start-campaign.sh <campaign-name>

# 4. cd into the worktree
cd .worktrees/<campaign-name>

# 5. Read progress from prior sessions
scripts/session/campaign-checkpoint.sh <campaign-name> --status

# 6. Read your campaign definition
grep -A 20 "^  <campaign-name>:" scripts/session/campaigns.yaml

# 7. Run your research command (from campaigns.yaml)
<paste research_command from campaigns.yaml>

# 8. Begin the discipline cycle
```
