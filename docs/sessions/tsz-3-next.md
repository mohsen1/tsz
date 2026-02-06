# Session TSZ-3: TS2339 String Literal Property Access Fix

**Started**: 2026-02-06
**Status**: 🔄 IN PROGRESS (Investigation phase)
**Focus**: Fix TS2339 false positives on string literal property access

## Problem Statement

**Issue**: String literals are being treated as having all properties (any-like behavior in property access), causing TS2339 false positives.

**Expected Behavior**:
```typescript
const str = "hello";
str.unknownProperty; // Should emit TS2339: Property 'unknownProperty' does not exist on type '"hello"'
```

## Investigation Findings

**Gemini Guidance Summary**:

The issue is that string literals should use the `String` interface type for property lookup, not return `ANY` for unknown properties.

**Key Files**:
1. `src/solver/operations.rs` - property lookup logic for primitives
2. `src/checker/expr.rs` - TS2339 reporting
3. `src/solver/lawyer.rs` or `src/solver/compat.rs` - possible lax rules for primitives

**Root Cause Hypothesis**:
Currently, when `get_property_of_type` encounters a string literal type, it may be:
- Returning `TypeId::ANY` instead of looking up the `String` interface
- Using a "lax" rule in lawyer/compat that allows all properties on primitives

**Correct Approach**:
1. Identify the Base Type: When `get_property_of_type` encounters `TypeKey::Literal(LiteralValue::String(_))`, it should lookup the `String` symbol
2. Lookup Global Interface: Resolve the `String` interface type and perform property lookup
3. Handle Missing Properties: If property not found on `String` interface, return `None`/error to trigger TS2339

## Next Steps

1. Use tracing to see the flow:
   ```bash
   TSZ_LOG="wasm::solver::operations=trace" cargo run -- test_string_literal_prop.ts
   ```

2. Check `src/solver/operations.rs` for `get_property_of_type` to see why it returns valid type for unknown properties

3. Ensure `src/solver/lower.rs` correctly identifies `String` interface symbol during bootstrap

## Estimated Impact

- **Estimated**: ~50-100 false positives
- **Complexity**: Medium (requires understanding global interface lookup)
- **Estimated Time**: 2-4 hours

---

*Session updated by tsz-3 on 2026-02-06*
