# Conformance Analysis: Top Missing Errors and Root Cause Investigation

**Date**: 2026-01-24
**Baseline Pass Rate**: 41.5%
**Worker**: worker-14

## Executive Summary

Analysis of the top missing TypeScript error codes reveals that the emission infrastructure is largely in place, but these errors are being suppressed or not triggered in cases where TSC emits them.

## Top Missing Errors (Priority Order)

| Error Code | Missing Count | Description | Root Cause Category |
|------------|---------------|-------------|---------------------|
| **TS2318** | 3,386 | Cannot find global type | Lib context loading |
| **TS2307** | 2,139 | Cannot find module | Module resolution fallback |
| **TS2304** | 1,977 | Cannot find name | Symbol resolution permissiveness |
| **TS2488** | 1,749 | Type must have Symbol.iterator | Iterator protocol checks |
| **TS2583** | 706 | Change target library? | ES version filtering |

## Detailed Analysis

### TS2318: Cannot Find Global Type

**Current Implementation Status**: ✅ INFRASTRUCTURE EXISTS

**Location**:
- `src/checker/state.rs` lines 1187-1191
- `src/checker/error_reporter.rs` line 679 (error_cannot_find_global_type)
- `src/checker/type_checking.rs` line 3244 (is_known_global_type_name)

**Root Cause - Why We're MISSING These Errors**:

1. **Lib Context Loading is Too Permissive**
   - The lib.d.ts file might contain ALL ES6+ types regardless of target
   - When `compilerOptions.target` is set to ES5, types like `Promise`, `Map`, `Set` should NOT be available
   - Current code in `is_known_global_type_name` includes ES2015+ types without target checking

2. **Missing Target-Based Filtering**
   - No check of `ctx.compiler_options.target` before looking up global types
   - Lib symbols should be filtered based on ES version requirements:
     ```rust
     // MISSING: Check target before allowing global type
     if matches!(name, "Promise|Map|Set|Symbol") {
         if self.ctx.compiler_options.target <= Target::ES5 {
             // Should emit TS2583, not return the type
         }
     }
     ```

3. **Symbol Resolution Falls Back Too Early**
   - When a global type isn't found, code might be returning `TypeId::ANY` instead of `TypeId::ERROR`
   - This suppresses the error because `ANY` is compatible with everything

**Evidence from Code**:
```rust
// src/checker/state.rs:1187-1191
if self.is_known_global_type_name(name) {
    // TS2318/TS2583: Emit error for missing global type
    self.error_cannot_find_global_type(name, type_name_idx);
    return TypeId::ERROR;  // ✅ Correct - error IS emitted here
}
```

The error emission code exists, so the issue is that this code path isn't being reached in cases where TSC would emit it.

**Fix Strategy**:
1. Add target-based filtering to `resolve_named_type_reference`
2. Track ES version requirements for each global type
3. Filter lib context symbols based on compiler options.target
4. Ensure `resolve_named_type_reference` returns `None` (not `ANY`) when type not found for target

**Expected Impact**: +2,500 to +3,000 TS2318 errors added

---

### TS2307: Cannot Find Module

**Current Implementation Status**: ✅ INFRASTRUCTURE EXISTS

**Locations**:
- `src/checker/state.rs` - `emit_module_not_found_error`
- `src/checker/type_computation.rs` lines 4796-4801 (ES6 imports)
- `src/checker/type_computation.rs` lines 4883-4888 (require())
- `src/module_resolver.rs` - Module resolution

**Root Cause - Why We're MISSING These Errors**:

1. **Silent ANY Fallback for Unresolved Modules**
   ```rust
   // src/checker/type_computation.rs:4797-4798
   self.emit_module_not_found_error(module_name, value_decl);
   return TypeId::ANY;  // ⚠️ PROBLEM: Returns ANY suppresses further errors
   ```

   When a module isn't found, we emit TS2307 but then return `ANY`. TSC likely:
   - Returns `ERROR` type instead of `ANY`
   - Continues checking with the error type, which affects downstream type checking

2. **Module Resolution is Too Permissive**
   - The module resolver might be accepting relative paths that should fail
   - No validation of module specifier format
   - `module.exports` lookups might be falling back to empty instead of error

