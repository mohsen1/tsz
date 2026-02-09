# Conformance Test Investigation - Slice 2

## Overview

This document summarizes the investigation into improving conformance test pass rates for test slice 2 (tests 3,101-6,201 out of 12,404 total tests).

**Current Status**: 1,734/3,030 tests passing (57.2%)

## Test Categories

### False Positives (407 tests)
We emit errors that TSC doesn't. Top issues:
- **TS2339** (89 tests): Property access errors
- **TS2322** (86 tests): Type assignment errors
- **TS2345** (77 tests): Argument type errors
- **TS7006** (35 tests): Implicit any parameter
- **TS1005** (28 tests): Token expected errors

### Missing Errors (434 tests)
TSC emits errors we don't. Top issues:
- **TS2307** (42 tests): Module not found
- **TS6053** (39 tests): File location errors
- **TS2874** (28 tests): Duplicate function implementation
- **TS1206** (13 tests): Cannot find module 'tslib' (import helpers)
- **TS2300** (1 test): Duplicate identifier

### Wrong Error Codes (453 tests)
Both emit errors but with different codes. Top extra codes we emit:
- **TS2322** (313 tests): Type assignment errors
- **TS1005** (197 tests): Token expected
- **TS2694** (189 tests): Namespace has no exported member
- **TS2339** (175 tests): Property does not exist
- **TS2345** (170 tests): Argument type errors

### Close to Passing (166 tests)
Differ by only 1-2 error codes. Examples:
- `indexedAccessConstraints.ts`: Expected [TS2322], got [TS2322, TS18050]
- `importDeclarationInModuleDeclaration1.ts`: Expected [TS1147], got [TS1147, TS2307]

## Specific Issues Investigated

### 1. Namespace Member Exports in Ambient Modules

**Issue**: TypeScript allows namespace members to be accessible without `export` keyword when inside ambient module declarations.

```typescript
declare module "m" {
    namespace x {
        interface c { }  // No export keyword needed
    }
    type T = x.c;  // TSC allows this
}
```

**Attempted Fix**:
- Added `in_ambient_module` flag to track ambient declaration context
- Modified `populate_module_exports` to make namespace members accessible in ambient contexts

**Result**: REVERTED
- Fix didn't work as expected
- The logic seemed correct but tests still failed
- Needs deeper investigation into how TypeScript's binder/checker coordinate

**Root Cause**: Unknown. Possible issues:
- Flag not propagating correctly through nested declarations
- Symbol exports table not checked correctly during resolution
- Different code path for type vs import resolution

### 2. Duplicate Import Declarations (TS2300)

**Issue**: TypeScript should emit TS2300 for duplicate `import =` declarations.

```typescript
namespace m {
  export var m = '';
}

import x = m.m;
import x = m.m;  // Should emit TS2300: Duplicate identifier 'x'
```

**Attempted Fix**:
1. Changed `bind_import_equals_declaration` to use `declare_symbol` instead of directly allocating symbols
2. Added ALIAS case to `excluded_symbol_flags` function to make ALIAS + ALIAS conflict

**Result**: REVERTED
- Changes broke 7 unit tests related to namespace alias member resolution
- Example: `test_namespace_value_member_access` had TypeId mismatch
- Root cause: `declare_symbol` changes how symbols are created/merged, affecting type resolution

**Lessons Learned**:
- Import aliases have special binding semantics that can't be changed lightly
- Using `declare_symbol` for aliases affects how the type system resolves aliased types
- The duplicate checking needs to happen WITHOUT changing how aliases are bound
- Alternative approach: Add duplicate checking AFTER binding, in checker phase

### 3. Specific Test Patterns

#### indexedAccessConstraints.ts (close to passing)
- **Expected**: [TS2322]
- **Actual**: [TS2322, TS18050]
- **Issue**: Line 15 `return fn.length` incorrectly reports TS18050 "The value 'never' cannot be used here"
- **Root cause**: After `if (typeof fn !== 'function')` guard, `fn` should narrow to `T[K] & Function` but we're narrowing to `never`
- **Complexity**: Medium - requires fixing narrowing for generic indexed access types

