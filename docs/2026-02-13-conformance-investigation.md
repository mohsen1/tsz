# Conformance Tests 100-199 Investigation (2026-02-13)

## Current Status
**Pass Rate: 90/100 (90%)**

## Summary of Findings

### 1. JavaScript Type Checking Not Working (CRITICAL)
**Impact:** 2-3 tests failing

TSZ does not perform type checking on JavaScript files, even with `--allowJs --checkJs` flags.

**Evidence:**
```javascript
// tmp/type-error.js
/** @type {number} */
let x = "string";

// TSC emits TS2322, TSZ emits nothing
```

**Affected Tests:**
- `argumentsReferenceInConstructor4_Js.ts` - Missing TS1210  
- `argumentsReferenceInFunction1_Js.ts` - Missing TS2345, TS7006

**Root Cause:**
- The checker may not be invoked for JS files
- Or `enclosing_class` tracking not working for JavaScript classes
- The TS1210 check (lines 835-848 in state_checking.rs) exists but only fires for TypeScript

**Next Steps:**
1. Investigate CLI/driver handling of `--checkJs` flag
2. Check if checker is being invoked for .js files
3. Verify binder creates proper symbols for JavaScript classes
4. Ensure `enclosing_class` is set when checking JavaScript constructors

### 2. False Positive Errors (6 tests)

#### TS2488: Arguments Symbol.iterator (1 test)
**Test:** `argumentsObjectIterator02_ES6.ts`
**Issue:** `arguments[Symbol.iterator]()` incorrectly inferred as `AbstractRange<any>` instead of `ArrayIterator<any>`

**Root Cause:**
- Property access `arguments[Symbol.iterator]` should resolve to iterator function
- Call result should be `ArrayIterator<any>` (which is iterable)
- Instead getting DOM type `AbstractRange<any>` (which lacks Symbol.iterator)

**Investigation:**
- IArguments interface correctly defined in es2015.iterable.d.ts:114
- Type computation for `arguments` identifier looks correct (type_computation.rs:598-607)
- Issue likely in:
  - Computed property access with Symbol values
  - Call expression type inference
  - Type resolution for iterator methods

#### TS2322/TS2345: Type Assignability (4 tests)
**Tests:**
- `ambientClassDeclarationWithExtends.ts` - TS2322 on `new D()` assignment
- `amdDeclarationEmitNoExtraDeclare.ts` - TS2322, TS2345 on class mixins
- `anonClassDeclarationEmitIsAnon.ts` - TS2345 on Timestamped(User)

**Pattern:** All involve:
- Class expressions or constructors
- Extends clauses
- Mixin patterns or ambient declarations

**Hypothesis:**
- Over-checking in declaration emit mode?
- Incorrect type inference for class expressions in return positions?
- Issues with mixin pattern type compatibility?

#### TS2339: Property Access (2 tests)  
**Tests:**
- `amdModuleConstEnumUsage.ts` - `CharCode.A` after import
- `amdLikeInputDeclarationEmit.ts` - `BaseClass.extends` method access

**Pattern:**
- Const enum member access post-import
- Static method access on imported types

**Hypothesis:**
- Const enums not being inlined correctly
- Module resolution issues with AMD modules
- Static member resolution in AMD context

### 3. Wrong-Code Errors (2 tests)

#### ambiguousGenericAssertion1.ts
- Expected: [TS1005, TS1109, TS2304]
- Actual: [TS1005, TS1109, TS1434]
- Issue: Parser treats `<<T>` as left-shift, should emit TS2304 "Cannot find name 'T'" instead of TS1434

#### argumentsObjectIterator02_ES5.ts
- Expected: [TS2585]
- Actual: [TS2495, TS2551]
- Issue: ES5 Symbol.iterator error code mismatch

## Recommended Priority Order

### Priority 1: JavaScript Type Checking (Highest Impact)
- **Potential Gain:** +2-3 tests
- **Effort:** High (architectural investigation needed)
- **Action:** Investigate CLI/driver `--checkJs` handling

### Priority 2: TS2339 Const Enum Issues (Medium Impact)
- **Potential Gain:** +2 tests
- **Effort:** Medium
- **Action:** Investigate const enum member resolution post-import

### Priority 3: TS2488 Iterator Type Inference (Low Impact)
- **Potential Gain:** +1 test
- **Effort:** Medium-High
- **Action:** Debug type inference for `arguments[Symbol.iterator]()`

### Priority 4: TS2322/TS2345 False Positives (Medium Impact)
- **Potential Gain:** +3-4 tests
- **Effort:** Medium-High
- **Action:** Investigate mixin/class expression type compatibility

## Files to Investigate

### JavaScript Checking:
- `crates/tsz-cli/src/driver.rs` - CLI argument handling
- `crates/tsz-checker/src/state_checking.rs` - Checker invocation
- `crates/tsz-binder/src/` - Symbol creation for JS files

### Type Inference Issues:
- `crates/tsz-checker/src/type_computation.rs` - Property access type
- `crates/tsz-checker/src/iterators.rs` - Iterator type checking
- `crates/tsz-checker/src/iterable_checker.rs` - Iterability validation

### Const Enum:
- `crates/tsz-checker/src/` - Const enum member resolution
- `crates/tsz-solver/` - Type resolution for imported enums

## Unit Test Status
All 2394 unit tests passing, 44 skipped. No regressions.

## Goal
Target 95/100 (95%) by fixing JavaScript checking (+2-3) and one category of false positives (+2-3).
