# Agent Campaign Protocol — v3 (Post-93% Fingerprint Era)

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

---

## The 93% Reality: Why We Changed the Model Again

At **93.3% conformance** (11741/12579), the remaining 838 failures break down:

| Category | Count | % | What's wrong |
|----------|-------|---|-------------|
| **Fingerprint-only** | **617** | **73.6%** | Right error codes, wrong message/position/count |
| Wrong codes | 174 | 20.8% | Different error codes than tsc |
| Crashes / all-missing | 44 | 5.3% | We emit 0, tsc expects diagnostics |
| False positives | 3 | 0.4% | We emit errors, tsc expects 0 |

**Three-quarters of remaining failures already emit the correct error codes.**
The diagnostics just have wrong message text (~310 tests), different instance
counts (~130 tests), or wrong line/column positions (~90 tests).

The v2 model focused on wrong-code drift and architecture campaigns. That was
right at 90-92%. At 93%+, **fingerprint parity is the dominant remaining
problem** and requires a different methodology.

---

## Campaign Tiers and Agent Allocation

| Tier | Allocation | Campaigns | Focus |
|------|-----------|-----------|-------|
| **1** | 50% of agents | type-display-parity, diagnostic-count | Fingerprint parity: match tsc's error message text and diagnostic counts |
| **2** | 30% of agents | big3-unification, narrowing-boundary, request-transport | Wrong-code: relation kernel, context transport, boundary cleanup |
| **3** | 20% of agents | crash-zero, diagnostic-position, parser-diagnostics, module-resolution, jsdoc-jsx-salsa | Subsystem and leaf work |

**Tier 1 campaigns are always staffed first.** Only assign agents to Tier 2/3
after Tier 1 has adequate coverage.

---

## KPIs (What We Track Instead of Overall %)

| KPI | Command | Target |
|-----|---------|--------|
| **Fingerprint-only count** | `query-conformance.py --dashboard` | Reduce from 617 |
| FP-only for TS2322+TS2345+TS2339 | `query-conformance.py --fingerprint-only` | Reduce from 410 |
| Big3 wrong-code count | `query-conformance.py --dashboard` | Reduce from 71 |
| Crash count | `query-conformance.py --dashboard` | Zero (44) |
| Tests per fix commit | git log analysis | >1.0 (currently 0.32) |

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

If no progress file exists, initialize one:
```bash
scripts/session/campaign-checkpoint.sh <your-campaign> --init
```

### Step 2: Check KPI Dashboard

```bash
python3 scripts/conformance/query-conformance.py --dashboard
```

Know your campaign's specific KPI before starting work.

---

## The Discipline Cycle

### For Fingerprint Campaigns (Tier 1)

Fingerprint work has a fundamentally different methodology than wrong-code work.
You are NOT looking for missing/extra error codes. You are looking for **why
the error message text, position, or count differs** from tsc.

**1. Research (30-40%)**

```bash
# See which codes have the most fingerprint-only failures
python3 scripts/conformance/query-conformance.py --fingerprint-only

# Pick a code (e.g., TS2322) and list affected tests
python3 scripts/conformance/query-conformance.py --fingerprint-only --code TS2322

# Run 10-15 tests with fingerprint detail
./scripts/conformance/conformance.sh run --filter "test1|test2|test3" --verbose
```

