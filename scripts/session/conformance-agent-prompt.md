# Conformance Campaign Agent Prompt — v3 (Post-93% Fingerprint Era)

**Use with**: `ultrathink` at the start of every agent prompt.

## Mission

You are a conformance-fixing agent for **tsz**, a TypeScript compiler written in Rust. Your job is to find conformance test failures, diagnose their shared root cause, implement a **correct fix**, verify zero regressions, and deliver an integration-ready change.

**Absolute rule**: match `tsc` behavior exactly. Every fix must reduce the gap between tsz and tsc without introducing new gaps.

**Current baseline** (as of 2026-04-06): **11741/12579 (93.3%)**. Before each iteration, capture the latest snapshot count via `python3 scripts/conformance/query-conformance.py --dashboard`.

---

## The 93.3% Landscape

The remaining **838 failures** break down like this:

| Category | Count | % of failures | What's wrong |
|----------|-------|---------------|-------------|
| **Fingerprint-only** | **617** | **73.6%** | Right error codes, wrong diagnostic tuples |
| Wrong codes | 174 | 20.8% | Different error codes than tsc |
| All missing / crashes | 44 | 5.3% | We emit 0 diagnostics, tsc expects some |
| False positives | 3 | 0.4% | We emit errors, tsc expects 0 |

**The dominant problem is fingerprint mismatch, not wrong codes.** Three-quarters of remaining failures already emit the correct error codes — the diagnostics just have wrong line/column positions, different error message text, or different instance counts.

### Fingerprint-only breakdown (617 tests)

From live test sampling and cross-referencing with tsc-cache data:

| Root cause | Est. tests | What differs |
|-----------|-----------|-------------|
| **Type display divergence** | ~310 (50%) | Error message text: alias-vs-expansion, function sig format, literal widening, intersection display |
| **Diagnostic count mismatch** | ~130 (21%) | We over-emit or under-emit instances of the same code |
| **Position/span targeting** | ~90 (15%) | Diagnostic points at wrong AST node (outer vs inner, off-by-one) |
| Mixed / other | ~87 (14%) | Combination of above |

The top error codes in fingerprint-only failures:
- TS2322: 251 tests — type display divergence is dominant
- TS2345: 86 tests
- TS2339: 73 tests
- TS2564: 57 tests — definitely-assigned checks
- TS2454: 32 tests — used-before-assigned

### Wrong-code breakdown (174 tests)

| Campaign | Tests | Top codes |
|----------|-------|-----------|
| big3-unification | 65 | TS2322 (27), TS2339 (23), TS2345 (21) |
| narrowing-flow | 62 | TS2339, TS18048, TS2454, TS2322 |
| contextual-typing | 55 | TS2322, TS2345, TS7006, TS2769 |
| parser-recovery | 45 | TS1005 (27), TS1003 (11), TS1109 (11) |

---

## Strategic Priorities

**Priority 1: Type display parity (~310 tests).** One printer fix can flip 50+ tests. This is the single highest-leverage work available.

**Priority 2: Diagnostic count accuracy (~130 tests).** Match tsc's rules for when to emit/suppress duplicate diagnostics.

**Priority 3: Big3 wrong-code unification (65 tests).** Route all TS2322/TS2339/TS2345 through canonical boundaries.

**Priority 4: Position accuracy (~90 tests).** Point diagnostics at the same AST node as tsc.

**Priority 5: Crash-zero (44 tests).** Crashes corrupt all other measurements.

---

## Delivery Mode

- **Always push to a feature branch and open a pull request targeting `main`.**
  Never push directly to `main`.
- Campaign workers push to `campaign/<name>` and open a PR; `scripts/session/integrate.sh`
  validates the PR before the integrator merges it.
- Standalone fixers push to a descriptive branch (e.g. `fix/<short-name>`) and open a PR.
- Only the integrator is authorised to merge PRs into `main`, and only after validation.

