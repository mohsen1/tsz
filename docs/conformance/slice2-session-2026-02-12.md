# Conformance Work Session - Slice 2 - 2026-02-12

## Current Status

**Pass Rate**: 59.2-59.3% (1859/3138 tests passing in slice 2)
**Baseline at session start**: ~58.9%
**Improvement**: +0.3-0.4%

## Completed Fixes

### 1. T & {} Assignability for Type Parameters
**Commit**: `045b5303e`
**Issue**: `T & {}` was not assignable to `T` where T is a type parameter
**Solution**: Added special case check in `check_subtype_inner` (line 2575-2587 in `subtype.rs`)
**Impact**: ~0.3% improvement, affects tests using the common pattern to exclude null/undefined
**Tests**: Added `intersection_type_param_tests.rs` with 3 comprehensive tests

## Top Remaining Issues

### False Positives (we emit errors when tsc doesn't)

1. **TS2339** - Property doesn't exist: 154 extra occurrences
   - Likely causes: Mapped type property resolution, index signatures, symbol handling
   - Example: `indirectGlobalSymbolPartOfObjectType.ts` - issues with `Symbol.iterator`

2. **TS2345** - Argument type mismatch: 121 extra occurrences
   - Likely causes: Generic type inference failures, rest parameter handling
   - Examples:
     - `inferObjectTypeFromStringLiteralToKeyof.ts` - inferring T from `keyof T`
     - `inferRestArgumentsMappedTuple.ts` - mapped tuple rest arguments

3. **TS2322** - Type not assignable: 109 extra occurrences
   - Likely causes: Complex type evaluations, indexed access normalization
   - Examples:
     - `indexedAccessRetainsIndexSignature.ts` - `Omit<T, K>` utility types
     - `inferStringLiteralUnionForBindingElement.ts` - array literal inference

4. **TS1005** - Syntax error (';' expected): 91 extra occurrences
   - Likely cause: Parser error recovery emitting extra errors
   - Example: `invalidUnicodeEscapeSequance.ts` - should only emit TS1127, but also emits TS1005

### Missing Errors (tsc emits but we don't)

1. **TS2322**: 19 tests (partially implemented)
2. **TS2339**: 14 tests (partially implemented)
3. **TS2345**: 11 tests (partially implemented)
4. **TS2451**: 9 tests (redeclaration errors)
5. **TS2307**: 9 tests (module not found)

## Investigation Notes

### Generic Type Inference Issues
- Multiple tests fail because tsz doesn't correctly infer type parameters from:
  - Array literals: `func({keys: ["aa", "bb"]})` should infer `T = "aa" | "bb"`
  - Keyof constraints: passing `"a" | "d"` to `keyof T` should infer appropriate T
  - Mapped tuples: rest arguments with mapped types

### Property Resolution Issues
- Well-known symbols (`Symbol.iterator`) not correctly resolved as valid indices
- Mapped type properties not being found
- Index signature handling incomplete

### Parser Issues
- Error recovery producing extra "expected" errors alongside actual errors
- Affects Unicode escape sequences and possibly other syntax errors

## Recommended Next Steps

### High Impact (affects 50+ tests)
1. **Fix generic type inference from array literals**
   - Files to investigate: `crates/tsz-solver/src/infer.rs`, `crates/tsz-checker/src/expressions.rs`
   - Would fix tests like `inferStringLiteralUnionForBindingElement.ts`

2. **Improve mapped type property resolution**
   - Files: `crates/tsz-solver/src/evaluate_rules/mapped.rs`, `crates/tsz-solver/src/objects.rs`
   - Would fix TS2339 false positives with Record<K, T> and similar

3. **Fix parser error recovery for extra TS1005**
   - Files: `crates/tsz-parser/src/parser/*.rs`
   - Would fix 2 tests immediately (invalidUnicodeEscapeSequance tests)

### Medium Impact (affects 10-50 tests)
4. **Implement missing TS2451 (redeclaration) checks**
   - Would pass 9 tests

5. **Implement missing TS2307 (module not found) checks**
   - Would pass 9 tests

### Investigation Needed
6. **Symbol handling for well-known symbols**
   - `Symbol.iterator`, `Symbol.toStringTag`, etc.
   - Affects indexed access with symbols

## Test Examples for Each Issue

### Generic Inference
```typescript
// Should infer T = "aa" | "bb"
declare function func<T extends string>(arg: { keys: T[] }): { readonly keys: T[]; readonly firstKey: T; };
const { firstKey } = func({keys: ["aa", "bb"]})
const a: "aa" | "bb" = firstKey; // tsz: error, tsc: ok
```

### Mapped Types
```typescript
// Record<K, T> should be assignable to { [key: string]: T }
function f1<T, K extends string>(x: { [key: string]: T }, y: Record<K, T>) {
    x = y; // tsz: error, tsc: ok
}
```

### Parser Error Recovery
```typescript
var arg\u003 // Invalid Unicode escape
// tsz emits: TS1005, TS1127
// tsc emits: TS1127 only
```

## Statistics

- **Total slice 2 tests**: 3,138
- **Passing**: 1,859 (59.2%)
- **Failing**: 1,279 (40.8%)
  - False positives: 407 (31.8% of failures)
  - All missing: 357 (27.9% of failures)
  - Wrong codes: 514 (40.2% of failures)

## Quick Wins (Single Missing Error)

298 tests are missing just ONE error code. Implementing these would provide immediate wins:
- TS2322 (partial): 13 tests
- TS2339 (partial): 9 tests
- TS2345 (partial): 9 tests
- TS2307 (missing): 8 tests
- TS2451 (missing): 7 tests
- TS2320 (missing): 6 tests
- TS2415 (missing): 6 tests
- TS2480 (missing): 6 tests

## Notes for Next Session

1. All unit tests pass (3,548 solver tests)
2. The codebase is stable
3. Focus on **generic type inference** for highest impact
4. Parser issues are localized but need careful handling
5. Consider using the `tsz-gemini` skill for complex architectural questions
6. Use `tsz-tracing` skill for debugging type inference issues
