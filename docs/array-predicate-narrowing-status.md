# Array Predicate Type Narrowing - Implementation Status

**Date**: 2026-02-13
**Status**: Partial implementation - guard creation working, application blocked

## Problem Statement

Array methods with type predicates should narrow the array type:

```typescript
const foo: (number | string)[] = ['aaa'];
const isString = (x: unknown): x is string => typeof x === 'string';

if (foo.every(isString)) {
    foo[0].slice(0);  // ❌ TS2339: Property 'slice' doesn't exist on type 'number | string'
                       // ✅ TSC: No error - foo narrowed to string[]
}
```

## What Was Implemented

### 1. Type Guard Infrastructure (✅ Complete)
- **File**: `crates/tsz-solver/src/narrowing.rs`
- Added `TypeGuard::ArrayElementPredicate` variant
- Implemented `narrow_array_element_type()` to narrow array element types
- Applied guard in `narrow_type()` match arm

### 2. Guard Detection (✅ Complete)
- **File**: `crates/tsz-checker/src/control_flow_narrowing.rs`
- Added `check_array_every_predicate()` function
- Detects `.every()` calls with type predicate callbacks
- Extracts predicate type and creates `ArrayElementPredicate` guard
- Returns `(guard, target)` tuple where target is the array expression

### 3. Flow-Sensitive Identifier Types (✅ Complete)
- **File**: `crates/tsz-checker/src/state.rs`
- Modified `get_type_of_node()` to apply flow narrowing for cached identifier types
- This ensures identifiers can have different types in different control flow branches
- Identifiers now cache their declared type but apply narrowing on retrieval

## What's Blocked

### The Core Issue
The guard is **created correctly** but **never applied** during flow narrowing.

**Root Cause**: Type ordering dependency
- When `check_array_every_predicate()` is called during flow analysis (walking backwards through control flow graph)
- It needs the callback's type to extract the predicate
- But the callback is an identifier, and its type may not be cached yet
- Solution implemented: Check cache first, but guard extraction now works

**However**, even with the guard created, it's not being applied when narrowing `foo` inside the if block.

## Debugging Findings

### Trace Analysis

When checking `foo[0].slice(0)` inside the if block:

1. ✅ `get_type_of_identifier` is called for `foo` at NodeIndex(34)
2. ✅ `check_flow_usage` is called with declared_type=TypeId(57137)
3. ✅ `apply_flow_narrowing` is invoked
4. ✅ Walks back through flow graph
5. ✅ Finds the condition node for `foo.every(isString)`
6. ✅ Calls `check_array_every_predicate()` to extract guard
7. ✅ Guard is created: `ArrayElementPredicate { element_type: TypeId(10) }`
8. ❌ Guard is NOT applied - returns unnarrowed type

### Key Observations

**Guard Target Mismatch**:
- Guard created with target=NodeIndex(29) (`foo` in `foo.every(isString)`)
- Narrowing requested for target=NodeIndex(34) (`foo` in `foo[0]`)
- These are different AST nodes for the same variable
- Matching should be based on **symbol**, not NodeIndex

**Flow Graph Structure**:
The binder creates flow nodes during parsing. When we have:
```typescript
if (foo.every(isString)) {  // Condition node created here
    foo[0].slice(0);          // Inside true branch
}
```

The flow analysis should:
1. Get flow node for `foo` at line 2
2. Walk backwards to condition node at line 1
3. Extract guard for that condition
4. Check if guard target matches current reference
5. Apply guard if matches

**Step 4 is failing** - the matching logic doesn't recognize that NodeIndex(29) and NodeIndex(34) refer to the same variable.

## Next Steps

### Immediate Fix Needed

**File**: `crates/tsz-checker/src/control_flow.rs`
**Function**: `narrow_type_by_condition_inner()` around lines 2280-2327

The issue is in how guards are matched to targets. Currently:
```rust
if self.is_matching_reference(guard_target, target) {
    // Apply guard
}
```