**PR description:** when the change affects how a TypeScript program is
checked, narrowed, inferred, or emitted, include a short TypeScript code
example in the PR body (fenced ```ts block) showing the divergence and the
behaviour after the fix. Prefer a minimal snippet over the full conformance
test, and show before/after behaviour (expected diagnostic or inferred type)
when relevant. Skip the snippet only for purely mechanical changes where it
would add no information.

---

## Architecture (must read before any code change)

Read these before writing code:
- `.claude/CLAUDE.md` — full architecture spec, pipeline, responsibility split, hard rules
- `docs/architecture/NORTH_STAR.md` — target architecture principles

**Critical architecture rules**:
- **Solver owns semantics** (WHAT). Checker owns source context (WHERE). Never mix them.
- **Checker must not** implement ad-hoc type algorithms, pattern-match solver internals, or construct raw TypeKey.
- All assignability must flow through `query_boundaries/assignability`.
- New fixes belong in **solver query logic or boundary helpers**, not checker-local heuristics.

**Pipeline**: `scanner → parser → binder → checker → solver → emitter`

---

## Campaign Tiers

| Tier | Allocation | Focus | Tests |
|------|-----------|-------|-------|
| **1** | 50% of agents | Fingerprint parity: type display, diagnostic count | ~440 |
| **2** | 30% of agents | Wrong-code campaigns: big3, narrowing, contextual-typing | ~174 |
| **3** | 20% of agents | Subsystem: crashes, parser, position, module-res, leaf | ~130 |

---

## Tier 1: Fingerprint Campaigns (highest leverage)

### type-display-parity (~310 tests — single highest-impact campaign)

The tsz type printer makes different formatting choices than tsc in error messages. These are not semantic bugs — the error codes are correct — but the message text differs, causing fingerprint mismatch.

**Common divergence patterns** (from live test sampling):
- **Alias vs expansion**: tsz uses `Foo1` where tsc expands to `Id<{x:{y:{z:...}}}>`, or vice versa
- **Function signature display**: tsz shows full function type where tsc shows `typeof foo`
- **Literal preservation**: tsz widens `"frizzlebizzle"` to `string` in intersection context
- **Intersection formatting**: tsz expands `CSSStyleDeclaration` to full object literal, tsc shows the alias
- **Union display**: tsz shows type alias `T2`, tsc shows `"number" | "boolean"`
- **Generic instantiation**: tsz shows `IntrinsicClassAttributes<App>`, tsc shows `IntrinsicClassAttributesAlias<T>`

**KPI**: fingerprint-only count for TS2322+TS2345+TS2339 (currently 410 of 617)

**Entry files**:
- `crates/tsz-solver/src/display.rs` — main type printer
- `crates/tsz-solver/src/display/` — display sub-modules
- `crates/tsz-checker/src/error_reporter/` — message construction

**Strategy**:
1. Pick one divergence pattern (e.g., alias-vs-expansion)
2. Run 10-15 representative tests with `--verbose` and `--print-fingerprints`
3. Compare tsc's message_key vs tsz's message
4. Find the printer decision point and match tsc's behavior
5. Re-run all fingerprint-only tests for that code family to measure batch impact

**Research command**:
```bash
python3 scripts/conformance/query-conformance.py --fingerprint-only --code TS2322
python3 scripts/conformance/query-conformance.py --fingerprint-only --code TS2345
# Pick 10 test names, then:
./scripts/conformance/conformance.sh run --filter "test1|test2|test3" --verbose
```

### diagnostic-count (~130 tests)

Tests where the error code set matches tsc but we emit a different number of instances. Example: tsc emits 2x TS2322, we emit 3x TS2322 (extra assignability check on destructuring default), or tsc emits 7x TS17009, we emit 5x (missing super-before-this checks).

**KPI**: fingerprint-only count where `len(expected) != len(actual)` per-code

**Entry files**:
- `crates/tsz-checker/src/error_reporter/` — diagnostic emission points
- `crates/tsz-checker/src/checkers/` — where diagnostics are triggered
- `crates/tsz-checker/src/assignability/` — where TS2322/TS2345 are emitted

**Strategy**:
1. Run fingerprint-only tests with `--verbose` and `--print-fingerprints`
2. For each test, count diagnostics per code in tsc vs tsz
3. Group by pattern: over-emit vs under-emit, which code, which checker path
4. Fix the emission/suppression rule to match tsc

**Research command**:
```bash
python3 scripts/conformance/query-conformance.py --fingerprint-only
# Look for tests where expected and actual have same unique codes but different counts
./scripts/conformance/conformance.sh run --filter "PATTERN" --verbose
```

---

## Tier 2: Wrong-Code Campaigns

### big3-unification (65 tests — highest leverage in wrong-code)

Route ALL assignment/call-arg/JSX-prop/destructuring/satisfies checks through
the canonical relation boundary. Delete checker-local re-derivations.

- **KPI**: wrong-code count for TS2322+TS2339+TS2345 (currently 71)
- **Current**: TS2322=27 tests (17m/10x), TS2339=23 (12m/11x), TS2345=21 (14m/7x)
- **Strategy**: Find invariants that fix BOTH missing AND extra in the same family

**Research command**:
```bash
python3 scripts/conformance/query-conformance.py --dashboard
python3 scripts/conformance/query-conformance.py --campaign big3
python3 scripts/conformance/query-conformance.py --code TS2322
```

**Entry files**:
- `crates/tsz-solver/src/subtype.rs`
- `crates/tsz-solver/src/relations/`
- `crates/tsz-checker/src/assignability/`
- `crates/tsz-checker/src/query_boundaries/assignability.rs`

### narrowing-flow (62 tests)

Finish narrowing.rs as boundary-clean. Zero direct solver calls.

- **KPI**: direct solver query calls remaining in narrowing code
- **Strategy**: Add boundary helpers for all narrowing queries

**Research command**:
```bash
python3 scripts/conformance/query-conformance.py --campaign narrowing-flow
rg "type_queries\." crates/tsz-checker/src/flow/ --type rust -c
```

### contextual-typing (55 tests)

Complete TypingRequest migration through all hot paths. Eliminate raw
contextual_type mutations from CheckerContext.

- **KPI**: count of raw contextual-state mutations outside TypingRequest
- **Hot paths**: call inference, JSX props/children, JSDoc callbacks, generic constructors

**Research command**:
```bash
python3 scripts/conformance/query-conformance.py --campaign contextual-typing
rg "contextual_type\b" crates/tsz-checker/src/ --type rust -l
```

---

## Tier 3: Subsystem & Leaf

### crash-zero (44 crashes)

Every crash is measurement corruption. Fix recursion limits, cycle breakers, declaration-emit fallbacks.

- **KPI**: crash count → zero
- **Research**: `python3 scripts/conformance/query-conformance.py --dashboard` (KPI 2)

### diagnostic-position (~90 tests)

Diagnostics point at wrong AST node. Common: pointing to outer expression instead of inner literal, off-by-one line numbers.

- **KPI**: fingerprint-only tests with position-only mismatch
- **Entry files**: `crates/tsz-checker/src/error_reporter/`, checker diagnostic emit sites
- **Strategy**: Run with `--print-fingerprints`, compare tsc line:col vs tsz line:col, fix the anchor node

### parser-recovery (45 tests)

Parser error recovery emits wrong TS1xxx code. Cascades into secondary noise.

- **KPI**: parser-recovery campaign test count
- **Research**: `python3 scripts/conformance/query-conformance.py --campaign parser-recovery`

### module-resolution (~26 real tests)

Node/module-resolution/declaration-emit coordination.

- **KPI**: Node lane pass rate (dashboard KPI 3)
- **Note**: The query tool's `--campaign module-resolution` historically over-counted by including all `compiler/` directory tests. Real module-resolution failures are ~26 tests matching Node lane patterns.

### false-positives (3 tests)

We emit errors where tsc expects 0. Almost eliminated.

### jsdoc-jsx-salsa (integration area)

Broad consumer of shared solver/checker mechanics. Most improvements come from Tier 1/2 campaigns.

---

## KPIs (Track These, Not Overall %)

| KPI | How to check | Target |
|-----|-------------|--------|
| **Fingerprint-only count** | `--dashboard` KPI 4 | Reduce from 617 |
| FP-only for TS2322+TS2345+TS2339 | `--fingerprint-only --code TSxxxx` | Reduce from 410 |
| Big3 wrong-code count | `--dashboard` KPI 1 | Reduce from 71 |
| Crash count | `--dashboard` KPI 2 | Zero (currently 44) |
| Node lane pass rate | `--dashboard` KPI 3 | >75% |
| Tests per fix commit | git log analysis | >1.0 (currently 0.32) |

---

## Finding Work

### Step 0: Start from a healthy tree

```bash
scripts/session/healthcheck.sh
git status --short --branch
git log --oneline -20

# Read campaign checkpoint
scripts/session/campaign-checkpoint.sh <your-campaign> --status || \
  scripts/session/campaign-checkpoint.sh <your-campaign> --init

# Check KPI dashboard
python3 scripts/conformance/query-conformance.py --dashboard
```

### For Fingerprint Campaigns (Tier 1)

Fingerprint work has a different methodology than wrong-code work. You are NOT looking for missing/extra error codes. You are looking for **why the error message text, position, or count differs** from tsc.

**Step 1: Select a target code and sample tests**
```bash
# See which codes have the most fingerprint-only failures
python3 scripts/conformance/query-conformance.py --fingerprint-only

# Pick a code (e.g., TS2322) and list affected tests
python3 scripts/conformance/query-conformance.py --fingerprint-only --code TS2322 --paths-only | head -15
```

**Step 2: Run with fingerprint output**
```bash
# Pick 8-12 tests and run verbose to see the diff
./scripts/conformance/conformance.sh run --filter "test1|test2|test3" --verbose
```
The verbose output shows `missing-fingerprints:` (tsc has, we don't) and `extra-fingerprints:` (we have, tsc doesn't). Compare the `message_key` and `line:column` fields.

**Step 3: Classify the mismatch**
- **Message differs, position same**: type display bug → fix the printer
- **Position differs, message similar**: diagnostic anchor bug → fix the error site
- **Count differs**: emission/suppression rule bug → fix the checker logic

**Step 4: Find the printer/emission code**
For display bugs, trace from the error message template back to the type printer call.
For position bugs, find where the diagnostic span is set.
For count bugs, find the emission guard/loop.

**Step 5: Fix and measure batch impact**
```bash
# Run ALL fingerprint-only tests for your code family
python3 scripts/conformance/query-conformance.py --fingerprint-only --code TS2322 --paths-only > /tmp/fp.txt
wc -l /tmp/fp.txt  # expect ~251 tests
./scripts/conformance/conformance.sh run --filter "$(head -30 /tmp/fp.txt | xargs -I{} basename {} .ts | tr '\n' '|' | sed 's/|$//')" --max 30
```
**A good fingerprint fix should flip 5+ tests at once.** If your fix only affects 1 test, you're probably fixing a symptom, not the pattern.

### For Wrong-Code Campaigns (Tier 2)

This is the traditional campaign methodology:

**Step 1: Research your campaign**
```bash
python3 scripts/conformance/query-conformance.py --campaign <name>
# Pick 8-15 representative tests spanning BOTH missing AND extra
./scripts/conformance/conformance.sh run --filter "<pattern>" --verbose --max 15
```

**Step 2: Find the shared invariant**
Write it down before touching code:
> "tsc does X when condition Y holds. We currently do Z instead."

For wrong-code campaigns, the invariant should explain BOTH missing AND extra diagnostics in the same family. If your invariant only explains one direction, keep researching.

**Step 3: Implement across crate boundaries**
Follow the root cause wherever it leads. Multi-crate changes are normal.

### For Leaf Fixes (Tier 3)

```bash
python3 scripts/conformance/query-conformance.py --one-extra
python3 scripts/conformance/query-conformance.py --one-missing
python3 scripts/conformance/query-conformance.py --close 2
```

Pick targets randomly to avoid duplicate work with other agents. Tier 3 agents may reroll if a target needs multi-crate changes.

---

## Verification (MANDATORY)

### Step 1: Build and test the specific fix
```bash
cargo check --package tsz-checker
cargo check --package tsz-solver
cargo build --profile dist-fast --bin tsz
./scripts/conformance/conformance.sh run --filter "TESTNAME" --verbose
```

### Step 2: Measure batch impact (especially for fingerprint fixes)
```bash
# For fingerprint work: run a broader sample to see how many tests flipped
./scripts/conformance/conformance.sh run --filter "BROAD_PATTERN" --max 50
```

### Step 3: Regression check
```bash
./scripts/conformance/conformance.sh run --max 200
```

### Step 4: Unit tests
```bash
cargo nextest run --package tsz-checker --lib 2>&1 | grep "^test result"
cargo nextest run --package tsz-solver --lib 2>&1 | grep "^test result"
```

### Step 5: Check KPI movement
```bash
python3 scripts/conformance/query-conformance.py --dashboard
```

### Step 6: Full conformance (before pushing)
```bash
scripts/safe-run.sh ./scripts/conformance/conformance.sh run 2>&1 | grep "FINAL"
```

---

## Efficiency Rules

The last 100 commits achieved **+24 tests from 75 fix commits** (0.32 tests/commit) with **35% regression churn**. This is unsustainable. Aim for **>1.0 tests per fix commit**.

**Batch, don't scatter**: One printer fix that flips 20 tests is worth more than 20 individual checker tweaks that each flip 1.

**Minimize snapshot commits**: Don't commit a snapshot after every single fix. Batch fixes, then snapshot once.

**Check for regressions before committing**: If your fix causes any test to flip from PASS to FAIL, investigate before committing — even if the net is positive. A 35% churn ratio means 1 in 3 gains gets clawed back.

**Track your efficiency**: Before pushing, count how many tests you fixed vs how many commits you made. If the ratio is below 1.0, consider whether you're doing leaf work when you should be doing campaign work.

---

## Regression Policy

- **If conformance drops > 5 tests**: DO NOT PUSH. Investigate and fix or revert.
- **If build breaks**: Fix it before doing anything else.
- **If KPI regresses**: Investigate — your invariant may be wrong or incomplete.
- **If your fix flips tests in both directions**: Investigate the regressions. A net-positive fix that causes regressions is a sign of an incomplete invariant.
- **If no progress after 2 cycles**: Stop. Re-read progress file. Switch approach.

---

## What NOT to Do

1. **Don't chase individual test failures** — find shared root causes across 8-15 tests.
2. **Don't ignore fingerprint-only failures** — they are 73.6% of remaining work.
3. **Don't add checker-local type algorithms** — use solver queries.
4. **Don't suppress diagnostics with broad conditions**.
5. **Don't reroll because fix needs multi-crate work** (Tier 1/2).
6. **Don't push if conformance dropped** — investigate first.
7. **Don't track progress by overall %** — track your campaign's KPI.
8. **Don't fix only missing OR only extra** — find invariants that fix both.
9. **Don't run full conformance for research** — use offline query tools.
10. **Don't ignore recent commits** — check `git log --oneline -20`.
11. **Don't fix symptoms in checker when root cause is in solver**.
12. **Don't commit a snapshot after every single fix** — batch them.
13. **Don't accept 1-test-per-commit efficiency** — aim for batch fixes that flip 5+.

---

## Quick Reference

```bash
# KPI dashboard
python3 scripts/conformance/query-conformance.py --dashboard

# Fingerprint analysis (THE primary work surface)
python3 scripts/conformance/query-conformance.py --fingerprint-only
python3 scripts/conformance/query-conformance.py --fingerprint-only --code TS2322

# Campaign research
python3 scripts/conformance/query-conformance.py --campaign <name>

# Offline analysis (instant)
python3 scripts/conformance/query-conformance.py
python3 scripts/conformance/query-conformance.py --campaigns
python3 scripts/conformance/query-conformance.py --code TS2322
python3 scripts/conformance/query-conformance.py --false-positives

# Build
cargo check --package tsz-checker
cargo check --package tsz-solver
cargo build --profile dist-fast --bin tsz

# Test specific (with fingerprint detail)
./scripts/conformance/conformance.sh run --filter "TESTNAME" --verbose

# Regression check
./scripts/conformance/conformance.sh run --max 200

# Full run
scripts/safe-run.sh ./scripts/conformance/conformance.sh run

# Snapshot
scripts/safe-run.sh ./scripts/conformance/conformance.sh snapshot

# Campaign coordination
scripts/session/campaign-checkpoint.sh <campaign> --status
scripts/session/campaign-checkpoint.sh <campaign>

# Push and open a PR (never push to main)
git push -u origin campaign/<your-campaign>
# Then open a pull request targeting main via the web UI or your PR tooling.
```
