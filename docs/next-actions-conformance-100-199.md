# Next Actions: Conformance Tests 100-199

**Current Status:** 85/100 passing (85.0%)
**Target:** Maximize pass rate

## Investigation Complete

See `docs/conformance-100-199-analysis.md` for full analysis of all 15 failing tests.

## Recommended Fix Priority (Easiest to Hardest)

### 1. Fix Symbol.iterator Recognition ⭐ HIGH VALUE
**Impact:** Fixes 1 test immediately (`argumentsObjectIterator02_ES6.ts`), likely improves others
**Complexity:** Medium
**Issue:** TypeScript's Symbol type includes well-known symbols like `Symbol.iterator` but we don't recognize them

**What's needed:**
- Find where TypeScript lib files are loaded (lib.es6.d.ts, lib.es2015.iterable.d.ts)
- Ensure `SymbolConstructor` interface includes:
  - `readonly iterator: unique symbol`
  - `readonly asyncIterator: unique symbol`
  - `readonly hasInstance: unique symbol`
  - etc.
- OR: Hardcode well-known symbol properties if we're not using lib files

**Test case:**
```typescript
function foo() {
    let it = arguments[Symbol.iterator];  // Should work, we emit TS2339
    for (let arg of it()) {}  // Should work, we emit TS2488
}
```

**Files to check:**
- Where built-in types are defined/injected
- How lib.d.ts files are loaded
- Type resolution for `Symbol.iterator`

### 2. Implement TS1210 Error 🏆 QUICK WIN
**Impact:** Fixes 1 test (`argumentsReferenceInConstructor4_Js.ts`)
**Complexity:** Medium-High (requires binder changes)
**Issue:** Using `arguments` as a variable name in ES6 class context (strict mode)

**What's needed:**
- Check if we're in a class context (classes are strict mode)
- Detect `const/let/var arguments = ...` declarations
- Emit TS1210 error instead of allowing it

**Error message:**
```
TS1210: Code contained in a class is evaluated in JavaScript's strict mode which does not allow this use of 'arguments'.
```

**Files to modify:**
- `crates/tsz-common/src/diagnostics.rs` - Add TS1210 definition
- `crates/tsz-binder/src/` - Add check when binding variable declarations
- May need to track "in class body" context

### 3. Add emit_declaration_only to CheckerOptions 📋 MEDIUM VALUE
**Impact:** Could fix 3 tests with declaration emit
**Complexity:** Medium (requires understanding what checks to skip)
**Issue:** When `emitDeclarationOnly: true`, TypeScript is more lenient with type checking

**What's needed:**
- Add `emit_declaration_only: bool` to `CheckerOptions` in `crates/tsz-common/src/checker_options.rs`
- Thread the option from CLI args to checker
- Determine which checks to skip when this is true (needs TypeScript behavior analysis)
- Tests affected:
  - `amdDeclarationEmitNoExtraDeclare.ts`
  - `amdLikeInputDeclarationEmit.ts`
  - `anonClassDeclarationEmitIsAnon.ts`

**Research needed:**
- What exactly does TypeScript skip when `emitDeclarationOnly: true`?
- Does it skip all type checking or just certain errors?

### 4. Fix Named Default Import Error Code (TS1192 → TS2305) 🐛 SMALL FIX
**Impact:** Fixes 1 test (`allowSyntheticDefaultImports8.ts`)
**Complexity:** Low (logic exists, might just be a bug)
**Issue:** `import { default }` should emit TS2305 not TS1192

**Code location:** `crates/tsz-checker/src/state_type_resolution.rs:1824-1827`

The logic to detect named default imports exists (lines 1746-1822) and should call `emit_no_exported_member_error`. Need to debug why it's not working for the test case.

**Test case:**
```typescript
// @allowSyntheticDefaultImports: false
// b.d.ts
export function foo();

// a.ts
import { default as Foo } from "./b";  // Should be TS2305, we emit TS1192
```

### 5. Fix Ambient Declaration Merging 🔧 COMPLEX
**Impact:** Could fix 2-3 tests
**Complexity:** High (requires understanding declaration merging rules)
**Issue:** Namespace + declare class with same name not handled correctly

**Test:** `ambientClassDeclarationWithExtends.ts`
```typescript
declare class C { public foo; }
namespace D { var x; }
declare class D extends C { }
var d: C = new D();  // We incorrectly error here
```

**What's needed:**
- Understand TypeScript's declaration merging rules for ambient contexts
- Fix binder to properly merge namespace and class declarations
- Ensure merged type is constructable

## Strategy

**For this session:**
1. Start with #4 (Named Default Import) - smallest, should be quick
2. Then #1 (Symbol.iterator) - high value, medium complexity
3. Save #2, #3, #5 for future sessions

**Testing workflow:**
1. Make fix
2. Run `cargo nextest run` to ensure no regressions
3. Run `./scripts/conformance.sh run --max=100 --offset=100` to see improvement
4. Commit with clear message
5. Sync: `git pull --rebase origin main && git push origin main`

## Files to Reference

- Analysis: `docs/conformance-100-199-analysis.md`
- Checker: `crates/tsz-checker/src/`
- Diagnostics: `crates/tsz-common/src/diagnostics.rs`
- Checker Options: `crates/tsz-common/src/checker_options.rs`
- Type Resolution: `crates/tsz-checker/src/state_type_resolution.rs`
