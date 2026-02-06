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