3. **Import Alias Resolution Returns Types**
   ```rust
   // src/checker/state.rs:4808-4841 (ES6 imports)
   if let Some(ref module_name) = symbol.import_module {
       if let Some(exports_table) = self.ctx.binder.module_exports.get(module_name)
           && let Some(export_sym_id) = exports_table.get(export_name)
       {
           let result = self.get_type_of_symbol(export_sym_id);
           return (result, Vec::new());
       }
       // ⚠️ PROBLEM: Falls through to return ANY instead of ERROR
       self.emit_module_not_found_error(module_name, value_decl);
       return (TypeId::ANY, Vec::new());
   }
   ```

**Fix Strategy**:
1. Change unresolved module return type from `ANY` to `ERROR`
2. Ensure module resolution validates paths correctly
3. Track modules that emitted TS2307 and return `ERROR` type for their imports

**Expected Impact**: +1,500 to +2,000 TS2307 errors added

---

### TS2583: Change Target Library?

**Current Implementation Status**: ✅ INFRASTRUCTURE EXISTS

**Locations**:
- `src/checker/error_reporter.rs` line 679 (`error_cannot_find_global_type`)
- `src/lib_loader.rs` line 22 (`MISSING_ES2015_LIB_SUPPORT`)
- `src/checker/state.rs` lines 1034, 2836, 8473-8480

**Root Cause - Why We're MISSING These Errors**:

The code already has the logic to emit TS2583! The issue is:

1. **Target Option Not Checked During Type Resolution**
   - `compiler_options.target` is not consulted when resolving global types
   - All ES2015+ types are available in lib.d.ts regardless of target setting

2. **ES Version Requirements Not Enforced**
   - Types like `Promise`, `Map`, `Set`, `Symbol` require ES2015+
   - Types like `async`/`await` require ES2017+
   - Types like `BigInt` require ES2020+

**Evidence from Code**:
```rust
// src/lib_loader.rs:18-22
pub const CANNOT_FIND_GLOBAL_TYPE: u32 = 2318;
pub const MISSING_ES2015_LIB_SUPPORT: u32 = 2583;

// src/checker/error_reporter.rs:679-693
pub fn error_cannot_find_global_type(&mut self, name: &str, idx: NodeIndex) {
    let is_es2015_type = lib_loader::is_es2015_plus_type(name);

    let (code, message) = if is_es2015_type {
        (
            lib_loader::MISSING_ES2015_LIB_SUPPORT,  // ✅ TS2583
            "Do you need to change your target library?..."
        )
    } else {
            (CANNOT_FIND_GLOBAL_TYPE,  // ✅ TS2318
             "Cannot find global type...")
        )
    };
}
```

The error code is correct! The issue is that for ES2015+ types when target < ES2015, we're finding the type successfully (returning a valid TypeId) instead of returning ERROR.

**Fix Strategy**:
1. In `resolve_named_type_reference`, check `ctx.compiler_options.target`
2. For ES2015+ types when target < ES2015, return `None` to trigger error path
3. Maintain mapping of types -> minimum ES version:
   ```rust
   const MIN_ES_VERSION: &[(&str, Target)] = &[
       ("Promise", Target::ES2015),
       ("Map", Target::ES2015),
       ("Set", Target::ES2015),
       ("Symbol", Target::ES2015),
       ("async", Target::ES2017),
       ("BigInt", Target::ES2020),
   ];
   ```

**Expected Impact**: +500 to +700 TS2583 errors added

---

## The "Any Poisoning" Effect

**Definition**: When `TypeId::ANY` is returned instead of `TypeId::ERROR`, it "poisons" the type checking results by making everything compatible, suppressing subsequent errors.

**Locations Where Any is Incorrectly Returned**:

1. **Module Resolution Fallback** (TS2307 suppression):
   ```rust
   // src/checker/type_computation.rs:4798
   self.emit_module_not_found_error(module_name, value_decl);
   return TypeId::ANY;  // ❌ Should be ERROR
   ```

2. **Unresolved Import Symbols**:
   ```rust
   // src/checker/state.rs:1194-1196
   if self.is_unresolved_import_symbol(type_name_idx) {
       return TypeId::ANY;  // ❌ Should be ERROR
   }
   ```

3. **Generic Type Arguments with Errors**:
   - When type argument inference fails, returns `ANY`
   - Should return `ERROR` to expose invalid type usage

