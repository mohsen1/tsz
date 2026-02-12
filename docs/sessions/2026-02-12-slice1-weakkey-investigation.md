# Slice 1 WeakKey Investigation - Session Summary (2026-02-12)

## Goal
Achieve 100% conformance test pass rate for Slice 1 (offset 0, max 3146)

## Starting Status
- **Pass Rate**: 68.5% (2150/3139 tests)
- **Failing Tests**: 989
- **Key Issues**: False positives (319 tests), missing errors (281 tests), wrong codes (390 tests)

## Investigation: WeakKey Type Issue

### Problem
Tests like `acceptSymbolAsWeakType.ts` fail with:
```
error TS2769: No overload matches this call.
  Argument of type 'symbol' is not assignable to parameter of type 'WeakKey'.
```

This is a **false positive** - TypeScript correctly accepts `symbol` as `WeakKey` in ES2023+.

### Root Cause Analysis

#### Expected Behavior
```typescript
// In ES2023+, WeakKey should be: object | symbol
type WeakKey = WeakKeyTypes[keyof WeakKeyTypes];

// WeakKeyTypes is defined across multiple lib files:
// es5.d.ts:
interface WeakKeyTypes {
    object: object;
}

// es2023.collection.d.ts:
interface WeakKeyTypes {
    symbol: symbol;
}
```

TypeScript merges these interface declarations, so:
- `keyof WeakKeyTypes` = `"object" | "symbol"`
- `WeakKey` = `WeakKeyTypes["object"] | WeakKeyTypes["symbol"]` = `object | symbol`

#### What's Broken in tsz

**Issue 1: Missing lib file reference**
- `es2024.collection.d.ts` didn't reference `es2023.collection.d.ts`
- This meant `es2023.collection.d.ts` wasn't being loaded in the esnext lib chain

**Issue 2: Interface merging across arenas**
- Even with correct references, tsz's interface merging doesn't properly combine declarations from different lib files
- The `lower_merged_interface_declarations` function should merge properties from all arenas
- Currently, `WeakKeyTypes` resolves to just `{object: object}`, missing the `symbol` property

### Fix Applied

**Commit**: `2ec96292f fix: add es2023.collection reference to es2024.collection.d.ts`

Added to `src/lib-assets/es2024.collection.d.ts`:
```typescript
/// <reference lib="es2023.collection" />
```

This ensures the reference chain is complete:
```
esnext.d.ts
  → es2024.d.ts
    → es2023.d.ts
      → es2023.collection.d.ts (WeakKeyTypes { symbol: symbol })
    → es2024.collection.d.ts (now references es2023.collection.d.ts)
```

### Current Status: Fix Incomplete

**Expected**: Fix would resolve WeakKey issues
**Actual**: Tests still fail with same error

**Why**: The lib file reference fix was necessary but not sufficient. The underlying bug is in tsz's interface merging logic.

## Deeper Issue: Cross-Arena Interface Merging

### How It Should Work

When resolving `WeakKeyTypes`:
1. Find all declarations of `WeakKeyTypes` across all loaded lib files
2. Merge their properties into a single interface type
3. Use that merged type for indexed access (`WeakKeyTypes[keyof WeakKeyTypes]`)

### What's Failing

The `lower_merged_interface_declarations` function in `crates/tsz-solver/src/lower.rs` iterates over declarations but appears to not be correctly merging properties from different arenas.

Relevant code:
```rust
pub fn lower_merged_interface_declarations(
    &self,
    declarations: &[(NodeIndex, &NodeArena)],
) -> (TypeId, Vec<TypeParamInfo>) {
    // ...
    for (decl_idx, decl_arena) in declarations {
        let lowerer = self.with_arena(decl_arena);
        // Collect members from each arena
        lowerer.collect_interface_members(&interface.members, &mut parts);
    }
    let result = self.finish_interface_parts(parts, None);
    // ...
}
```

### Related Fix

A concurrent session just committed:
- `b27d2382b fix: prevent cross-arena ping-pong for multi-file lib interfaces`

