# Slice 1 Conformance Test Action Plan

**Current Status**: 68.2% passing (2,142/3,139)
**Target**: 70%+ (need 60+ test improvements)
**Date**: 2026-02-12

## Immediate Actions (Session Continuation)

### 1. Fix Build Environment (5 min)
```bash
# Clear any locked builds
cargo clean
#Build tsz-cli
cargo build --release -p tsz-cli

# Verify binary location
find target -name tsz -type f

# Test with simple file
./target/release/tsz tmp/array_find_test.ts
```

### 2. Debug Type Guard Predicate Issue (30-60 min)

**Test case**: `tmp/array_find_test.ts` (already created)

**Add tracing** to `crates/tsz-solver/src/operations.rs`:

```rust
// At line 2593, add tracing before type predicate constraint
if let (Some(s_pred), Some(t_pred)) = (&source.type_predicate, &target.type_predicate) {
    trace!(
        source_pred = ?s_pred,
        target_pred = ?t_pred,
        "Checking type predicate compatibility"
    );
    if let (Some(s_pred_type), Some(t_pred_type)) = (s_pred.type_id, t_pred.type_id) {
        trace!(
            s_pred_type = ?s_pred_type,
            t_pred_type = ?t_pred_type,
            "Adding type predicate constraint"
        );
        self.constrain_types(ctx, var_map, s_pred_type, t_pred_type, priority);
    }
}

// At line 907, add tracing for type parameter resolution
for (tp, &var) in func.type_params.iter().zip(type_param_vars.iter()) {
    let constraints = infer_ctx.get_constraints(var);
    trace!(
        type_param = ?tp.name,
        var = ?var,
        has_constraints = constraints.is_some_and(|c| !c.is_empty()),
        constraints = ?constraints,
        "Resolving type parameter"
    );
    // ... rest of code
}
```

**Run with tracing**:
```bash
TSZ_LOG="tsz_solver::operations=trace" TSZ_LOG_FORMAT=tree \
  cargo run --release -- tmp/array_find_test.ts 2>&1 | tee tmp/trace.log
```

**Expected findings**:
- Either type predicate constraint is NOT being added (both functions don't have predicates)
- Or constraint IS added but resolution fails/falls back to constraint

### 3. Implement Fix Based on Findings

**Scenario A**: Type predicate not being added
- Check if target function type has type predicate after instantiation
- Ensure `instantiate_type` preserves type predicates properly
- Fix in `crates/tsz-solver/src/instantiate.rs`

**Scenario B**: Constraint resolution failing
- Check why `resolve_with_constraints_by` fails
- Might need special handling for type guard lower bounds
- Fix in `crates/tsz-solver/src/infer.rs` or `operations.rs`

**Scenario C**: No constraints collected
- Verify function-to-function constraints are being collected
- Check if `constrain_function_to_call_signature` is being called
- Add missing call site if needed

### 4. Verify Fix (10 min)
```bash
# Run the specific test
./target/release/tsz TypeScript/tests/cases/compiler/arrayFind.ts

# Should have no errors (tsc has 0 errors on this file)

# Run unit tests
cargo nextest run --release -p tsz-solver

# Run conformance slice
./scripts/conformance.sh run --offset 0 --max 100  # Small sample first
```

## Alternative High-Impact Fixes (If Type Guard Is Too Complex)

If the type guard issue takes > 2 hours, pivot to these quicker wins:

### Option 1: TS2740 Missing Properties Error (15 tests, LOW effort)
**File**: `crates/tsz-checker/src/diagnostics.rs`

Currently we only emit TS2740 when 5+ properties are missing. TSC emits it more liberally.

**Fix**: Change threshold from 5 to 2-3, or emit both TS2322 AND TS2740 in some cases.

### Option 2: Fix Specific False Positives (HIGH impact)
Pick 5-10 false positive tests and debug individually:
```bash
# Get list of false positives
./scripts/conformance.sh analyze --offset 0 --max 3146 --category false-positive | head -20

# Debug each one
for test in aliasUsageInArray.ts aliasUsageInGenericFunction.ts; do
  echo "=== $test ==="
  ./target/release/tsz "TypeScript/tests/cases/compiler/$test"
  tsc "TypeScript/tests/cases/compiler/$test"
done
```

Common false positive patterns:
- Module import/export compatibility issues
- Type alias in generic constraints
- Object literal excess property checks

### Option 3: Implement Missing Error Codes (MEDIUM effort, 20+ tests)
Focus on high-frequency missing codes:
- TS2304 (9 tests): "Cannot find name"
- TS2339 (8 tests): "Property does not exist"
- TS2353 (7 tests): "Object literal may only specify known properties"

## Testing Strategy

### Incremental Validation
After each fix:
```bash
# 1. Run affected unit tests
cargo nextest run --release -p tsz-solver <test_pattern>

# 2. Run small conformance sample (faster feedback)
./scripts/conformance.sh run --offset 0 --max 500

# 3. If sample looks good, run full slice
./scripts/conformance.sh run --offset 0 --max 3146

# 4. Commit and push immediately
git add <changed_files>
git commit -m "fix: <description>"
git push
```

### Commit Strategy
- Commit after EACH passing fix (even if small)
- Push immediately after each commit
- Don't wait to batch fixes

## Success Criteria

- **Minimum**: 70% pass rate (+60 tests)
- **Good**: 72% pass rate (+125 tests)
- **Excellent**: 75% pass rate (+215 tests)

## Time Budget

Total session time: ~4 hours

- Setup & environment: 30 min
- Type guard investigation: 90 min
- Alternative fixes (if needed): 120 min
- Testing & validation: 30 min
- Documentation: 10 min

## Blockers & Risks

1. **Build environment issues** - Cargo locks, compilation errors
   - Mitigation: Use `cargo clean`, check for runaway processes

2. **Type guard fix too complex** - Might need deeper refactoring
   - Mitigation: Pivot to alternative fixes after 2 hours

3. **Fix causes regressions** - Other tests start failing
   - Mitigation: Run full test suite before committing

4. **Conformance cache issues** - TSC cache gets corrupted
   - Mitigation: Use `./scripts/reset-ts-submodule.sh` if needed

## Resources

- Main analysis: `docs/conformance_analysis_slice1.md`
- Type guard deep dive: `docs/bugs/type-guard-predicate-inference.md`
- Tracing guide: `.claude/skills/tsz-tracing/SKILL.md`
- Architecture: `docs/architecture/NORTH_STAR.md`
