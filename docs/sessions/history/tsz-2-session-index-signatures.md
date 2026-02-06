# Session tsz-2: Index Signature Type Resolution

**Date:** 2026-02-06  
**Task:** #18 - Fix Index Signatures  
**Status:** Just started

## Overview

Working on Task #18 (Index Signatures) which has 2 failing tests:
- `test_indexed_access_class_property_type`
- `test_indexed_access_resolves_class_property_type`

## Test Case

```typescript
class C {
    foo = 3;
    constructor() {
        const ok: C["foo"] = 3;  // Should have no error
    }
}
```

**Expected:** No diagnostics (C["foo"] should resolve to number type)  
**Actual:** TS2322: Type 'number' is not assignable to type 'C["foo"]'

## Previous Context

Task #22 (interface readonly properties) is blocked on fundamental architecture:
- Trilemma: Can't preserve Lazy (recursion), can't resolve Lazy, can't use flow_type (loses flags)
- Requires cycle detection in property access resolution
- Documented in `tsz-2-session-readonly-investigation.md`

## Next Steps

Investigate why indexed access type `C["foo"]` doesn't resolve to class property type.

## Investigation Findings

### Root Cause: Circular Type Resolution

**Issue:** `C["foo"]` is resolved inside class C's constructor, creating a cycle.

**Code Path:**
1. `evaluate_index_access(C, "foo")` is called
2. `C` is a Lazy type (class definition)
3. `IndexAccessVisitor::visit_lazy` tries to resolve C
4. Resolver detects it's already resolving C (prevents infinite loop)
5. Returns None/incomplete type
6. Falls back to unevaluated `IndexAccess(C, "foo")`
7. Assignment check fails: `number` != `IndexAccess(C, "foo")`

### Fix Location (Per Gemini):

**Option 1:** `src/solver/evaluate_rules/index_access.rs` (visit_lazy)
- Too binary: if resolve_lazy returns None, gives up
- Should handle "in-progress" state of class resolution
- Look up properties via Binder's symbol table directly

**Option 2:** TypeResolver in Checker
- Needs smarter cycle detection
- Return partial object type with properties discovered so far
- Instead of returning None for recursive calls

### Status:

This is complex and touches the same architectural issues as Task #22.
Both tasks require handling circular type resolution more gracefully.

### Recommendation:

Document findings and continue in next session. This requires
careful implementation of cycle-aware type resolution.
