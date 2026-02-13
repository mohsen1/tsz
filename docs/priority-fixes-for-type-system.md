# Priority Fixes for Type System Conformance

**Date**: 2026-02-13
**Context**: Based on conformance analysis of first 500 tests

## Executive Summary

Current pass rate: **414/499 (83.0%)**

Analysis shows 3 clear categories of high-impact fixes:
1. **🔴 Unimplemented error codes** - instant wins when implemented
2. **🟠 False positives** - reduce noise and improve DX
3. **🟡 Missing cases** - errors that work sometimes but not always

## 🎯 Top Priority: Array Predicate Type Narrowing

### The Issue

**TS2339 False Positives**: 9 tests affected (highest false positive count)

Array methods with type predicates don't narrow the array type:

```typescript
const foo: (number | string)[] = ['aaa'];
const isString = (x: unknown): x is string => typeof x === 'string';

if (foo.every(isString)) {
    foo[0].slice(0);  // ❌ TS2339: Property 'slice' doesn't exist on type 'number | string'
                       // ✅ TSC: No error - foo narrowed to string[]
}
```

### Implementation Plan

**Task**: Task #7 created

**What needs to be done:**
1. Detect calls to array methods with type predicates: `.every()`, `.filter()`, `.some()`
2. Extract type predicate from callback argument
3. Apply narrowing in control flow analysis

**For `.every(predicate)`:**
- If condition is `if (arr.every(predicate))`, narrow `arr` in true branch
- Element type becomes the narrowed type from predicate

**For `.filter(predicate)`:**
- Result type should have narrowed element type
- Original array not narrowed (returns new array)

**For `.some(predicate)`:**
- More complex - doesn't narrow original array
- Only tells us at least one element matches

### Files to Modify

```
crates/tsz-checker/src/control_flow_narrowing.rs  - Extract type guards from .every() calls
crates/tsz-solver/src/narrowing.rs                - May need new TypeGuard variant
```

### Test Case

```
TypeScript/tests/cases/compiler/arrayEvery.ts
```

### Impact

- **Immediate**: Fixes arrayEvery.ts (1 test)
- **Broader**: Pattern applies to other array predicate tests
- **Category**: Reduces TS2339 false positives (9 tests with this error)

## 🔴 High-Value: Unimplemented Error Codes

These errors are **never** emitted by tsz. Implementing them gives instant wins.

### TS2503: Cannot find namespace (4 tests)

**What it is**: Reference to a namespace that doesn't exist or isn't imported

**Example:**
```typescript
MyNamespace.someFunction();  // TS2503 if MyNamespace not found
```

**Why we don't emit it**: Namespace lookup may be returning a different error

### TS2693: Only refers to a type, being used as a value (3 tests)

**What it is**: Using a type in a value position

**Example:**
```typescript
interface Foo {}
const x = Foo;  // TS2693 - Foo is a type, not a value
```

**Why we don't emit it**: Type vs value distinction may not be checked everywhere

### TS2741: Property is missing but required (2 tests)

**What it is**: Object literal missing required property

**Example:**
```typescript
interface Foo { x: number; y: string }
const f: Foo = { x: 1 };  // TS2741 - missing 'y'
```

**Why we don't emit it**: We emit TS2322 (general type mismatch) instead

### TS2488: Type must have Symbol.iterator (2 tests)

**What it is**: for-of requires iterable type

**Example:**
```typescript
for (const x of notIterable) {}  // TS2488
```

**Why we don't emit it**: Iterable check may not be comprehensive

## 🟠 High-Impact: False Positives to Fix

### TS2345: Argument type mismatch (8 tests)

These are cases where we emit TS2345 but TSC doesn't. Often due to:
- Generic inference not matching TSC
- Bivariant parameter checking differences
- Contextual typing not propagating correctly

### TS2769: No overload matches (6 tests)

We covered this in generic function inference investigation. Complex - defer.

### TS7006: Parameter implicitly has 'any' (4 tests)

**Root cause**: JSDoc parameter type annotations not applied to arrow functions

**Tests affected**: All JavaScript files with JSDoc annotations

**Example:**
```typescript
/**
 * @param {string} x
 */
const fn = x => x.toUpperCase();  // We emit TS7006, TSC infers string from JSDoc
```

**Complexity**: Medium - JSDoc parsing works, but not applied to arrow function params

## 🟡 Medium Priority: Missing Cases

### TS2322: Type not assignable (11 tests missing)

We emit TS2322 in many cases but miss some. Need case-by-case investigation.

### TS2304: Cannot find name (5 tests missing)

We emit this generally but miss specific cases, possibly:
- Lookup in specific scopes
- Global augmentation cases
- Module resolution edge cases

## Implementation Strategy

### Week 1 Focus

1. ✅ **Array predicate narrowing** (Task #7)
   - Fixes TS2339 false positives
   - Clear implementation path
   - Visible user impact

2. **JSDoc arrow function parameters**
   - Fixes TS7006 false positives (4 tests)
   - Medium complexity
   - Improves JavaScript developer experience

### Week 2 Focus

3. **Unimplemented error codes** (pick 2-3)
   - TS2693 (type vs value) - 3 tests
   - TS2741 (missing property) - 2 tests
   - TS2488 (not iterable) - 2 tests

### Future

4. **Generic inference refinements** (defer until above done)
5. **Module resolution edge cases** (many tests, complex)

## Measurement

Track improvement after each fix:

```bash
./scripts/conformance.sh run --max=500 --offset=0 2>&1 | tail -20
```

**Current**: 414/499 (83.0%)
**Target Week 1**: 420+/499 (84%+)
**Target Week 2**: 430+/499 (86%+)

## References

- **Analysis**: `docs/type-system-conformance-status.md`
- **Conformance tool**: `./scripts/conformance.sh analyze`
- **Task tracking**: Tasks #1-7

## Key Insights

1. **Impact > Complexity**: Array narrowing affects 9 tests, generic inference affects 6
2. **False positives matter**: Better to emit no error than wrong error
3. **Unimplemented codes**: Low-hanging fruit - just need the check logic
4. **JSDoc is important**: 4 TS7006 tests are all JSDoc-related

## Success Metrics

- ✅ No regressions (all 2394 unit tests pass)
- ✅ Architecture principles maintained
- 📈 Conformance rate improvement
- 📉 False positive count reduction
