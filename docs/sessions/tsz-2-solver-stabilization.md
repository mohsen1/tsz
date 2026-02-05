# Session tsz-2: Solver Stabilization

**Started**: 2026-02-05 (redefined from Application expansion session)
**Status**: Active
**Goal**: Reduce failing solver tests from 31 to zero

## Context

Original tsz-2 session (Application expansion) was completed successfully. This session is now focused on solver test stabilization.

**Recent Progress** (commit ea1029cf3):
- ✅ Fixed function contravariance in strict mode (AnyPropagationMode::TopLevelOnly)
- ✅ Fixed interface lowering (Object vs ObjectWithIndex)
- Reduced test failures from 37 → 31

## Current Focus (2026-02-05 Redefined by Gemini Pro)

### Primary Focus: Generic Inference (16 tests)

**Attack Strategy**: Stop "trying fixes" and start "tracing execution"

**Next Steps**:
1. Wait for disk cleanup to finish (cargo clean running in background)
2. Trace the simplest failing test: `test_infer_generic_array_map`
   ```bash
   TSZ_LOG="wasm::solver::infer=trace,wasm::solver::instantiate=debug" \
   TSZ_LOG_FORMAT=tree \
   cargo nextest run test_infer_generic_array_map --nocapture 2>&1 | head -n 300
   ```
3. Ask Gemini Pro with trace data:
   ```bash
   ./scripts/ask-gemini.mjs --pro --include=src/solver/infer.rs \
   "I am debugging 'test_infer_generic_array_map'.
   The test fails because it returns TypeId(115) instead of the expected type.
   Here is the trace output: [PASTE TRACE]
   1) Why is the inference failing to narrow down to the specific type?
   2) Is the issue in candidate collection or final type resolution?
   3) What specific function needs to be adjusted?"
   ```
4. Implement the fix based on Gemini's guidance
5. Verify if this fixes the other 15 generic tests

### Secondary Focus: Intersection Normalization (5 tests)
**Fallback if Generic Inference takes > 1 hour**

**Problem**: `null & object` should reduce to `never`

**Gemini Question** (Pre-implementation):
```bash
./scripts/ask-gemini.mjs --include=src/solver/operations.rs --include=src/solver/intern.rs \
"I need to fix intersection normalization.
Problem: 'null & object' is not reducing to 'never'.
1. Where is the canonical place to add reduction rules?
2. Does TypeScript handle this via the Lawyer layer or the Judge layer?
3. Please show the correct pattern."
```

---

## Original Status (31 Failing Solver Tests)

### Priority 1: Generic Inference Deep Dive (16 tests) - 🔴 CRITICAL
**Tests**:
- `test_infer_generic_array_map`
- `test_infer_generic_callable_param_from_function`
- `test_infer_generic_callable_param_from_callable`
- And 13 others...

**Attempted Fix**: Added `strengthen_constraints()` in `resolve_generic_call_inner` - didn't work

**Root Cause Investigation**: Need to trace with tsz-tracing to understand why TypeId resolution is wrong

**Files**:
- `src/solver/operations.rs` (resolve_generic_call_inner, lines 843-891)
- `src/solver/infer.rs` (InferenceContext)

**Action Plan**:
1. Pick one simple failing test
2. Use tracing: `TSZ_LOG="wasm::solver::infer=trace,wasm::solver::operations=debug" cargo test test_infer_generic_array_map`
3. Identify: Are candidates being found? Is inference failing to unknown? Is instantiate wrong?
4. Ask Gemini Pro with trace data

### Priority 2: Weak Type Detection (2 tests) - 🟡 PRE-EXISTING
**Tests**:
- `test_weak_union_rejects_no_common_properties`
- `test_weak_union_with_non_weak_member_not_weak`

**Status**: Pre-existing failures, NOT a regression from commit ea1029cf3

**Issue**: `explain_failure` returns `None` instead of `TypeMismatch`

**Files**:
- `src/solver/compat.rs`
- `src/solver/lawyer.rs`

### Priority 3: Intersection Normalization (5 tests) - 🟢 PENDING
**Tests**:
- `test_intersection_null_with_object_is_never`
- `test_intersection_undefined_with_object_is_never`
- And 3 others...

**Issue**: `null & object` should reduce to `never` but doesn't

**Files**:
- `src/solver/operations.rs` (intersection factory function)

### Other Failing Tests (8 tests)
- Constraint resolution (2 tests)
- Narrowing (1 test)
- Conditional types (1 test)
- Generic fallback (1 test)
- Property intersection (1 test)
- Integration tests (2 tests)

## MANDATORY: Two-Question Rule

For ALL changes to `src/solver/` or `src/checker/`:

1. **Question 1** (Pre-implementation): Ask Gemini for approach validation
2. **Question 2** (Post-implementation): Ask Gemini Pro to review

Evidence from investigation: 100% of unreviewed solver/checker changes had critical type system bugs.

## Session History

*2026-02-05*: Redefined from Application expansion session to Solver Stabilization after Gemini consultation.
