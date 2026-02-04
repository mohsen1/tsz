# Session tsz-2 - Assignability & Solver Fixes

## WORK IS NEVER DONE UNTIL ALL TESTS PASS
Work is never done until all tests pass. This includes:
- Unit tests (`cargo nextest run`)
- Conformance tests (`./scripts/conformance.sh`)
- No existing `#[ignore]` tests
- No cleanup work left undone
- No large files (>3000 lines) left unaddressed

## Status: ACTIVE - Assignability (TS2322) Conformance Fixes

**Date**: 2025-02-04
**Focus**: TS2322 (Type 'X' is not assignable to type 'Y') and related solver issues

### Current Conformance Stats (5k samples)

**Top Error Code Mismatches:**
- **TS2322: missing=99, extra=227** ← PRIMARY FOCUS
- TS2664: missing=99 (Invalid module name in augmentation)
- TS2300: missing=25, extra=4 (Duplicate identifier)
- TS2339: missing=20, extra=7 (Property doesn't exist)
- TS2304: missing=11, extra=9 (Cannot find name)

### Strategy: Focus on Extra Errors (False Positives) First

Per Gemini recommendation, prioritize **227 extra errors** over 99 missing errors:

1. **Extra errors** block valid TypeScript code from compiling
2. Usually indicate **missing logic** (e.g., "I don't know how to relate Union A to Union B")
3. Fixing one root cause often fixes **dozens of tests at once**
4. Missing errors (false negatives) mean we're too permissive (bad for safety, but doesn't block compilation)

## Architecture: Lawyer vs Judge Model

The solver uses a two-layer assignability system:

### Layer 1: The Lawyer (`src/solver/compat.rs`)
**Entry point**: `is_assignable_to()`
**Responsibility**: Handle TypeScript quirks
- `any` propagation
- `null`/`undefined` legacy rules
- Weak type checking
- Error type handling

**Common bugs**: Returning `false` immediately because it didn't apply a loose TS rule

### Layer 2: The Judge (`src/solver/subtype.rs`)
**Entry point**: `is_subtype_of()`
**Responsibility**: Strict structural checking
- Object property matching
- Union/intersection distributivity
- Generic compatibility
- Function subtype rules

**Common bugs**: Fails to match complex structures or recursion limits

### Layer 3: Diagnostics (`src/solver/diagnostics.rs`)
**Responsibility**: Explain *why* it failed

**Common bugs**: The check returns `false`, but diagnostic generation crashes or produces wrong message

## Investigation Workflow: The "Golden Loop"

### Step 1: Pick One Failing Test
Don't try to fix "TS2322" generally. Pick **one** simple failing conformance test.

Example:
```typescript
// Find a test case with TS2322 extra error
// Create debug.ts with the failing code
```

### Step 2: Trace with Logging
Run with tracing enabled:
```bash
TSZ_LOG=debug TSZ_LOG_FORMAT=tree cargo run -- debug.ts
```

Look for:
- `check_subtype` calls
- `is_assignable_to` calls
- The exact pair of types (Source, Target) that returned `false`

### Step 3: Determine Failure Layer

**Scenario A: Failed in `compat.rs` (Lawyer)**
- Did it fail to handle `any`?
- Did it fail a "Weak Type" check?
- Action: Modify `src/solver/compat.rs`

**Scenario B: Failed in `subtype.rs` (Judge)**
- Did it fail on Unions? → Check `src/solver/subtype_rules/unions.rs`
- Did it fail on Object properties? → Check `src/solver/subtype_rules/objects.rs`
- Did it fail on Generics? → Check `src/solver/subtype_rules/generics.rs`

## Common Patterns to Check First

### 1. Object Literal Freshness
**Files**: `src/solver/freshness.rs`, `src/solver/subtype_rules/objects.rs`

**Issue**: TS2322 often triggers on "Excess property checks"

**Check**: Is the source type marked with `ObjectFlags::FRESH_LITERAL`?
- If yes, solver enforces strict property matching
- If test expects loose matching, freshness flag might be sticky when it shouldn't be

### 2. Union/Intersection Distributivity
**File**: `src/solver/subtype_rules/unions.rs`

**Issues**:
- `(A | B)` assignable to `C`? → Must verify `A <: C` AND `B <: C`
- `A` assignable to `(B | C)`? → Must verify `A <: B` OR `A <: C`

**Common bug**: One branch fails, so whole assignment fails

### 3. Optional Properties
**File**: `src/solver/subtype_rules/objects.rs`

**Issue**: `{ a: number }` assignable to `{ a?: number }`?

**Check**: `check_property_compatibility` - ensure `exact_optional_property_types` isn't accidentally enabled

### 4. Index Signatures
**File**: `src/solver/subtype_rules/objects.rs`

**Issue**: Assigning object with specific keys to type with index signature

