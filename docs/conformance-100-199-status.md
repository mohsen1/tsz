# Conformance Tests 100-199 - Current Status

**Last Updated:** 2026-02-13
**Current Pass Rate:** 89/100 (89.0%)
**Starting Pass Rate:** 85/100 (85.0%)
**Improvement:** +4 tests (+4%)

## Recent Fixes

### Fixed: Named Default Import Error Code (Commit a1f6a66fc)
**Tests Fixed:** ~4-5 tests
**Issue:** `import { default as Foo } from "./mod"` was emitting TS1192 instead of TS2305
**Fix:** Refactored `is_true_default_import_binding` check to use explicit variable assignment
**File:** `crates/tsz-checker/src/state_type_analysis.rs`

## Remaining Failures (11 tests)

### By Category

#### False Positives (6 tests) - We emit errors, TSC doesn't
1. **ambientClassDeclarationWithExtends.ts** - TS2322
2. **amdDeclarationEmitNoExtraDeclare.ts** - TS2322, TS2345
3. **amdModuleConstEnumUsage.ts** - TS2339
4. **anonClassDeclarationEmitIsAnon.ts** - TS2345
5. **amdLikeInputDeclarationEmit.ts** - TS2339
6. **argumentsObjectIterator02_ES6.ts** - TS2488

#### Missing Errors (2 tests) - We don't emit expected errors
1. **argumentsReferenceInConstructor4_Js.ts** - Missing TS1210
2. **argumentsReferenceInFunction1_Js.ts** - Missing TS2345, TS7006

#### Wrong Error Codes (2 tests) - Different codes emitted
1. **ambiguousGenericAssertion1.ts** - Emit TS1434, should emit TS2304
2. **argumentsObjectIterator02_ES5.ts** - Emit TS2495+TS2551, should emit TS2585

#### Other (1 test)
1. **ambientExternalModuleWithInternalImportDeclaration.ts** - Details TBD

## Priority Fixes (In Order)

### 1. Symbol.iterator Recognition (HIGH PRIORITY) 🔴
**Status:** Investigated, not yet fixed
**Impact:** 1-2 tests
**Complexity:** Medium

**Issue:** `Symbol.iterator` property not recognized
- Symbol type has other well-known symbols but missing `iterator`
- Error: TS2339 "Property 'iterator' does not exist"
- Affects: argumentsObjectIterator02_ES6.ts (maybe argumentsObjectIterator02_ES5.ts)

**Investigation:** See `docs/symbol-iterator-investigation.md`

**Root Cause:** Likely `lib.es2015.iterable.d.ts` not being loaded or `iterator` property filtered

**Next Steps:**
- Debug lib file loading for ES6 target
- Check which lib files are included
- Verify Symbol type construction
- May need to explicitly load iterable lib

**Files to Check:**
- `crates/tsz-binder/src/lib_loader.rs`
- `crates/tsz-cli/src/driver.rs` (lib selection)
- `crates/tsz-checker/src/type_computation_complex.rs` (Symbol type)

### 2. Implement TS1210 (QUICK WIN) ⚡
**Status:** Not started
**Impact:** 1 test
**Complexity:** Medium

**Issue:** Missing error code for `arguments` variable shadowing in strict mode

**Test:** argumentsReferenceInConstructor4_Js.ts
```javascript
class A {
    constructor() {
        const arguments = this.arguments;  // Should emit TS1210
    }
}
```

**Error Message:**
```
TS1210: Code contained in a class is evaluated in JavaScript's strict mode
which does not allow this use of 'arguments'.
```

**Implementation:**
1. Add TS1210 to `crates/tsz-common/src/diagnostics.rs`
2. Add check in binder when binding variable declarations
3. Track "in class body" context (classes are strict mode)
4. Emit error when binding variable named "arguments" in class

**Files to Modify:**
- `crates/tsz-common/src/diagnostics.rs` - Add diagnostic
- `crates/tsz-binder/src/` - Add validation

### 3. Ambient Declaration Merging (COMPLEX) 🔧
**Status:** Not started
**Impact:** 2-3 tests
**Complexity:** High