#### inKeywordAndIntersection.ts (false positive)
- **Expected**: No errors
- **Actual**: [TS2339] on `instance.one()`
- **Issue**: After `instanceof ClassOne` check, type should narrow to `InstanceOne`
- **Root cause**: `ClassOne` is typed as `{ new(): InstanceOne }`, so instanceof should narrow to the constructor's return type
- **Complexity**: High - requires instanceof narrowing for intersection types with constructor signatures

#### implicitConstParameters.ts (false positive)
- **Expected**: No errors
- **Actual**: Multiple TS2339 and TS18048 errors
- **Issue**: Parameters captured in closures aren't treated as "const" for narrowing
- **Root cause**: After `if (typeof x === 'number')`, parameter `x` should stay narrowed even inside arrow function closures
- **Complexity**: High - requires implicit const parameter detection and cross-closure narrowing

## Recommended Next Steps

### High-Impact, Lower Complexity Fixes

1. **TS2339 False Positives** (89 tests)
   - Many related to narrowing and control flow analysis
   - Focus on specific patterns (instanceof, typeof guards, closures)

2. **TS1206 Missing Errors** (13 tests)
   - Check if `importHelpers` compiler option is set
   - Verify `tslib` module can be resolved
   - Emit TS1206 if not found
   - Relatively isolated, unlikely to break other tests

3. **TS18050 Extra Errors** (appears in close-to-passing tests)
   - "The value 'never' cannot be used here"
   - Investigate narrowing logic producing `never` incorrectly
   - Check generic indexed access type narrowing

### Medium Complexity Fixes

4. **TS2694 Extra Errors** (189 tests in wrong-code category)
   - "Namespace has no exported member"
   - Namespace exports in ambient modules (previously investigated)
   - Needs architectural understanding

5. **TS2322/TS2345 False Positives** (86 + 77 = 163 tests)
   - Type assignment and argument type errors
   - Likely related to type compatibility checks
   - Could be multiple different root causes

### Investigation Needed

6. **TS2300 Duplicate Identifiers**
   - Import alias duplicates
   - Need to add checking in checker phase, not binder
   - Look at existing passing TS2300 tests for patterns

7. **Namespace Member Exports**
   - Ambient module context handling
   - Consider using tsz-gemini skill for architectural guidance
   - May require significant refactoring

## Tools and Techniques

### For Debugging

1. **Use tracing infrastructure**:
   ```bash
   TSZ_LOG=debug TSZ_LOG_FORMAT=tree cargo run -- test.ts 2>&1 | head -200
   ```

2. **Filter to specific modules**:
   ```bash
   TSZ_LOG="tsz_binder::state_binding=trace" TSZ_LOG_FORMAT=tree cargo run -- test.ts
   ```

3. **Conformance analysis**:
   ```bash
   ./scripts/conformance.sh analyze --offset 3101 --max 3101 --category false-positive
   ```

### For Implementation

1. **Start with unit tests**: Write failing test first, then implement fix
2. **Test incrementally**: Make small changes, run tests frequently
3. **Check TSC source**: For ambiguous cases, reference TypeScript's implementation
4. **Use tsz-gemini skill**: For architectural questions and guidance

## Time Spent

- Namespace exports investigation: ~2 hours (reverted)
- False positive investigation: ~30 minutes (patterns identified)
- Duplicate import fix: ~1.5 hours (reverted)
- Documentation: ~30 minutes
- **Total**: ~4.5 hours

## Key Learnings

1. **Start simpler**: Complex issues like namespace exports require deeper understanding
2. **Use tracing early**: Helps identify issues quickly rather than guessing
3. **Check existing tests**: Look at passing unit tests to understand correct patterns
4. **Isolated changes**: Fixes that modify core binding logic can have wide-ranging effects
5. **Test frequently**: Small, incremental changes with frequent testing catch regressions early