The verbose output shows `missing-fingerprints:` (tsc has, we don't) and
`extra-fingerprints:` (we have, tsc doesn't). Compare the `message_key` and
`line:column` fields.

**Classify the mismatch pattern**:
- Message differs, position same → type display bug → fix the printer
- Position differs, message similar → diagnostic anchor bug → fix the error site
- Count differs → emission/suppression rule bug → fix the checker logic

**2. Plan (10-15%)**

State the divergence pattern:
> "tsc displays type X as `Foo<T>` (alias form). We display it as
> `{ a: string; b: number }` (expanded form). The decision point is in
> display.rs at the TypeData::Application branch."

**3. Implement (20-25%)**

For type-display-parity: Fix the printer code in `crates/tsz-solver/src/display/`.
For diagnostic-count: Fix emission guards in `crates/tsz-checker/src/checkers/`.

**4. Measure batch impact (critical for fingerprint work)**

```bash
# Run ALL fingerprint-only tests for your code family (not just the 10 you researched)
python3 scripts/conformance/query-conformance.py --fingerprint-only --code TS2322 --paths-only | head -30
# Use those paths to run a broader sample
./scripts/conformance/conformance.sh run --filter "BROAD_PATTERN" --max 50
```

**A good fingerprint fix should flip 5+ tests at once.** If your fix only
affects 1 test, you're fixing a symptom, not the pattern. Go back to research.

### For Wrong-Code Campaigns (Tier 2)

**1. Research (30-40%)**

```bash
python3 scripts/conformance/query-conformance.py --campaign <your-campaign>

# Pick 8-15 tests spanning BOTH missing AND extra diagnostics
./scripts/conformance/conformance.sh run --filter "<pattern>" --verbose --max 15
```

**2. Plan: Find the shared invariant**

Write it down before touching code:
> "tsc does X when condition Y holds. We currently do Z instead."

For wrong-code campaigns, the invariant must explain BOTH missing AND extra
diagnostics. If it only explains one direction, keep researching.

**3. Implement**

- Fix the invariant in the correct architectural layer
- Follow root cause across crate boundaries (expected for Tier 2)
- Every big3 fix routes through `query_boundaries/assignability`
- Every request-transport fix uses TypingRequest, not raw mutations
- Every narrowing-boundary fix adds a boundary helper

**4. Verify**

```bash
# Targeted test
./scripts/conformance/conformance.sh run --filter "TESTNAME" --verbose

# Regression check
./scripts/conformance/conformance.sh run --max 200

# Unit tests
cargo nextest run --package tsz-checker --lib
cargo nextest run --package tsz-solver --lib

# KPI check
python3 scripts/conformance/query-conformance.py --dashboard
```

### For Leaf Fixes (Tier 3)

```bash
python3 scripts/conformance/query-conformance.py --one-extra
python3 scripts/conformance/query-conformance.py --one-missing
python3 scripts/conformance/query-conformance.py --close 2
```

Tier 3 agents may reroll if a target needs multi-crate changes.

---

## Efficiency Rules

The last 100 commits achieved +24 tests from 75 fix commits (0.32 tests/commit)
with 35% regression churn. This is unsustainable.

**Batch, don't scatter**: One printer fix that flips 20 tests > 20 checker
tweaks that each flip 1.

**Minimize snapshot commits**: Don't commit a snapshot after every single fix.
Batch fixes, then snapshot once per session.

**Check for regressions before committing**: If your fix causes any PASS→FAIL
flips, investigate even if the net is positive.

**Track your efficiency**: Before pushing, count tests fixed vs commits made.
Target >1.0 tests per commit.

---

## Commit Format

One clean commit per invariant fix:

```
fix(solver): match tsc alias-vs-expansion display for generic mapped types

tsc displays `Id<{x: string}>` in expanded form for error messages while
preserving the alias for hover. Our printer was using the alias form in
both contexts. Match tsc's display policy for Application types.

Campaign: type-display-parity
KPI impact: fingerprint-only TS2322 reduced by ~30
Tests fixed: ~30
```

**NOT**: `fix: make test xyz pass` or `fix TS2322 in foo.ts`

Push to `campaign/<your-campaign>` and open a pull request targeting `main`.
The integrator reviews/validates the PR; do not merge or push to `main` yourself.

### PR description

When the fix changes how a TypeScript program is checked, narrowed, inferred,
or emitted, **include a short TypeScript code example** in the PR body that
illustrates the divergence and the behavior after the fix. Use fenced
```ts blocks. Prefer a minimal snippet over the full conformance test. Show
before/after behaviour (e.g. expected diagnostic or inferred type) when
relevant. Skip the snippet only for purely mechanical changes (renames,
formatting, doc-only edits) where it would add no information.

---

## Session End: Mandatory Checkpoint

```bash
export CHECKPOINT_TRIED="tried fixing alias expansion in display.rs — only covers Application, not Conditional"
export CHECKPOINT_LEADS="ConditionalType display also needs alias-vs-expansion logic; see conditionalTypes2.ts"
scripts/session/campaign-checkpoint.sh <your-campaign>
```

You can NEVER declare a campaign "complete." Only the integrator can.

---

## Cross-Cutting Work

**Follow the root cause, don't leave notes.** If the fix is in another
subsystem, just fix it there (< 300 lines). Only coordinate when another
agent is actively modifying the same file.

Multi-crate changes are normal. Do NOT reroll for "broad-surface" targets
(Tier 1/2). Only Tier 3 agents should prefer single-file targets.

---

## What NOT to Do

| Don't | Why | Instead |
|-------|-----|---------|
| Chase individual test failures | 0.32 tests/commit | Fix patterns that flip 5+ tests |
| Ignore fingerprint-only failures | They're 73.6% of remaining work | Prioritize display/count fixes |
| Add checker heuristics to suppress errors | Architecture violation | Fix in solver/boundary |
| Declare the campaign "complete" | Only integrator can | Run checkpoint |
| Re-investigate known dead ends | Wasted session | Read progress file first |
| Push to main directly | Regressions break all agents | Push to campaign branch and open a PR |
| Merge your own PR | Only the integrator validates and merges | Let integrator review and merge |
| Run full conformance for research | Takes minutes, wasteful | Use offline query tools |
| Reroll for multi-crate changes (Tier 1/2) | That IS the work | Follow the root cause |
| Track progress by overall % | Hides the real frontier | Track your campaign's KPI |
| Fix one direction (missing OR extra) only | Creates drift | Find invariants for both |
| Accept 1-test-per-commit efficiency | Unsustainable | Aim for batch fixes |
| Commit snapshots after every single fix | 25% noise commits | Batch snapshots |

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
