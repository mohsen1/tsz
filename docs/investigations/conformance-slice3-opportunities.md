# Conformance Test Opportunities - Slice 3

## Current Status
- **Pass Rate**: 1812/3144 tests (57.6%)
- **Failing**: 1330 tests
- **Offset**: 6292
- **Max**: 3146

## Top Impact Opportunities

### 1. TS2322 Type Assignability Gaps (27 Quick Wins)
**Impact**: 27 tests would pass immediately
**Category**: Partially implemented

Example test: `yieldExpression1.ts`
```typescript
function* b(): IterableIterator<number> {
    yield;  // Should error: undefined not assignable to number
    yield 0;
}
```
**Issue**: Missing type checking for `yield` expressions without values in typed generators.

**Location to investigate**:
- `crates/tsz-checker/src/` - generator yield type checking
- Need to validate yield value type against generator's declared yield type

---

### 2. TS2339 Property Access Gaps (19 Quick Wins)
**Impact**: 19 tests would pass immediately
**Category**: Partially implemented
**Note**: Separate from the 151 false positives (Readonly bug)

**Issue**: Missing property existence checks in specific scenarios

---

### 3. Readonly<T> Generic Parameter Bug (50-100 Tests)
**Impact**: Estimated 50-100+ tests
**Status**: Root cause identified, documented separately

See: `docs/investigations/readonly-generic-parameter-bug.md`

**Summary**: `Readonly<P>` where P is a type parameter resolves to `unknown` instead of a proper mapped type, causing TS2339 false positives.

---

### 4. TS6192 Missing Implementation (Multiple Tests)
**Error**: "All imports in import declaration are unused."
**Impact**: Many "close to passing" tests (diff=1)

Example test: `unusedImports12.ts`
```typescript
import { Member } from './b';
import d, { Member as M } from './b';
// Expected: [TS6133, TS6192]
// Actual: [TS6133]
```

**Issue**: We emit TS6133 for individual unused imports but missing TS6192 for when ALL imports in a declaration are unused.

**Location to investigate**:
- `crates/tsz-checker/src/` - unused import tracking
- Need to detect when entire import declaration is unused vs individual imports

---

### 5. Parser Error Code Mismatches (TS1005)
**Impact**: 37 missing, 40 extra

Example: `restElementWithInitializer1.ts`
```typescript
var [...x = a] = a;
// Expected: [TS1186] (rest element cannot have initializer)
// Actual: [TS1005] (expected token)
```

**Issue**: Detecting syntax errors but emitting wrong error codes

**Location to investigate**:
- `crates/tsz-parser/src/` - parser diagnostic codes
- Need to map specific syntax errors to correct diagnostic codes

---

### 6. Protected Member Access in Nested Classes
**Impact**: Several false positives

Example: `protectedClassPropertyAccessibleWithinNestedClass.ts`
```typescript
class C {
    protected x: string;
    protected bar() {
        class C2 {
            protected foo() {
                let x: C;
                var x1 = x.foo;  // Should be allowed
                var x2 = x.bar;  // Should be allowed
            }
        }
    }
}
```

**Issue**: Incorrectly reporting [TS2302, TS2339] for accessing protected members from nested class

**Location to investigate**:
- `crates/tsz-checker/src/` - accessibility checking
- Need to properly handle nested class access to outer class protected members

---

### 7. Cross-File Namespace Merging
**Impact**: Several false positives

Example: `useBeforeDeclaration.ts`
```typescript
// A.ts
namespace ts {
    export function printVersion() {
        log("Version: " + sys.version);  // sys defined in B.ts
    }
}

// B.ts
namespace ts {
    export let sys:{version:string} = {version: "2.0.5"};
}
```

**Issue**: Not properly merging namespace declarations across files

**Location to investigate**:
- `crates/tsz-binder/src/` - namespace merging
- `crates/tsz-checker/src/` - cross-file symbol resolution

---

## Error Code Statistics

### False Positives (We emit, shouldn't)
1. TS2339: 151 tests (mostly Readonly bug)
2. TS2322: 77 tests
3. TS2345: 50 tests
4. TS1005: 40 tests
5. TS1128: 33 tests

### Missing Implementations (Never emitted)
1. TS2343: 35 tests (import helpers - out of scope for type checker)
2. TS1501: 19 tests
3. TS1362: 14 tests
4. TS2792: 13 tests
5. TS1361: 13 tests

### Partially Implemented (Missing in some cases)
1. TS2322: 69 tests
2. TS2304: 46 tests
3. TS2339: 42 tests
4. TS1005: 37 tests
5. TS2345: 29 tests

---

## Recommended Priority

### High Priority (Good ROI)
1. **TS6192 Implementation** - Simpler, affects many close-to-passing tests
2. **Parser Error Codes** - Wrong codes being emitted, straightforward fixes
3. **TS2322 Yield Expression** - Specific gap, well-defined fix

### Medium Priority
4. **Protected Member Access** - Accessibility checking refinement
5. **Cross-File Namespace Merging** - Binder/resolver work

### Complex (Requires Deep Investigation)
6. **Readonly<T> Generic Bug** - Already investigated, needs careful fix
7. **TS2339 General Gaps** - Varied scenarios, need case-by-case analysis

---

## Next Steps

1. Start with **TS6192** (unused imports) - clear scope, good test coverage
2. Fix **parser error code mismatches** - improve diagnostic accuracy
3. Tackle **TS2322 yield expression** gap - specific, testable
4. Return to **Readonly bug** with fresh perspective or ask for help via tsz-gemini

---

## Notes

- Always run `cargo nextest run` before committing
- Sync with main after every commit: `git pull --rebase origin main && git push origin main`
- Use `tsz-tracing` skill for debugging complex issues
- Document complex bugs in `docs/investigations/`