**Issue:** Namespace + declare class with same name not merged correctly

**Tests:**
- ambientClassDeclarationWithExtends.ts
- ambientExternalModuleWithInternalImportDeclaration.ts (maybe)

**Example:**
```typescript
declare class C { public foo; }
namespace D { var x; }
declare class D extends C { }
var d: C = new D();  // We incorrectly error with TS2322
```

**Root Cause:** Declaration merging rules for ambient contexts not fully implemented

**Next Steps:**
- Study TypeScript's declaration merging rules
- Check binder's merge logic for ambient declarations
- Ensure namespace + class declarations can merge
- Verify merged type is constructable

### 4. Declaration Emit Checks (COMPLEX) 📋
**Status:** Not started
**Impact:** 2-3 tests
**Complexity:** Medium-High

**Issue:** Type checking too strict when `emitDeclarationOnly: true`

**Tests:**
- amdDeclarationEmitNoExtraDeclare.ts
- amdLikeInputDeclarationEmit.ts
- anonClassDeclarationEmitIsAnon.ts

**Root Cause:** Checker doesn't respect `emit_declaration_only` compiler option

**Implementation:**
1. Add `emit_declaration_only: bool` to `CheckerOptions`
2. Thread option from CLI args to checker
3. Determine which checks to skip (research TSC behavior)
4. Conditionally skip those checks

**Files to Modify:**
- `crates/tsz-common/src/checker_options.rs`
- `crates/tsz-cli/src/args.rs` (parse option)
- `crates/tsz-checker/src/` (respect option)

### 5. Const Enum Imports (MEDIUM) 📦
**Status:** Not started
**Impact:** 1 test
**Complexity:** Medium

**Test:** amdModuleConstEnumUsage.ts

**Issue:** Const enum with `baseUrl` and module imports not working
```typescript
// defs/cc.ts
export const enum CharCode { A, B }

// component/file.ts
import { CharCode } from 'defs/cc';
if (CharCode.A === input) {}  // We emit TS2339
```

**Root Cause:** Const enum + baseUrl resolution interaction

### 6. Parser Error Recovery (LOW PRIORITY) 🔍
**Status:** Not started
**Impact:** 1 test
**Complexity:** Low-Medium

**Test:** ambiguousGenericAssertion1.ts

**Issue:** Parser ambiguity `<<T>` emits TS1434, should emit TS2304
```typescript
var r3 = <<T>(x: T) => T>f;  // Parser sees << operator
```

**Root Cause:** Error recovery produces different error code

**Fix:** Adjust parser error recovery to match TSC

## Testing Strategy

For each fix:
1. Create minimal reproduction in `tmp/`
2. Verify TSC behavior
3. Implement fix
4. Run `cargo nextest run` (no regressions)
5. Run conformance tests (verify improvement)
6. Commit with clear message
7. Sync: `git pull --rebase origin main && git push origin main`

## Goal Progression

- **Session 1 (Completed):** 85% → 89% (+4%)
- **Session 2 Target:** 89% → 93% (+4%)
- **Session 3 Target:** 93% → 96% (+3%)
- **Final Target:** 96% → 100% (+4%)

## Key Learnings

1. **Inline evaluation can hide bugs** - Store results explicitly
2. **Tracing is invaluable** - Add instrumentation early
3. **Start with close tests** - Easiest wins first
4. **Document investigations** - For future sessions
5. **Test incrementally** - Verify no regressions after each fix

## Documentation

- `docs/conformance-100-199-analysis.md` - Full failure analysis
- `docs/next-actions-conformance-100-199.md` - Action plan
- `docs/session-2026-02-13-conformance-fixes.md` - Session 1 summary
- `docs/symbol-iterator-investigation.md` - Symbol.iterator investigation
- `docs/conformance-100-199-status.md` - This document

## Code Health

✅ No regressions in unit tests (2394 passing)
✅ Added tracing infrastructure
✅ Improved code clarity
✅ All pre-commit hooks passing
✅ Comprehensive documentation

The codebase is ready for the next session with clear priorities and action items!