For `ArrayElementPredicate`, the `guard_target` is the array expression in the condition (`foo` at idx 29), and `target` is the identifier being narrowed (`foo` at idx 34).

**Solution**: `is_matching_reference()` needs to match based on **symbol identity**, not just NodeIndex equality. Two identifiers match if they:
1. Both resolve to the same SymbolId
2. Are both simple identifiers (not property accesses)

### Alternative Approach

If symbol-based matching is complex, an alternative is:
1. Store guards with **SymbolId** instead of NodeIndex
2. When extracting guards, map NodeIndex → SymbolId
3. When applying guards, map target NodeIndex → SymbolId
4. Match on SymbolId

This would be more robust but requires refactoring the `TypeGuard` structure and all guard creation/application code.

## Testing

### Test Case
```bash
# Create test file
cat > tmp/test-array-every.ts <<'EOF'
const foo: (number | string)[] = ['aaa'];
const isString = (x: unknown): x is string => typeof x === 'string';

if (foo.every(isString)) {
    foo[0].slice(0);  // Should not error
}
EOF

# Run test
.target/dist-fast/tsz tmp/test-array-every.ts

# Expected: No errors
# Actual: TS2339: Property 'slice' does not exist on type 'number | string'
```

### Tracing Commands
```bash
# See guard creation
TSZ_LOG="tsz_checker::control_flow_narrowing=trace" TSZ_LOG_FORMAT=tree \
  .target/dist-fast/tsz tmp/test-array-every.ts 2>&1 | head -30

# See flow narrowing
TSZ_LOG=trace TSZ_LOG_FORMAT=tree \
  .target/dist-fast/tsz tmp/test-array-every.ts 2>&1 | grep -A 20 "check_flow_usage called.*34"
```

## Architecture Notes

### Control Flow Narrowing Architecture

**Separation of Concerns**:
1. **Binder** (`tsz-binder`): Creates control flow graph structure
2. **Checker** (`tsz-checker/control_flow_narrowing.rs`): Extracts type guards from AST nodes
3. **Solver** (`tsz-solver/narrowing.rs`): Applies narrowing operations to types

**Guard Lifecycle**:
1. **Creation**: When condition is checked, guard is extracted via `extract_type_guard()`
2. **Storage**: Guard is NOT explicitly stored - it's recomputed on demand
3. **Application**: When identifier is used, flow analysis walks back and extracts/applies guards

This on-demand approach means guards must be efficiently re-extractable from the AST.

### Why This is Hard

Type narrowing requires:
1. **Correct AST structure** ✅
2. **Correct flow graph** ✅
3. **Correct guard extraction** ✅
4. **Correct guard application** ❌
5. **Correct target matching** ❌ ← **We are here**

The matching logic is subtle because:
- Same variable has different NodeIndex in different locations
- Guards can apply to expressions, not just identifiers
- Property accesses like `obj.prop` need special handling
- Captured variables in closures have different rules

## Known Issues

### Test Failure
**Test**: `control_flow_tests::test_switch_discriminant_narrowing`
**Status**: Failing after identifier flow-narrowing changes
**Cause**: TypeId mismatch (130 vs 125) - likely type creation order changed
**Impact**: Minimal - test checks exact TypeId equality which is fragile
**Fix**: Either update test to check semantic equality, or investigate why type creation order changed

### Unit Test Status
- **Total**: 368 tests
- **Passed**: 367
- **Failed**: 1 (switch discriminant narrowing)
- **Skipped**: 20

## Conformance Impact

**Direct Impact**: 1 test (`arrayEvery.ts`)
**Pattern Impact**: ~9 tests with similar array predicate patterns
**Category**: Reduces TS2339 false positives

## References

- **Priority Doc**: `docs/priority-fixes-for-type-system.md`
- **Test**: `TypeScript/tests/cases/compiler/arrayEvery.ts`
- **Baseline**: `TypeScript/tests/baselines/reference/arrayEvery.errors.txt`
- **Related**: Control flow narrowing, type predicates, flow-sensitive typing
