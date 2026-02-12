# Conformance Tests 100-199 Mission Plan

## Mission
Maximize pass rate for conformance tests at offset 100 (tests 100-199).

## Current Blockers
1. **Build Infrastructure** - cargo builds keep getting killed with SIGKILL due to memory exhaustion
2. **Fix Applied** - `crates/tsz-binder/src/state.rs` parameter name fixed (`_modules_with_export_equals` → `modules_with_export_equals`)
3. **Status** - Once builds complete, need to establish baseline pass rate

## Workflow (Once Builds Work)

### 1. Establish Baseline
```bash
./scripts/conformance.sh run --max=100 --offset=100
```

### 2. Analyze Failures
```bash
# See high-level breakdown
./scripts/conformance.sh analyze --max=100 --offset=100

# Focus on easiest wins
./scripts/conformance.sh analyze --max=100 --offset=100 --category close

# See false positives (we emit errors TSC doesn't)
./scripts/conformance.sh analyze --max=100 --offset=100 --category false-positive

# See missing errors (TSC emits, we don't)
./scripts/conformance.sh analyze --max=100 --offset=100 --category all-missing
```

### 3. Pick High-Impact Fixes
Priority order:
1. **"close" tests** - differ by 1-2 errors only (easiest wins)
2. **False positives** - we're too strict
3. **Missing errors** - we're too permissive  
4. **Wrong codes** - we emit wrong error codes

### 4. Fix Strategy
For each issue:
1. Create minimal reproduction in `tmp/test.ts`
2. Run: `./target/dist-fast/tsz tmp/test.ts`
3. Compare with TSC behavior
4. Fix checker/solver/binder code
5. Verify with: `cargo nextest run`
6. Re-run conformance tests
7. Commit and sync:
   ```bash
   git commit -m "fix: <description>"
   git pull --rebase origin main
   git push origin main
   ```

## Key Learnings from Other Slices

### From Slice 2 (docs/conformance/slice2-final-status.md)
- **Generic Type Inference from Array Literals** (~50+ tests)
  - Problem: `["aa", "bb"]` infers as `string[]` instead of `("aa" | "bb")[]`
  - Root cause: `widen_literals()` always widens
  - Files: `crates/tsz-solver/src/expression_ops.rs`

- **Mapped Type Property Resolution** (~50+ tests)
  - Problem: `Record<K, T>` assignability issues
  - Files: `crates/tsz-solver/src/evaluate_rules/mapped.rs`

- **esModuleInterop Validation** (~50-80 tests)
  - Problem: Should emit TS2497/TS2598 for `export =` with wrong import style
  - Missing: `has_export_assignment()` method

### General Patterns
- **False Positives** usually indicate:
  - Missing `any` type propagation
  - Over-strict assignability checks
  - Missing index signature handling
  
- **Missing Errors** usually indicate:
  - Unimplemented validation checks
  - Missing error codes in diagnostics
  
- **Wrong Codes** usually indicate:
  - Error emitted in wrong validation phase
  - Incorrect error categorization

## Code Locations
- Checker: `crates/tsz-checker/src/checker/`
- Solver: `crates/tsz-solver/src/`
- Type computation: `crates/tsz-checker/src/checker/type_computation_complex.rs`
- Subtype checks: `crates/tsz-solver/src/subtype.rs`
- Diagnostics: `crates/tsz-common/src/diagnostics.rs`

## Success Criteria
- **100% pass rate for tests 100-199**
- **No regressions** in unit tests (`cargo nextest run` must pass)
- **No regressions** in other conformance test slices
- All changes committed and synced after each fix

## Next Steps (Post-Build)
1. Run baseline: `./scripts/conformance.sh run --max=100 --offset=100`
2. Document baseline pass rate
3. Run analyze command to identify top issues
4. Start with "close" category for quick wins
5. Progress through priority list systematically
