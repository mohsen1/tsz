# Module/Namespace Export TS2339 Investigation Notes

**Date:** 2026-02-09
**Status:** Needs further investigation
**Priority:** Medium

---

## Problem

Tests like `aliasDoesNotDuplicateSignatures.ts` emit TS2339 instead of expected TS2322 errors.

---

## Example Test Case

**File:** `TypeScript/tests/cases/compiler/aliasDoesNotDuplicateSignatures.ts`

```typescript
// @filename: demo.d.ts
declare namespace demoNS {
    function f(): void;
}
declare module 'demoModule' {
    import alias = demoNS;
    export = alias;
}

// @filename: user.ts
import { f } from 'demoModule';
let x1: string = demoNS.f;  // Expected: TS2322, Actual: unknown
let x2: string = f;          // Expected: TS2322, Actual: unknown
```

**Expected Errors (TypeScript):**
- Line 3: TS2322 - Type '() => void' is not assignable to type 'string'
- Line 4: TS2322 - Type '() => void' is not assignable to type 'string'

**Actual Errors (tsz):** Unknown (test uses special `@filename` directives)

---

## Issue Analysis

### Pattern
- Namespace with exports
- Module that re-exports namespace via `export = alias`
- Named import from that module
- Type checking the imported value

### Suspected Root Cause
Import alias properties not resolving correctly when:
1. Module uses `export = alias` syntax
2. Alias points to a namespace
3. Named imports extract namespace members

### Code Locations to Investigate
- Module resolution: How `export =` syntax is handled
- Namespace resolution: How namespace members are resolved through aliases
- Import resolution: How named imports extract members from re-exported namespaces

---

## Testing Challenges

### 1. Multi-File Tests
- Test uses `// @filename` directives (TypeScript test syntax)
- Requires proper multi-file testing infrastructure
- Cannot easily test manually

### 2. Module Resolution
- Depends on proper module resolution setup
- Needs declaration files (`.d.ts`)
- Requires understanding of TypeScript's module resolution algorithm

### 3. Export = Syntax
- Less common export syntax
- Special semantics for CommonJS interop
- May have edge cases in implementation

---

## Next Steps for Investigation

### Phase 1: Reproduce Manually
1. Set up proper multi-file test structure
2. Create separate `.d.ts` and `.ts` files
3. Configure module resolution correctly
4. Run tsz and verify error output

### Phase 2: Trace Code Path
1. Add tracing to import resolution
2. Track how `export =` is processed
3. Follow namespace member resolution
4. Identify where property lookup fails

### Phase 3: Implement Fix
1. Based on findings from Phase 2
2. May require changes to:
   - Import statement processing
   - Export assignment handling
   - Namespace member resolution
   - Module export type computation

---

## Complexity Assessment

**Complexity:** Medium-High
- Requires understanding module system
- Multiple interconnected systems (imports, exports, namespaces)
- Testing infrastructure needed

**Estimated Effort:** 4-6 hours
- 1-2 hours: Set up proper test environment
- 1-2 hours: Trace and identify root cause
- 1-2 hours: Implement and test fix

**Dependencies:**
- Multi-file testing infrastructure
- Module resolution system understanding
- Namespace handling knowledge

---

## Alternative: Find Simpler Cases

Before tackling this complex case, consider:
1. Look for simpler TS2339 false positives
2. Find tests that don't require multi-file setup
3. Identify patterns that are easier to reproduce

---

## Recommendation

**Defer this investigation** until:
1. Simpler TS2339 patterns are fixed
2. Better testing infrastructure is available
3. Symbol bug is resolved (may be related)

**Current Priority:** Focus on Symbol bug or other simpler patterns first.

---

## Related Issues

- Symbol resolution bug (may affect namespace resolution)
- Module resolution in general
- Export = syntax handling
- Import statement type resolution

---

**Status:** Documented for future investigation
**Next Action:** Look for simpler TS2339 patterns or work on Symbol bug
