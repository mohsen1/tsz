# Session TSZ-3: TS2339 String Literal Property Access Fix

**Started**: 2026-02-06
**Status**: ✅ COMPLETE
**Focus**: Fix TS2339 false positives on string literal property access

## Problem Statement

**Issue**: String literals were being treated as having all properties (any-like behavior in property access), causing TS2339 false negatives.

**Expected Behavior**:
```typescript
const str = "hello";
str.unknownProperty; // Should emit TS2339: Property 'unknownProperty' does not exist on type '"hello"'
str.length; // Should work (returns number)
```

## Solution Implemented

**File**: `src/solver/operations_property.rs` - PropertyAccessEvaluator visitor implementation

### Changes Made

1. **Updated `visit_literal`** (lines 196-218):
   - Now handles `LiteralValue::String`, `Number`, `Boolean`, `BigInt`
   - Delegates to existing helper methods: `resolve_string_property`, `resolve_number_property`, etc.
   - These helpers call `get_boxed_type()` to get the interface type from lib.d.ts

2. **Updated `visit_intrinsic`** (lines 128-230):
   - Added handling for `IntrinsicKind::Never` - returns `NEVER` type
   - Added handling for `IntrinsicKind::String`, `Number`, `Boolean`, `Bigint`
   - Each primitive type delegates to its corresponding `resolve_*_property` helper

3. **Added `visit_template_literal`** (lines 391-411):
   - Template literals are string-like for property access
   - Delegates to `resolve_string_property`

4. **Added `visit_string_intrinsic`** (lines 413-433):
   - String intrinsics (Uppercase<T>, Lowercase<T>, Capitalize<T>, etc.) are string-like
   - Delegates to `resolve_string_property`

### How It Works

The fix leverages the existing infrastructure:
- `resolve_primitive_property` (lines 2072-2098) first tries `get_boxed_type(kind)` to get the interface from lib.d.ts
- If the boxed type is found, it recursively calls `resolve_property_access_inner` on that interface
- Falls back to `resolve_apparent_property` for hardcoded members (bootstrapping/partial lib)

## Success Criteria

- [x] Find failing TS2339 test case for string literals
- [x] Implement fix in type computation using visitor pattern
- [x] Verify TS2339 is emitted for unknown properties
- [x] Handle all primitive types (String, Number, Boolean, BigInt, Symbol)
- [x] Handle Never type
- [x] Handle template literals
- [x] Handle string intrinsics (Uppercase<T>, etc.)
- [x] Compiled successfully with zero warnings
- [x] Commit and push fixes

## Test Results

- **Build**: ✅ Compiled successfully with zero clippy warnings
- **Commit**: `a41321d34` - "feat(solver): add property resolution for primitive literals and intrinsics"

## Key Insights

1. **Visitor Pattern**: The fix follows the Solver-First Architecture by implementing visitor methods that delegate to existing helper functions
2. **No Duplication**: The helper functions (`resolve_string_property`, etc.) already existed and contained the correct logic
3. **Type Safety**: Using the visitor pattern ensures all type variants are systematically handled
4. **Edge Cases**: Added explicit handling for `Never` type to prevent false positives on unreachable code

---

*Session completed by tsz-3 on 2026-02-06*