**Check**: `check_object_subtype` - verifies all properties match index signature

## Files to Audit (In Priority Order)

### 1. `src/solver/compat.rs`
- Look at `is_assignable_impl` - high-level logic
- Check `check_assignable_fast_path` - rejecting valid types too early?
- Common issues: `any`, `error`, `undefined` handling

### 2. `src/solver/subtype_rules/objects.rs`
- Look at `check_object_subtype` and `check_property_compatibility`
- Most TS2322 errors come from here
- Check: Freshness, optional properties, index signatures

### 3. `src/solver/subtype_rules/unions.rs`
- Check distributivity logic
- Verify AND/OR conditions for union subtyping

### 4. `src/solver/diagnostics.rs`
- Look at `SubtypeFailureReason::to_diagnostic`
- Sometimes error IS detected, but message is wrong
- Test runner thinks it's a mismatch because text doesn't match TSC

## Session Coordination

**Other Sessions** (no conflicts):
- **tsz-1**: Parse errors (TS1005, TS1109, TS1202, TS2695, TS2304, TS2300)
- **tsz-3**: Parser/binder fixes (ClassDeclaration26), const type parameters
- **tsz-4**: Declaration emit (.d.ts file generation)

**tsz-2 focus**: Assignability (TS2322) + solver-related issues

## Completed Work

### 1. TS2664 (Invalid module name in augmentation) ✅
**Date**: 2025-02-04

**Root cause**: `is_external_module` lost when binders recreated for type checking

**Solution**: Store `is_external_module` per-file in `BindResult` → `BoundFile` → `CheckerContext`

**Files**: `src/parallel.rs`, `src/cli/driver.rs`, `src/checker/context.rs`, `src/checker/declarations.rs`

**Result**: TS2664 now emits correctly for non-existent module augmentations

### 2. TS2322 Bivariance Fix ✅
**Date**: 2025-02-04

**Root cause**: Object literal methods marked `is_method=false` instead of `true`

**Solution**: Changed to `is_method: true` for bivariant parameter checking

**File**: `src/checker/type_computation.rs:1535`

**Rationale**: Per TS_UNSOUNDNESS_CATALOG.md item #2, methods are bivariant in TS

### 3. Accessor Type Compatibility Fix ✅
**Date**: 2025-02-04

**Root causes**:
1. **Nominal typing for empty classes**: Empty classes A and B both got `Object(ObjectShapeId(0))`
2. **Type annotation resolution**: Class references in type position resolved to constructor types

**Solution**:
- Set `symbol` field in `ObjectShape` for ALL class instance types
- Added `resolve_type_annotation()` helper to extract instance type

**Files**: `src/checker/class_type.rs`, `src/checker/type_checking_queries.rs`

## Known Issues (Pre-existing)

### Abstract Constructor Assignability (BLOCKED)
**Test**: `test_abstract_constructor_assignability`
**Issue**: Shows Object prototype type instead of class type
**Error**: `Type '{ isPrototypeOf, propertyIsEnumerable, ... }' is not assignable to type 'Animal'`
**Root cause**: `typeof AbstractClass` returns Object prototype instead of constructor type
**Status**: Requires deeper tracing of type resolution path

## Next Steps

1. [ ] Run conformance to find specific TS2322 failing tests
2. [ ] Pick ONE simple "extra error" test case
3. [ ] Run with `TSZ_LOG=debug` to trace failure
4. [ ] Identify if `compat.rs` or `subtype.rs` rejected it
5. [ ] Fix the root cause
6. [ ] Verify fix, repeat

## Commands

```bash
# Run conformance tests
./scripts/conformance.sh

# Run with tracing for debugging
TSZ_LOG=debug TSZ_LOG_FORMAT=tree cargo run -- file.ts

# Run unit tests
cargo nextest run

# Check session files
ls docs/sessions/
```

## Commits

```
dcebfa46b docs: verify TS2339 working for basic cases
262592567 docs: verify TS2300 and TS2664 are working correctly
909314213 docs: update conformance results (500-test sample)
8eabb0153 docs: update tsz-2 session with bivariance fix
b4052c0fc fix: object literal methods should use bivariant parameter checking
3c8a2adca fix: TS2664 (Invalid module name in augmentation) now emits correctly
```

## History Summary

### 2025-02-04: TS2664, TS2322 Bivariance, Accessor Compatibility
Fixed three major issues:
1. TS2664 module augmentation errors (binder state corruption)
2. TS2322 false positives from incorrect bivariance handling
3. Accessor type compatibility with class inheritance

### Earlier Work
- TS2305 (Module has no exported member) ✅ Working
- TS2318 (Cannot find global type) ✅ Working
- TS2307 (Cannot find module) ✅ Working
- Conformance baseline: 46.8% pass rate (up from 32%)

## Punted Todos

*None currently - all punted items moved to other sessions or documented as pre-existing*
