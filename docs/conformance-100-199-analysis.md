# Conformance Tests 100-199 Analysis

**Current Status: 85/100 passing (85.0%)**
**Goal: Maximize pass rate**

## Summary

- 15 failing tests
- 8 false positives (we emit errors, TSC doesn't)
- 7 unimplemented error codes
- 3 "close" tests (differ by 1-2 codes)

##False Positives (8 tests)

These tests expect NO errors but tsz emits errors:

### 1. `argumentsObjectIterator02_ES6.ts`
**Errors we emit:** TS2339, TS2488
**Root cause:** Symbol.iterator not recognized

```typescript
function foo(x: number) {
    let blah = arguments[Symbol.iterator];  // TS2339: Property 'iterator' does not exist
    for (let arg of blah()) {}  // TS2488: Type must have '[Symbol.iterator]()' method
}
```

**Issue:** We don't recognize well-known symbols. TSC knows `Symbol.iterator` is valid.

**Fix needed:** Ensure Symbol type includes well-known symbol properties:
- Symbol.iterator
- Symbol.hasInstance
- Symbol.toStringTag
- etc.

### 2. `ambientClassDeclarationWithExtends.ts`
**Errors we emit:** TS2322, TS2449
**Pattern:** Namespace + declare class with same name (declaration merging)

```typescript
declare class C { public foo; }
namespace D { var x; }
declare class D extends C { }
var d: C = new D();  // We incorrectly error here
```

**Issue:** Not handling namespace + class declaration merging in ambient context.

### 3. `amdDeclarationEmitNoExtraDeclare.ts`
**Errors we emit:** TS2322, TS2345
**Options:** `declaration: true, module: amd, outfile: dist.js`

```typescript
import { Configurable } from "./Configurable"
export class HiddenClass {}
export class ActualClass extends Configurable(HiddenClass) {}
```

**Pattern:** Mixin pattern with generic constraints during declaration emit.

### 4. `amdModuleConstEnumUsage.ts`
**Errors we emit:** TS2339
**Options:** `module: amd, preserveConstEnums: true, baseUrl: /proj`

```typescript
// defs/cc.ts
export const enum CharCode { A, B }

// component/file.ts
import { CharCode } from 'defs/cc';
if (CharCode.A === input) {}  // We incorrectly error here
```

**Issue:** Const enum imports with baseUrl resolution.

### 5. `amdLikeInputDeclarationEmit.ts`
**Errors we emit:** TS2339
**Options:** `checkJs: true, allowJs: true, declaration: true, emitDeclarationOnly: true`

**Issue:** False positives during declaration-only emit from JS files.

### 6. `anonClassDeclarationEmitIsAnon.ts`
**Errors we emit:** TS2345
**Options:** `declaration: true`

```typescript
export type Constructor<T = {}> = new (...args: any[]) => T;
export function Timestamped<TBase extends Constructor>(Base: TBase) {
    return class extends Base { timestamp = Date.now(); };
}
export class TimestampedUser extends Timestamped(User) {}  // We incorrectly error
```

**Issue:** Generic constraint checking too strict for mixin pattern.

### 7. `ambientExternalModuleWithInternalImportDeclaration.ts`
**Errors we emit:** TS2708
**Options:** `module: amd`

**Pattern:** Internal import alias in ambient module declaration.

### 8. `ambientExternalModuleWithoutInternalImportDeclaration.ts`
**Errors we emit:** TS2351
**Options:** `module: amd`

**Pattern:** Similar to #7 but without internal import.

## Close to Passing (3 tests)

### 1. `allowSyntheticDefaultImports8.ts`
**Expected:** TS2305
**We emit:** TS1192

```typescript
// @allowSyntheticDefaultImports: false
export function foo();
export function bar();

// a.ts
import { default as Foo } from "./b";  // Should be TS2305, we emit TS1192
```

**Fix:** When `allowSyntheticDefaultImports: false`, importing non-existent `default` member should emit TS2305 (no exported member) not TS1192 (no default export).

### 2. `ambientExportDefaultErrors.ts`
**Expected:** TS2714
**We emit:** TS2304

**Issue:** Wrong error code for ambient default export with `export as namespace`.

### 3. `ambiguousGenericAssertion1.ts`
**Expected:** TS1005, TS1109, TS2304
**We emit:** TS1005, TS1109, TS1434

```typescript
var r3 = <<T>(x: T) => T>f;  // Parser ambiguity
```

**Issue:** We emit TS1434 (Unexpected keyword) instead of TS2304 (Cannot find name).

## Missing Error Codes (7 codes)

Error codes we never emit that TSC does:

1. **TS2305** - Module has no exported member 'X' (1 test)
2. **TS2714** - The expression of an export assignment must be an identifier or qualified name in an ambient context (1 test)
3. **TS1437** - Module declaration names may only use quoted strings (1 test)
4. **TS2580** - Cannot find name 'X'. Do you need to install type definitions? (1 test)
5. **TS1210** - Invalid use of 'arguments'. Modules cannot reference 'arguments' of outer function. (1 test - QUICK WIN!)
6. **TS2585** - Promise constructor cannot be used to wrap a function that returns a Promise (1 test)
7. **TS7006** - Parameter 'X' implicitly has an 'any' type (1 test)

## Common Patterns in False Positives

1. **Ambient declarations** (3 tests)
   - `declare class`, `declare module`
   - We're likely too strict with ambient contexts

2. **Declaration emit** (3 tests)
   - `declaration: true`, `emitDeclarationOnly: true`
   - TSC performs less strict checking during declaration-only emit
   - Our checker doesn't check `emit_declaration_only` option

3. **Module + const enum** (1 test)
   - baseUrl resolution + const enum imports

4. **Well-known symbols** (1 test)
   - Symbol.iterator and other well-known symbols not recognized

## Recommended Priorities

### High Impact (should fix multiple tests):

1. **Fix Symbol.iterator recognition** - Affects iterator tests, potentially more
2. **Handle emit_declaration_only mode** - Could fix 3 tests immediately
3. **Implement TS1210** - Quick win, 1 test fixed immediately

### Medium Impact:

4. Fix ambient declaration merging (namespace + class)
5. Fix const enum imports with baseUrl
6. Implement TS2305, TS2714 error codes

### Lower Priority:

7. Fix edge case error codes (TS1437, TS2580, TS2585)

## Next Steps

1. **Immediate:** Check if checker respects `emit_declaration_only` compiler option
2. **Quick win:** Implement TS1210 for arguments reference error
3. **High value:** Fix Symbol.iterator to work with well-known symbols
4. **Systematic:** Review ambient declaration handling in binder/checker

## Code Locations

- Checker: `crates/tsz-checker/src/checker/`
- Solver: `crates/tsz-checker/src/solver/`
- Diagnostics: `crates/tsz-common/src/diagnostics.rs`
- Compiler options: `crates/tsz-cli/src/args.rs`
