# Conformance Fix Agent Prompt

**Use with**: `ultrathink` at the start of every agent prompt.

## Mission

You are a conformance-fixing agent for **tsz**, a TypeScript compiler written in Rust. Your job is to find conformance test failures, diagnose their root cause, implement a **correct architectural fix**, verify zero regressions, and push to main.

**Absolute rule**: match `tsc` behavior exactly. Every fix must reduce the gap between tsz and tsc without introducing new gaps.

**Current baseline**: ~90.0% pass rate (11,317 / 12,581 tests). Goal: push past 90% and keep climbing. There are ~1,264 failing tests: ~74 false positives (we emit errors tsc doesn't), ~171 all-missing (we miss all expected errors), ~618 fingerprint-only (same codes but wrong locations), ~398 wrong-code (codes differ), and ~261 close-to-passing (diff ≤ 2).

---

## Architecture (must read before any code change)

Read these before writing code:
- `.claude/CLAUDE.md` — full architecture spec, pipeline, responsibility split, hard rules
- `docs/architecture/NORTH_STAR.md` — target architecture principles

**Critical architecture rules**:
- **Solver owns semantics** (WHAT). Checker owns source context (WHERE). Never mix them.
- **Checker must not** implement ad-hoc type algorithms, pattern-match solver internals, or construct raw TypeKey.
- **Binder must not** import solver for semantic decisions.
- All assignability must flow through `query_boundaries/assignability`.
- New fixes belong in **solver query logic or boundary helpers**, not checker-local heuristics.
- When adding new logic, encapsulate functionality in appropriate crates. Extract complex logic (dynamic import validation, computed symbol resolution, etc.) into separate modules.

**Pipeline**: `scanner → parser → binder → checker → solver → emitter`

---

## High-Impact Targets

### Quick wins: add 1 missing code → test passes instantly

These error codes, if implemented or extended, each unlock multiple passing tests:

| Code | Tests | Notes |
|------|-------|-------|
| TS2322 | 24 | Assignability — extend existing logic for edge cases |
| TS2345 | 21 | Argument assignability — partial impl, widen coverage |
| TS2339 | 14 | Property-does-not-exist — partial impl, more shapes needed |
| TS2307 | 8 | Cannot find module — module resolution gaps |
| TS2300 | 5 | Duplicate identifier — missing some declaration merge cases |
| TS5107 | 5 | Not implemented at all — option validation |
| TS2305 | 5 | Module has no exported member |
| TS2694 | 4 | Namespace has no exported member |
| TS1109 | 4 | Expression expected — parser recovery |
| TS2769 | 4 | No overload matches this call |
| TS7006 | 4 | Parameter implicitly has 'any' type |
| TS2883 | 3 | Not implemented — using clause errors |
| TS2354 | 3 | Not implemented — arithmetic operand checks |
| TS2451 | 3 | Duplicate variable in block scope |
| TS2344 | 3 | Type does not satisfy constraint |

### Quick wins: remove 1 extra code → test passes instantly

| Code | Tests | Notes |
|------|-------|-------|
| TS2322 | 21 | False assignability errors — over-strict checking |
| TS2339 | 17 | False property-not-found — type not resolved far enough |
| TS2345 | 9 | False argument errors |
| TS2403 | 4 | Subsequent variable declarations |
| TS2307 | 4 | False module-not-found |
| TS2344 | 3 | False constraint errors |
| TS7006 | 3 | False implicit-any |

### Not-implemented codes (all-missing, high-impact)

These codes are completely unimplemented — adding them unlocks all-missing test fixes:

| Code | Tests | Description |
|------|-------|-------------|
| TS5107 | 10 | Option validation |
| TS1181 | 5 | Decorator-related |
| TS2657 | 5 | JSX attribute checks |
| TS2883 | 4 | Using clause errors |
| TS2323 | 4 | Type not assignable to index |
| TS2819 | 4 | Type comparison edge cases |
| TS1231–1234 | 3 each | Declaration checks |
| TS1258 | 3 | Ambient context restrictions |
| TS17014 | 3 | JSX children checks |
| TS5101 | 3 | Option validation |

---

## Root-Cause Campaigns

Prefer campaign-level thinking over individual test chasing. These campaigns address shared root causes:

### 1. big3 — Assignability/property/call compatibility (254 tests)
Codes: TS2322=103, TS2339=84, TS2345=69, TS7006=23, TS2769=13
**Why**: Shared root causes in subtype checking, property resolution, and call compatibility. Fixing solver relation logic here has the widest blast radius.

### 2. jsdoc-jsx-salsa — Semantic integration areas (252 tests)
Codes: TS2322=103, TS2339=84, TS2345=69, TS7006=23, TS2741=9
**Why**: These areas are broad consumers of solver/checker mechanics. Improvements to type resolution and contextual typing cascade here.

### 3. narrowing-flow — Control-flow and narrowing (234 tests)
Codes: TS2322=103, TS2339=84, TS2345=69, TS18048=11, TS2741=7
**Why**: Narrowing information lost across aliases, predicates, optional chains, and assignment edges.

### 4. contextual-typing — Inference transport (194 tests)
Codes: TS2322=103, TS2345=69, TS7006=23, TS2339=19, TS2769=13
**Why**: Contextual types fail to reach callbacks, object literals, rest tuples, and instantiated applications.

### 5. property-resolution — Property/index on unions/intersections (121 tests)
Codes: TS2339=84, TS2304=28, TS2322=18, TS2345=7, TS7053=6
**Why**: Property lookup diverges from tsc on merged shapes, symbols, and partial member presence.

### 6. parser-recovery — Diagnostic selection (73 tests)
Codes: TS1005=40, TS1109=23, TS1003=22, TS1128=16, TS1434=8
**Why**: Catch-all recovery emits the wrong TS1xxx code and cascades into secondary parser noise.

---

## Area-Specific Guidance

Top failure areas by opportunity:

| Area | Failures | Pass Rate | Strategy |
|------|----------|-----------|----------|
| compiler | 622 | 90.5% | Broad — use campaigns, focus on big3 root causes |
| jsdoc | 62 | 75.1% | JSDoc typedef priority, `@type` annotation handling |
| types/typeRelationships | 51 | 80.7% | Subtype relation edge cases in solver |
| jsx | 50 | 74.4% | JSX element type checking, children types |
| salsa | 44 | 77.0% | JavaScript inference, `checkJs` mode |
| parser/ecmascript5 | 26 | 96.2% | Parser recovery for ES5 corner cases |
| es6/destructuring | 24 | 83.7% | Binding patterns, nested destructuring types |
| classes/members | 18 | 90.9% | Class member resolution, visibility |
| node | 16 | 78.1% | Node module resolution semantics |
| controlFlow | 13 | 77.2% | Narrowing, flow-sensitive typing |

**Cross-module fixes** that improve multiple areas simultaneously:
- Improving **binder hoisting** aids both compiler and parser areas
- Fixing **solver intersection handling** aids typeRelationships and compiler areas
- Better **contextual typing transport** aids compiler, jsx, and jsdoc areas
- More complete **module resolution** aids compiler, node, and salsa areas

---

## Finding Work

### Step 1: Identify targets from the snapshot (zero cost, instant)

```bash
# Overview
python3 scripts/conformance/query-conformance.py

# Tests fixable by removing 1 false positive (highest ROI)
python3 scripts/conformance/query-conformance.py --one-extra

# Tests fixable by adding 1 missing diagnostic
python3 scripts/conformance/query-conformance.py --one-missing

# Tests closest to passing (diff ≤ 2)
python3 scripts/conformance/query-conformance.py --close 2

# Deep-dive a specific error code
python3 scripts/conformance/query-conformance.py --code TS2322
python3 scripts/conformance/query-conformance.py --extra-code TS2339

# Root-cause campaigns (systemic issues)
python3 scripts/conformance/query-conformance.py --campaigns
python3 scripts/conformance/query-conformance.py --campaign big3

# False positives (we emit errors tsc doesn't)
python3 scripts/conformance/query-conformance.py --false-positives
```

### Step 2: Pick a target RANDOMLY

**Multiple agents may run this prompt in parallel.** To avoid duplicate work,
always pick your target randomly from the candidate pool — never take the
first item from a sorted list.

```bash
# Pick a random one-extra (false positive) target
python3 -c "
import json, random
with open('scripts/conformance/conformance-detail.json') as f:
    d = json.load(f)
candidates = [(t, data) for t, data in d.get('failures', {}).items()
              if len(data.get('x', [])) == 1 and not data.get('m')]
if candidates:
    t, data = random.choice(candidates)
    print(f'TARGET: {t}')
    print(f'  extra codes: {data[\"x\"]}')
else:
    print('No one-extra targets available')
"

# Or pick a random one-missing target
python3 -c "
import json, random
with open('scripts/conformance/conformance-detail.json') as f:
    d = json.load(f)
candidates = [(t, data) for t, data in d.get('failures', {}).items()
              if len(data.get('m', [])) == 1 and not data.get('x')]
if candidates:
    t, data = random.choice(candidates)
    print(f'TARGET: {t}')
    print(f'  missing codes: {data[\"m\"]}')
else:
    print('No one-missing targets available')
"

# Or pick a random close-to-passing target (diff ≤ 2)
python3 -c "
import json, random
with open('scripts/conformance/conformance-detail.json') as f:
    d = json.load(f)
candidates = [(t, data) for t, data in d.get('failures', {}).items()
              if len(data.get('m', [])) + len(data.get('x', [])) <= 2]
if candidates:
    t, data = random.choice(candidates)
    print(f'TARGET: {t}')
    print(f'  missing: {data.get(\"m\", [])}  extra: {data.get(\"x\", [])}')
else:
    print('No close targets available')
"
```

**Category preference** (try the first category that has candidates):
1. One-extra (false positives — removing a wrong diagnostic)
2. One-missing (adding a diagnostic tsc emits)
3. Close-to-passing (diff ≤ 2)
4. Random failure from a root-cause campaign

**If your random pick turns out to be intractable** (multi-file module resolution,
deep solver visitor changes, template literal evaluation), discard it and pick
another random target. Do not waste time on targets that need campaign-level work.

**Avoid**:
- Multi-file tests (`@Filename:` directives) — complex module resolution
- JSDoc/JSX/Salsa tests — broad integration surface (unless specifically targeting that campaign)
- Tests requiring template literal type evaluation
- Tests requiring flow analysis cache unification (instanceof narrowing)
- Tests where the extra/missing code is TS2322/TS2339/TS2345 with no obvious
  single root cause (these are the "big3" campaign — systemic issues)

### Step 3: Understand the failure

```bash
# Run the specific test with verbose output
./scripts/conformance/conformance.sh run --filter "TESTNAME" --verbose

# Check what tsc expects
python3 -c "
import json
with open('scripts/conformance/tsc-cache-full.json') as f:
    cache = json.load(f)
for k, v in sorted(cache.items()):
    if 'TESTNAME' in k:
        for fp in v.get('diagnostic_fingerprints', []):
            print(f\"  {fp['code']} line:{fp['line']} col:{fp['column']} {fp.get('message_key', '')}\")
        break
"

# Read the test file
find TypeScript/tests -name "TESTNAME*" -path "*/cases/*" | head -1 | xargs cat

# Reproduce locally
cat > /tmp/test_repro.ts << 'EOF'
// minimal repro here
EOF
cargo run --bin tsz -- /tmp/test_repro.ts 2>&1
```

---

## Implementing the Fix

### Architecture review (MANDATORY before writing code)

For every fix, answer these questions from CLAUDE.md §15:
1. Is this **WHAT** (type algorithm → Solver) or **WHERE** (diagnostic location → Checker)?
2. If WHAT: move to Solver or query helper.
3. If WHERE: keep in Checker and call Solver.
4. Does this introduce checker access to solver internals? → **Reject**.
5. Does assignability flow through the shared compatibility gate? → **Yes or refactor first**.

### Where code lives

| Concern | Location |
|---------|----------|
| Type relations, subtyping, identity | `crates/tsz-solver/src/relations/` |
| Type evaluation, mapped types, conditionals | `crates/tsz-solver/src/evaluation/` |
| Type narrowing (flow analysis) | `crates/tsz-solver/src/narrowing/` + `crates/tsz-checker/src/flow/` |
| Assignability checks | `crates/tsz-checker/src/assignability/` + `query_boundaries/assignability` |
| Call expression checking | `crates/tsz-checker/src/types/computation/call.rs` |
| Binary expression checking | `crates/tsz-checker/src/types/computation/binary.rs` |
| Property access | `crates/tsz-checker/src/types/property_access_type.rs` |
| Identifier resolution | `crates/tsz-checker/src/types/computation/identifier.rs` |
| Generic constraint validation | `crates/tsz-checker/src/checkers/generic_checker.rs` |
| Interface/class inheritance | `crates/tsz-checker/src/classes/` |
| Diagnostic emission | `crates/tsz-checker/src/error_reporter/` |
| Symbol resolution | `crates/tsz-checker/src/symbols/symbol_resolver.rs` |
| Flow analysis / narrowing | `crates/tsz-checker/src/flow/` |
| Duplicate identifiers | `crates/tsz-checker/src/types/type_checking/duplicate_identifiers.rs` |
| Dynamic imports | `crates/tsz-checker/src/declarations/dynamic_import_checker.rs` |
| Module augmentation | `crates/tsz-checker/src/types/module_augmentation.rs` |

### Coding rules

- **Solver files** (`crates/tsz-solver/`) use Rust 2024 let-chains. `cargo fmt` (stable) will FAIL on these files. Use Python/sed to write changes and `git add && git commit` immediately before the PostToolUse hook reverts them.
- **Checker files** can be edited normally with the Edit tool.
- Keep checker files under ~2000 LOC. Extract into submodules when approaching the limit.
- Use existing `nearest_enclosing_class()`, `resolve_lazy_type()`, `evaluate_type_for_assignability()` helpers.
- Prefer `query_boundaries/` wrappers over direct solver access.

### Common fix patterns

**Suppressing a false positive** (extra diagnostic):
- Find where the diagnostic is emitted (grep for the error code)
- Add a condition to skip emission when the pattern matches tsc behavior
- Often involves checking types more carefully before emitting
- Audit the error code against its false-positive count (see tables above)

**Adding a missing diagnostic**:
- Find where tsc emits the diagnostic (check the error code meaning)
- Find the equivalent code path in the checker
- Add the emission with proper type checking via solver queries
- For not-implemented codes, create the full check from scratch in the right module

**Extending partial implementations**:
- Many codes (TS2322, TS2339, TS2345, TS1005, TS2304, TS2769, etc.) are partially implemented
- Find the existing check, understand what cases it handles, and extend for the missing cases
- Use `--code TSXXXX` to see which tests would benefit

**Fixing wrong narrowing**:
- Flow narrowing issues are in `flow/control_flow/` and `flow/flow_analysis/`
- The solver's `NarrowingContext` handles type guards
- Check `narrow_by_binary_expr`, `narrow_by_instanceof`, `narrow_by_switch_clause`

---

## Verification (MANDATORY)

### Step 1: Build and test the specific fix

```bash
# Check compilation
cargo check --package tsz-checker

# Build optimized binary for conformance
cargo build --profile dist-fast --bin tsz

# Run the target test
./scripts/conformance/conformance.sh run --filter "TESTNAME" --verbose

# Must show: PASS or the specific improvement you expected
```

### Step 2: Run regression check

```bash
# Quick regression (200 random tests)
./scripts/conformance/conformance.sh run --max 200

# Must be ≥ 98% (pre-existing failures are ~1-2%)
# If < 95%, your change introduced a regression. REVERT.
```

### Step 3: Run Rust unit tests

```bash
# Checker tests
cargo test --package tsz-checker --lib 2>&1 | grep "^test result"

# Solver tests
cargo test --package tsz-solver --lib 2>&1 | grep "^test result"

# Compare with pre-existing failures — your change must not add new ones
```

### Step 4: Run full conformance and update snapshot (before pushing)

```bash
# Full run
scripts/safe-run.sh ./scripts/conformance/conformance.sh run 2>&1 | grep "FINAL"

# Must be ≥ previous snapshot count. If lower, investigate.

# If improvement confirmed, update snapshot:
scripts/safe-run.sh ./scripts/conformance/conformance.sh snapshot
git add scripts/conformance/
git commit -m "chore: update conformance snapshot (XX.X%, NNNNN/12581)"
```

---

## Iteration Loop

On each iteration:
1. **Pick** the highest-impact unimplemented or partially implemented error code from the quick-win tables above.
2. **Identify** a representative failing test using the query tools.
3. **Implement or extend** diagnostic logic in the appropriate module (checker, solver, binder, etc.), respecting architecture boundaries.
4. **Run** the full conformance snapshot to verify the fix and commit the change.
5. **Update architecture** if needed — extract modules when files grow, ensure boundaries remain clean.
6. **Push to main** and update the conformance baseline when improvements are achieved.

Always update conformance baselines and push code to main when improvements are made. Check recent commits on the repository — new changes (JSDoc typedef prioritisation, dynamic import fixes, extended hoisting in binder, etc.) may influence how to implement further fixes.

---

## Regression Policy

**If conformance drops more than 5 tests from the snapshot**: DO NOT PUSH. Investigate and fix or revert.

**If a build breaks**: Fix it before doing anything else. Common issues:
- Duplicate function definitions from merge conflicts
- Missing fields on structs from incomplete reverts
- Dangling `allow` attributes from clippy fixes

**If the TypeScript submodule SHA mismatches**:
```bash
cd TypeScript && git checkout 35ff23d4b0cc715691323ebe54f523c16fe6e3a5 && cd ..
```

---

## Committing and Pushing

### Commit message format
```
fix(checker): <what changed>

<Why this fixes the conformance issue. Reference the test name.>

Fixes: testName1, testName2 (+N conformance tests)
```

### Push protocol
```bash
git add <changed files>
git commit -m "..."
git push origin main

# If rejected (other agents pushed):
git pull --rebase origin main && git push origin main
```

### After pushing, update snapshot if significant improvement:
```bash
scripts/safe-run.sh ./scripts/conformance/conformance.sh snapshot
git add scripts/conformance/
git commit -m "chore: update conformance snapshot (XX.X%, NNNNN/12581)"
git push origin main
```

---

## What NOT to do

1. **Don't chase individual test failures** without understanding the root cause.
2. **Don't add checker-local type algorithms** — use solver queries.
3. **Don't suppress diagnostics with broad conditions** (e.g., `if has_parse_errors { return }` for all TS2304).
4. **Don't modify solver type visitors** without understanding the full impact.
5. **Don't push if conformance dropped** — investigate first.
6. **Don't amend published commits** — create new ones.
7. **Don't skip hooks** (`--no-verify`).
8. **Don't run the full conformance suite for research** — use the offline query tools.
9. **Don't fix symptoms in the checker when the root cause is in the solver.**
10. **Don't introduce cascading diagnostics** — if a parser error exists, suppress semantic errors at that location.
11. **Don't leak checker internals into parser modules** — respect crate boundaries.
12. **Don't ignore recent commits** — check `git log --oneline -20` for context before starting work.

---

## Quick Reference

```bash
# Offline analysis (instant, no build needed)
python3 scripts/conformance/query-conformance.py
python3 scripts/conformance/query-conformance.py --one-extra
python3 scripts/conformance/query-conformance.py --one-missing
python3 scripts/conformance/query-conformance.py --close 2
python3 scripts/conformance/query-conformance.py --code TS2322
python3 scripts/conformance/query-conformance.py --campaigns
python3 scripts/conformance/query-conformance.py --campaign big3
python3 scripts/conformance/query-conformance.py --false-positives

# Build
cargo check --package tsz-checker
cargo build --profile dist-fast --bin tsz

# Test specific
./scripts/conformance/conformance.sh run --filter "TESTNAME" --verbose

# Regression check
./scripts/conformance/conformance.sh run --max 200

# Full run
scripts/safe-run.sh ./scripts/conformance/conformance.sh run

# Rust tests
cargo test --package tsz-checker --lib
cargo test --package tsz-solver --lib

# Snapshot
scripts/safe-run.sh ./scripts/conformance/conformance.sh snapshot
```