This fixed infinite delegation loops for interfaces like RegExp, Date, Error that span multiple lib files. However, it doesn't fully resolve the merging issue for WeakKeyTypes.

## Analysis Results

### Top Error Patterns (from analysis)

**False Positives** (we emit when we shouldn't):
- TS2345: 120 tests - Argument type errors
- TS2322: 106 tests - Assignment type errors
- TS2339: 95 tests - Property access errors
- TS7006: 31 tests - Implicit any errors
- TS2769: 25 tests - Overload resolution errors

**Missing Errors** (we don't emit when we should):
- TS2322: 57 tests
- TS2304: 44 tests - Cannot find name
- TS2339: 24 tests
- TS2300: 20 tests - Duplicate identifier

**Not Implemented** (never emitted):
- TS2792: 16 tests - Type instantiation depth
- TS2538: 9 tests - Type used before assigned
- TS2323: 9 tests - Duplicate identifier
- TS2301: 8 tests

### Quick Wins Available

234 tests are "close to passing" (missing just 1 error code):
- Implementing TS2322 properly → 20 test wins
- Implementing TS2304 → 9 test wins
- Implementing TS2353 → 7 test wins
- Implementing TS2339 → 7 test wins

## Lessons Learned

### 1. Systematic Bugs Have Wide Impact
The WeakKey issue affects multiple tests because it's a fundamental type system bug. Fixing it requires addressing the underlying architecture, not individual test cases.

### 2. Lib File Architecture is Complex
TypeScript's lib files form a dependency graph through `/// <reference lib="..." />` directives. The resolution order matters:
1. TypeScript/src/lib/ (TypeScript submodule)
2. src/lib-assets/ (our copies)

Changes must be made to both locations.

### 3. Interface Merging is Critical
Interface declaration merging is fundamental to TypeScript's type system. Bugs here cascade through:
- Built-in types (Array, Promise, etc.)
- Global augmentations
- Module augmentations
- Multi-file library definitions

### 4. 100% Conformance is Not a Single-Session Goal
With 989 failing tests and systematic issues like:
- Interface merging bugs
- Type guard narrowing not implemented
- Missing error code implementations
- Over-emission of certain diagnostics

Achieving 100% requires:
- Multiple focused sessions on specific subsystems
- Architectural fixes before test-by-test improvements
- Systematic debugging with tracing and unit tests

## Next Steps

### High Priority
1. **Debug interface merging** - Add tracing to `lower_merged_interface_declarations` to see why properties aren't merging
2. **Fix WeakKeyTypes specifically** - May need special handling for global interface augmentations
3. **Unit test interface merging** - Create failing test for WeakKeyTypes before fixing

### Medium Priority
4. **Type guard narrowing** - Implement predicate type narrowing for methods like Array.find()
5. **Reduce false positives** - Audit why we're over-emitting TS2345, TS2322, TS2339
6. **Implement missing codes** - TS2792, TS2538, TS2323 as quick wins

### Low Priority (Long-term)
7. **Improve lib file loading** - Better diagnostics for missing references
8. **Audit all lib files** - Ensure all reference chains are complete
9. **Cross-arena testing** - Add unit tests for multi-arena scenarios

## Files Changed
- `src/lib-assets/es2024.collection.d.ts` - Added es2023.collection reference

## Verification Needed
- [ ] Interface merging for WeakKeyTypes
- [ ] All lib file reference chains complete
- [ ] Cross-arena symbol resolution working

## Metrics After Session
- **Pass Rate**: 68.5% (unchanged - fix incomplete)
- **Commits**: 1 (lib file reference)
- **Investigation**: Documented root causes
- **Path Forward**: Clear next steps identified

## Conclusion

While this session didn't achieve the 100% goal, it provided valuable insights:
1. Identified the WeakKey bug's root cause
2. Fixed the lib file reference chain (prerequisite for full fix)
3. Documented the interface merging issue for future work
4. Established that 100% requires systematic fixes, not test-by-test

The work done here sets the foundation for future improvements to the interface merging system, which will benefit many tests beyond just WeakKey.
