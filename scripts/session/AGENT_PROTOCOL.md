# Agent Campaign Protocol

## Your Role

You are a campaign agent working on the **tsz** TypeScript compiler.
You own a specific mechanism area (campaign). Your goal: make fundamental,
sound fixes that improve conformance without regressions.

**You are NOT here to chase individual test failures. You are here to find
and fix the shared root causes behind families of failures.**

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

## Coordination Rules

### 1. Stay in your lane

Your campaign defines primary files (see `campaigns.yaml`). Focus there.
If you discover a root cause in another campaign's files:

- If it's a one-line fix: make it, note it in your commit message
- If it's substantial: leave a note and don't fix it. The owning campaign
  agent will handle it. Create a file at:
  `scripts/session/notes/<your-campaign>-to-<other-campaign>.md`

### 2. Never push to main

Always push to `campaign/<your-campaign>`. The integrator merges to main.

### 3. Rebase periodically

```bash
# Before starting new work:
git fetch origin
git rebase origin/main

# If conflicts: resolve carefully, don't drop changes
```

### 4. Use /loop for awareness

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
| Modify files outside your campaign | Merge conflicts with other agents | Leave a note for the owning campaign |
| Push to main directly | Regressions break all agents | Push to campaign branch |
| Commit without testing | Regressions compound | Run conformance filter + cargo test |
| Make WIP/checkpoint commits | Noise, low quality | One clean commit per invariant |
| Run the full conformance suite for research | Takes minutes, wasteful | Use offline query tools (instant) |

---

## Starting a New Campaign Session

```bash
# 1. See what's available
scripts/session/check-status.sh

# 2. Claim a campaign
scripts/session/start-campaign.sh <campaign-name>

# 3. cd into the worktree
cd .worktrees/<campaign-name>

# 4. Read your campaign definition
grep -A 20 "^  <campaign-name>:" scripts/session/campaigns.yaml

# 5. Run your research command
<paste research_command from campaigns.yaml>

# 6. Begin the discipline cycle
```

---

## Campaign Completion

When you've exhausted the low-hanging fruit in your campaign:

1. Push your final commits
2. Tell the integrator your campaign branch is ready for final merge
3. Pick a new unclaimed campaign (check-status.sh shows availability)
4. Or go deeper on the same campaign with a new branch:
   `campaign/<name>-v2`
