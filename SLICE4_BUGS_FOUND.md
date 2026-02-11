# Slice 4 Conformance Bugs Identified

**Pass Rate:** 1668/3134 (53.2%)
**Date:** 2026-02-11

## Bug 1: Interface/Namespace Scoping (CRITICAL)
**Status:** Root cause identified, no fix yet
**Impact:** Causes cascading failures in 100+ tests

### Symptom
When both exist in same file:
```typescript
interface A { y: string; }
namespace M { interface A<T> { z: T; } }
class D implements A { y: string; } // ERROR!
```

Errors emitted:
- TS2314: "Generic type 'A' requires 1 type argument(s)" (WRONG - top-level A is non-generic)
- TS2420: "Missing members: 'z'" (WRONG - using M.A shape instead of top-level A)

### Root Cause
Symbol resolution in heritage clauses doesn't properly scope lookups. When resolving "A" in `implements A`, it crosses namespace boundaries and finds M.A<T> instead of staying in the current scope.

### Test Files
- `test-minimal-bug.ts` - Demonstrates the bug
- `test-interface-scope.ts` - Scoping test
- `test-interface-merge2.ts` - With merging

---

## Bug 2: TS2339 False Positives on Generic + Readonly
**Status:** Partial fix committed by other developer
**Impact:** 142 tests in slice 4

### Symptom
```typescript
interface Props { foo: string; }
function test<P extends Props>(props: Readonly<P>) {
    props.foo; // ERROR TS2339: Property 'foo' does not exist on type 'unknown'
}
```

### Root Cause
In `get_type_of_element_access` (line 1129 of type_computation.rs):
```rust
let object_type = self.evaluate_application_type(object_type);
```

This evaluates `Readonly<P>` too early, returning type 3 (base Readonly symbol) instead of keeping the application with the type parameter.

### Partial Fix Applied
Modified `resolve_type_for_property_access_inner` to recursively resolve Application type arguments. However, this doesn't help because `evaluate_application_type` already corrupted the type.

### Complete Fix Needed
Either:
1. Fix `evaluate_application_type_inner` to not evaluate when args are uninstantiated type parameters
2. Swap order: call `resolve_type_for_property_access` BEFORE `evaluate_application_type`
3. Don't call `evaluate_application_type` at all before property access resolution

See `BUG_READONLY_GENERIC.md` for detailed analysis.

---

## Bug 3: {} Not Assignable to Object
**Status:** Root cause identified, no fix yet
**Impact:** 88 TS2322 false positives in slice 4

### Symptom
```typescript
interface Foo {
    g: Object;
}
var a: Foo = {
    g: {}  // ERROR TS2322: Type '{ g: {} }' is not assignable to type 'Foo'
};
```

TSC correctly accepts this. We incorrectly reject it.

### Root Cause
Object literal type checking doesn't recognize that `{}` (empty object literal type) is assignable to `Object`.

This likely affects:
- Object literal assignability checks
- Fresh object literal handling
- Subtype relation for Object type

### Test Files
- `test-prop-g.ts` - Minimal reproduction (property g: Object assigned {})
- `test-object-assignability.ts` - Direct and interface assignment
- `test-half-properties.ts`, etc. - Bisection tests

### Example Failing Test
`interfaceWithPropertyOfEveryType.ts` - Has property `g: Object` in an interface with many properties

---

## Bug 4: Namespace Merging with Dotted Syntax
**Status:** Identified, not investigated
**Impact:** 13 TS2403 false positives

### Symptom
```typescript
namespace X.Y.Z { export interface Line { ... } }
namespace X { export namespace Y.Z { export interface Line { ... } } }
```

These should merge but we emit TS2403 "Subsequent variable declarations must have the same type".

### Root Cause
Dotted namespace syntax (`X.Y.Z`) merging not working correctly.

---

## Error Code Statistics

### Top False Positives (we emit incorrectly):
- TS2339: 142 tests - Generic + Readonly bug
- TS1005: 100 tests - Parse errors
- TS2344: 90 tests - Generic type errors
- TS2322: 88 tests - Object assignability bug
- TS2345: 85 tests - Argument type

### Top Missing (tsc emits, we don't):
- TS2304: 141 tests - Cannot find name
- TS2322: 112 tests - Type assignment
- TS6053: 103 tests - File not module
- TS2307: 89 tests - Cannot find module
- TS2339: 67 tests - Property doesn't exist

### Quick Wins (need just 1 error code):
- TS2322: 36 tests
- TS2339: 21 tests
- TS2304: 16 tests
- TS2300: 10 tests - Duplicate identifier
- TS2411: 9 tests - Property incompatible with index

---

## Progress Summary

### Work Done
1. ✅ Identified interface/namespace scoping bug with minimal reproduction
2. ✅ Identified Object assignability bug with bisection tests
3. ✅ Reviewed TS2339 generic+readonly bug (being fixed by other dev)
4. ✅ Documented all findings with test cases
5. ✅ Committed and synced findings

### Blocked
- All identified bugs require deep investigation of type system
- Interface/namespace scoping needs heritage clause resolution refactor
- Object assignability needs subtype relation fixes
- Generic+readonly bug needs application type evaluation fixes

### Recommendation
These are all foundational type system bugs that will take significant time to fix correctly. Consider:
1. Bringing in developer with deep solver/checker knowledge
2. Fixing bugs in priority order (TS2339 has highest impact)
3. Adding comprehensive unit tests before attempting fixes
4. Using tracing infrastructure to debug type resolution paths