**Impact of Any Poisoning**:
- Each `ANY` return suppresses MULTIPLE downstream errors
- A single missing module (TS2307) returning `ANY` can suppress:
  - Invalid property accesses (would be TS2339)
  - Type incompatibility errors (would be TS2322)
  - Method call errors (would be TS2339, TS2345)

**Estimated Error Suppression**:
- Each `ANY` fallback likely suppresses 3-5 additional errors
- 2,139 missing TS2307 errors × 3 = ~6,400 additional errors suppressed

---

## Prioritized Fix Roadmap

### Priority 1: Fix Module Resolution Any Fallback (HIGHEST IMPACT)

**File**: `src/checker/type_computation.rs`
**Lines**: 4797-4798, 4883-4888, 4841

**Change**:
```rust
// BEFORE:
self.emit_module_not_found_error(module_name, value_decl);
return TypeId::ANY;

// AFTER:
self.emit_module_not_found_error(module_name, value_decl);
return TypeId::ERROR;  // Expose type errors instead of suppressing
```

**Expected Impact**:
- Direct: +2,100 TS2307 errors (currently missing)
- Indirect: +4,000+ other errors (no longer poisoned by ANY)

---

### Priority 2: Add Target-Based Global Type Filtering

**File**: `src/checker/state.rs`
**Function**: `resolve_named_type_reference` (around line 1180)

**Change**:
```rust
// Add target checking before looking up global types
if self.is_es2015_plus_type(name)
    && self.ctx.compiler_options.target < Target::ES2015
{
    // Let the error path handle it - will emit TS2583
    return None;
}
```

**Expected Impact**:
- +3,000 TS2318/TS2583 errors
- Proper lib filtering based on target

---

### Priority 3: Remove Unresolved Import Any Fallback

**File**: `src/checker/state.rs`
**Line**: 1194-1196

**Change**:
```rust
// BEFORE:
if self.is_unresolved_import_symbol(type_name_idx) {
    return TypeId::ANY;
}

// AFTER:
if self.is_unresolved_import_symbol(type_name_idx) {
    return TypeId::ERROR;  // TS2307 was emitted, return error type
}
```

**Expected Impact**:
- +1,500 additional errors (currently suppressed)

---

## Conformance Test Analysis

### Test Cases to Verify

**TS2318 Test Cases** (global type not found):
```typescript
// @target: ES5
const p = new Promise();  // Should emit TS2583 (Promise requires ES2015+)
const m = new Map();       // Should emit TS2583 (Map requires ES2015+)

// @target: ES5
interface I extends Promise {} }  // Should emit TS2583
function f(): Set {}              // Should emit TS2583
```

**TS2307 Test Cases** (module not found):
```typescript
import { Missing } from './does-not-exist';  // Should emit TS2307
const x = Missing.someMethod;  // Should ALSO emit TS2339 (error propagation)

import * as NS from './missing-module';  // Should emit TS2307
NS.member;  // Should ALSO emit TS2339
```

**TS2583 Test Cases** (target library mismatch):
```typescript
// @target: ES3
const arr = new Array(5);  // Should work (Array in ES3+)
const map = new Map();      // Should emit TS2583 (Map requires ES2015+)
```

---

## Estimated Conformance Improvement

**Current Baseline**: 41.5% pass rate

**After Priority 1 Fix** (Module Resolution):
- Pass rate: 41.5% → 38% (more errors detected = lower pass)
- Error detection: +6,000 errors

**After Priority 2 Fix** (Target Filtering):
- Pass rate: 38% → 35% (ES version errors detected)
- Error detection: +3,000 errors

**After Priority 3 Fix** (Import Any Poisoning):
- Pass rate: 35% → 33% (error propagation)
- Error detection: +1,500 errors

**Final Expected State**:
- **Pass rate**: ~33% (intentionally lower - better error detection)
- **Total new errors**: +10,500
- **Why lower pass rate is better**: More errors = better conformance with TSC

---

## Next Steps

1. ✅ Analysis complete
2. 🔜 Implement Priority 1 fix (Module Resolution Any Fallback)
3. 🔜 Implement Priority 2 fix (Target-Based Filtering)
4. 🔜 Implement Priority 3 fix (Import Any Poisoning)
5. 🔜 Run conformance tests to verify improvements
6. 🔜 Measure actual error count increases
7. 🔜 Document final results
