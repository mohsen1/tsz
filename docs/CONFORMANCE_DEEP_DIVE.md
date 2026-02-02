# Conformance Deep Dive: Action Plan for Improving Pass Rate

**Generated:** 2025-02-02
**Overall Pass Rate:** 49.7% (6,147 passed, 6,194 failed, 12,341 total tests)
**Last Updated:** 2025-02-02 (Generic type system validation completed)

## Related Documents

- **`GENERICS_IMPLEMENTATION_PLAN.md`** - Comprehensive generics strategy with 4-sprint roadmap
- **`docs/architecture/NORTH_STAR.md`** - Target architecture (Solver-First)

## Executive Summary

This document analyzes the root causes of conformance test failures and provides an **action-oriented roadmap** to improve the pass rate from 49.7% to 60%+ by focusing on high-impact fixes.

**⚠️ Critical Findings from AI Peer Review:**
- **TS2403 approach is technically flawed** - `var_decl_types` cache won't contain lib symbols (SymbolConstructor, PromiseConstructor)
- **TS2304 is underestimated** - likely caused by type inference failures, not just scope lookup
- **Missing critical area:** Control Flow Analysis bugs may be causing cascading TS2304/TS2339 errors
- **Realistic ceiling:** ~60% pass rate without fixing Generic Inference and Overload Resolution

### Top 5 High-Impact Issues (by potential test count improvement)

| Priority | Error Code | Pass Rate | Impact | Effort | File(s) |
|----------|------------|-----------|--------|--------|---------|
| **P0** | TS2428 | 0% (missing) | 11 tests | Medium | `src/checker/interface_type.rs` |
| **P0** | TS2403 | 0.0% (0/123) | 123 tests | Medium | `src/checker/type_checking.rs` |
| **P0** | TS2304 | 7.2% (48/663) | 615 tests | High | `src/binder/state.rs`, `src/checker/symbol_resolver.rs` |
| **P1** | TS1202 | 9.2% (25/271) | 246 tests | Low | `src/checker/import_checker.rs` |
| **P1** | TS2339 | 13.0% (53/406) | 353 tests | Medium | `src/checker/property_access.rs` |

---

## Critical Issues (P0) - Immediate Action Required

### Issue #1: TS2428 - Interface Type Parameter Consistency (0% - COMPLETELY MISSING)

**Description:** Interface declarations with mismatched type parameters do not emit TS2428.

**Validated Test Cases:**
```typescript
// genericAndNonGenericInterfaceWithTheSameName.ts
interface A { foo: string; }
interface A<T> { bar: T; }  // Should error: TS2428 - different type params

// twoGenericInterfacesDifferingByTypeParameterName.ts
interface A<T> { x: T; }
interface A<U> { y: U; }  // Should error: TS2428 - T != U

interface B<T,U> { x: U; }
interface B<T,V> { y: V; }  // Should error: TS2428 - second param differs

// twoGenericInterfacesDifferingByTypeParameterName2.ts
interface B<T, U> { x: U; }
interface B<U, T> { y: V; }  // Should error: TS2428 - params reordered AND V undefined
```

**Key Validation Rules Needed:**
1. Same number of type parameters
2. Same type parameter **names** in the same order (T must be T, not U)
3. Same constraints (or lack thereof)
4. Generic + non-generic interfaces cannot merge

**Root Cause:**
- **No validation exists** for type parameter consistency across merged interface declarations
- The binder allows merging (`can_merge_symbols` in `src/binder/state.rs:1007`) but doesn't validate parameter consistency

**Files to Modify:**
1. `src/binder/state.rs` - Add validation in `declare_symbol` or after interface declaration
2. `src/checker/types/diagnostics.rs` - Add TS2428 error code and message

**Implementation Plan:**
```rust
// In src/binder/state.rs, after checking can_merge_symbols:
if can_merge_symbols(existing_flags, new_flags) {
    // NEW: Validate type parameter consistency
    if (existing_flags & symbol_flags::INTERFACE) != 0 {
        if let Err(e) = validate_interface_type_params(existing_decl, new_decl) {
            self.error(e); // Emit TS2428
        }
    }
}

// New function to add:
fn validate_interface_type_params(
    existing_decl: &InterfaceDecl,
    new_decl: &InterfaceDecl
) -> Result<(), ValidationError> {
    // Check:
    // 1. Same number of type parameters
    // 2. Same type parameter names (in order)
    // 3. Same constraints (or lack thereof)
}
```

**Expected Impact:** +11 tests (0% → 100%)

---

### Issue #2: TS2403 - Subsequent Variable Declarations (0.0% - 0/123 tests passing)

**Description:** Variable redeclarations with different types do not emit TS2403.

**Validated Test Cases:**
```typescript
// ES5SymbolProperty3.ts
//@target: ES5
var Symbol: any;  // Should error: TS2403 - conflicts with global SymbolConstructor
class C { [Symbol.iterator]() { } }

// ES5SymbolProperty4.ts
var Symbol: { iterator: string };  // Should error: TS2403

// ES5SymbolProperty5.ts
declare var Symbol: { iterator: symbol };  // Should error: TS2403

// asyncArrowFunction9_es2017.ts
var Promise: any;  // Should error: TS2403 - conflicts with global PromiseConstructor
```

**Pattern:** ALL 76 TS2403 failures are about **global lib symbols** (Symbol, Promise, etc.) being redeclared with different types in ES5/es2017 mode.

**Root Cause:**
- Infrastructure exists: `var_decl_types` cache in `src/checker/context.rs:260`
- **Actual checking logic is missing** - types are not being compared on redeclaration
- Global symbols from lib files (SymbolConstructor, PromiseConstructor) are loaded but not checked against local declarations

**Files to Modify:**
1. `src/checker/type_checking.rs` - Add TS2403 check in variable declaration handling
2. `src/binder/state.rs` - May need to track redeclaration information

**⚠️ CRITICAL CORRECTION (from AI peer review):**

The original plan to use `var_decl_types` cache is **technically flawed**:
- `var_decl_types` is transient - populated as checker walks the current file
- It **will not** contain types for `Promise`, `Symbol` from `lib.d.ts` when checking starts
- Preloading lib types into the cache could trigger **cycles** (Array ↔ Iterator)

**CORRECTED Implementation Plan:**
```rust
// In src/checker/type_checking.rs, check_variable_declaration:
pub(crate) fn check_variable_declaration(&mut self, var_decl: NodeIndex) {
    let var_name = self.get_variable_name(var_decl);
    let sym_id = self.resolve_symbol(var_name)?;
    let new_type = self.check_variable_annotation(var_decl);

    // CRITICAL: Don't just check var_decl_types cache!
    // Must check if symbol has OTHER declarations (redeclaration/merge)
    let other_decls: Vec<_> = self.ctx.symbols[sym_id]
        .declarations
        .iter()
        .filter(|&&d| d != var_decl)  // Exclude current declaration
        .collect();

    if !other_decls.is_empty() {
        // Resolve type of previous declaration(s) immediately
        for &prev_decl in &other_decls {
            let prev_type = self.get_type_of_declaration(prev_decl)?;

            // Check assignability
            if !self.is_type_assignable(prev_type, new_type) {
                self.error_at_node(
                    var_decl,
                    TS2403,
                    &format!(
                        "Subsequent variable declarations must have the same type.  \
                         Variable '{}' must be of type '{}', but here has type '{}'.",
                        var_name, self.display_type(prev_type), self.display_type(new_type)
                    ),
                );
                return; // Emit once, don't spam
            }
        }
    }

    // Only cache if no conflicts
    self.ctx.var_decl_types.insert(sym_id, new_type);
}
```

**Key Changes from Original Plan:**
1. ✅ Check `symbol.declarations` directly instead of relying on cache
2. ✅ Resolve previous declaration types **eagerly** when needed
3. ✅ Handle cross-file declarations (user code + lib.d.ts)
4. ✅ Avoid preloading which could trigger cycles

**Risk Mitigation:**
- Watch for recursion in `get_type_of_declaration` - may need cycle guards
- Test with cases that have 3+ redeclarations
- Ensure cross-arena resolution works (user file + lib.d.ts)

**Expected Impact:** +60-76 tests (0% → significant, but slightly lower than hoped)

**Key Scenarios to Handle:**
1. Global `Symbol` type shadowing (ES5 + Symbol computed properties)
2. Ambient vs non-ambient redeclarations
3. Interface/type-only redeclarations

---

### Issue #3: TS2304 - Cannot Find Name (7.2% - 48/663 tests passing)

**⚠️ COMPLEXITY ALERT (from AI peer review):**

This is **NOT just a symbol resolution bug**. At 50% conformance, TS2304 is often a symptom of:
1. **Type inference failures** - `infer.rs` returns `Unknown`/`Error`, breaking downstream checks
2. **Control Flow Analysis bugs** - incorrect scope narrowing makes variables "disappear"
3. **Type vs. Value confusion** - finding interface (type) when class constructor (value) needed
4. **Actual scope lookup failures** - the "simple" case

**Root Causes (multi-layered):**
1. **Scope chain issues:** Symbols not properly propagated through scope hierarchy
2. **Namespace merging:** Namespace declarations not merging symbol tables correctly
3. **Ambient context:** Ambient declarations not globally visible when they should be
4. **Import resolution:** Imported symbols not available in correct scopes
5. **Inference cascades:** Failed type inference creates `Error` types that trigger TS2304
6. **CFA bugs:** Control flow analysis incorrectly prunes reachable code

**Investigation Strategy:**
```bash
# Categorize failures by pattern:
./scripts/conformance/run.sh --error-code=2304 --filter="namespace" --max=10 --print-test
./scripts/conformance/run.sh --error-code=2304 --filter="infer" --max=10 --print-test
./scripts/conformance/run.sh --error-code=2304 --filter="ambient" --max=10 --print-test

# For each failure, check:
# 1. Does binder find the symbol? (check symbol table)
# 2. Does type inference succeed? (check inferred type isn't Error/Unknown)
# 3. Is CFA removing the symbol incorrectly? (check control flow)
```

**Files to Modify:**
1. `src/binder/state.rs` - Scope chain and symbol lookup
2. `src/checker/symbol_resolver.rs` - Symbol resolution logic
3. `src/checker/namespace_checker.rs` - Namespace merging

**Investigation Steps:**
```bash
# Find failing patterns:
./scripts/conformance/run.sh --error-code=2304 --filter="namespace" --max=10 --print-test
./scripts/conformance/run.sh --error-code=2304 --filter="import" --max=10 --print-test
./scripts/conformance/run.sh --error-code=2304 --filter="generic" --max=10 --print-test
```

**Implementation Plan:**

**Phase 1: Fix Scope Chain**
```rust
// In src/binder/state.rs, resolve_symbol:
pub(crate) fn resolve_symbol(&self, name: &str) -> Option<SymbolId> {
    let mut current_scope = self.current_scope;

    while current_scope != ScopeId::ROOT {
        // Check current scope
        if let Some(&sym_id) = self.scopes[current_scope].symbols.get(name) {
            return Some(sym_id);
        }
        // Check parent scope
        current_scope = self.scopes[current_scope].parent?;
    }

    // Check global scope
    self.global_symbols.get(name).copied()
}
```

**Phase 2: Fix Namespace Merging**
```rust
// Ensure namespace blocks merge into parent namespace's symbol table
// In src/binder/state.rs, when entering namespace block:
pub(crate) fn enter_namespace(&mut self, namespace_sym: SymbolId) {
    // Create or get namespace scope
    let namespace_scope = self.get_or_create_namespace_scope(namespace_sym);

    // Merge parent namespace's symbols into this scope
    if let Some(parent_ns) = self.get_parent_namespace(namespace_sym) {
        self.merge_namespace_symbols(parent_ns, namespace_scope);
    }

    self.current_scope = namespace_scope;
}
```

**Expected Impact:** +200-400 tests (7.2% → 30-40%)

---

## High-Impact Issues (P1) - Next Sprint

### Issue #4: TS1202 - Import Assignment in ESM (9.2% - 25/271 tests passing)

**Description:** `import = require()` in ES modules doesn't emit TS1202.

**Validated Test Cases:**
```typescript
// ambientDeclarationsExternal.ts
/// <reference path="decls.ts" />
import imp1 = require('equ');  // Should error: TS1202
import imp3 = require('equ2');  // Should error: TS1202

// ambientShorthand.ts
///<reference path="declarations.d.ts"/>
import boom = require("jquery");  // Should error: TS1202

// circularReference.ts
import foo2 = require('./foo2');  // Should error: TS1202
import foo1 = require('./foo1');  // Should error: TS1202
```

**All 123 TS1202 tests use `import X = require(...)` syntax in ES module mode.**

**Root Cause:**
- Check exists in `src/checker/import_checker.rs:234`
- **Condition prevents execution:** `report_unresolved_imports` is false in conformance mode
- TSC always emits TS1202 for `import = require()` in ESM regardless of settings

**Files to Modify:**
1. `src/checker/import_checker.rs` - Fix condition at line 234

**Implementation Plan:**
```rust
// Current (line 231-235):
if self.ctx.report_unresolved_imports && self.ctx.binder.is_external_module() {
    self.error_at_node(stmt_idx, TS1202, ...);
}

// Fix to: Always check in ESM mode, regardless of report_unresolved_imports
if self.ctx.binder.is_external_module() {
    // Check if this is `import x = require(...)`
    if is_import_equals_require(stmt_idx) {
        self.error_at_node(stmt_idx, TS1202, ...);
    }
}
```

**Expected Impact:** +150-200 tests (9.2% → 60-70%)

---

### Issue #5: TS2339 - Property Does Not Exist (13.0% - 53/406 tests passing)

**Description:** Property access validation has multiple issues including wrong types being computed.

**Validated Test Cases Reveal Deeper Issues:**

```typescript
// ambientDeclarationsPatterns_merging3.ts - Wrong error code!
declare module "a.foo" { export interface OhNo { a: string } }
import { OhNo } from "b.foo"
ohno.a  // TSC: TS2339, tsz: TS2664 (wrong code!)

// constructorParameterProperties.ts - Wrong type!
class D<T> { constructor(a: T, private x: T, protected z: T) { } }
declare var d: D<string>;
d.a  // TSC: TS2339 on D<string>
     // tsz: TS2339 on huge object type with wrong members!

// protectedClassPropertyAccessibleWithinNestedSubclass.ts - Wrong type!
var c5 = c.z;  // TSC: TS2339 on C
              // tsz: TS2339 on huge object type instead of C
```

**Root Causes (Discovered from Validation):**
1. **Wrong error codes:** tsz emits TS2664 instead of TS2339 in some cases
2. **Wrong types computed:** Instead of computing `D<string>`, tsz computes a huge object type
3. This suggests issues **upstream** in type computation, not just property access checking

**Files to Modify:**
1. `src/checker/property_access.rs` - Property access checking
2. `src/solver/property_access.rs` - Property type resolution
3. **Also investigate:** Type computation for classes and generics

**Implementation Plan:**
```rust
// In src/checker/property_access.rs:
pub(crate) fn check_property_access(&mut self, expr: NodeIndex) {
    let object_type = self.get_type_of_object(expr);
    let property_name = self.get_property_name(expr);

    // Check if property exists on type
    if !self.has_property(object_type, property_name) {
        self.error(Error {
            code: 2339,
            message: format!("Property '{}' does not exist on type '{}'.",
                           property_name, object_type),
        });
    }
}

// Handle union types:
fn has_property(&self, type_id: TypeId, prop: &str) -> bool {
    if let Type::Union(members) = self.get_type(type_id) {
        // Property must exist on ALL union members
        members.iter().all(|m| self.has_property(*m, prop))
    } else {
        self.get_properties(type_id).contains(prop)
    }
}
```

**Expected Impact:** +100-150 tests (13.0% → 40-50%)

---

## Critical Missing Issue (P0) - Control Flow Analysis

### Issue: Control Flow Analysis Bugs (Not Previously Tracked)

**Why This Was Missed:**
- Not visible in error code summaries
- Manifests as cascading TS2304/TS2339 errors
- **Root cause of many "symbol not found" errors that are actually CFA false positives**

**The Problem:**
`src/checker/control_flow_narrowing.rs` implements TypeScript's complex control flow rules. If buggy:
- Variables in `if` blocks incorrectly marked "unreachable"
- `const` declarations in conditionals don't narrow correctly
- Closures capture wrong scope (Rule #42 implementation issues)

**Impact Assessment:**
```bash
# Find CFA-related failures:
./scripts/conformance/run.sh --filter="const.*if|let.*if" --max=20 --print-test
./scripts/conformance/run.sh --filter="narrow" --max=20 --print-test
```

**Investigation Areas:**
1. **Rule #42 (CFA invalidation in closures):** Check `src/checker/control_flow_narrowing.rs:1680+`
2. **Const narrowing in conditionals:** Does `if (const x = getValue())` work?
3. **Unreachable code detection:** Are branches incorrectly pruned?

**Files to Examine:**
- `src/checker/control_flow_narrowing.rs` - Main CFA logic
- `src/checker/flow_graph_builder.rs` - CFG construction
- `src/checker/flow_analysis.rs` - Dataflow analysis

**Implementation Priority:** Should be investigated **before** deep TS2304 fixes, as many TS2304 errors may be CFA false positives.

---

## Medium-Impact Issues (P2)

### Issue #6: TS2322 - Type Not Assignable (36.9% - 276/747 tests passing)

**Description:** Assignability checks incomplete.

**Files:**
- `src/solver/compat.rs` - Type assignability logic

**Investigation:** Generic type assignability, union/intersection handling

---

### Issue #7: TS2345 - Argument Not Assignable (31.8% - 85/267 tests passing)

**Description:** Function call argument checking incomplete.

**Files:**
- `src/checker/callable_type.rs` - Function type checking

---

### Issue #8: TS1005 - Expected Token (14.5% - 11/64 tests passing)

**Description:** Parser error recovery not matching TSC behavior.

**Files:**
- `src/parser/state.rs` - Parser state machine
- `src/scanner.rs` - Tokenization

**Note:** Parser issues are often cascading - fixing may improve multiple error codes.

---

## Additional Findings

### Pass Rate by Error Code (Top 20)

| Code | Description | Pass Rate | Passed | Failed | Priority |
|------|-------------|-----------|--------|--------|----------|
| TS2403 | Subsequent variable declarations | **0.0%** | 0 | 123 | P0 |
| TS2428 | Interface type params | **0%** | 0 | 11 | P0 |
| TS1202 | Import assignment ESM | **9.2%** | 25 | 246 | P1 |
| TS2304 | Cannot find name | **7.2%** | 48 | 615 | P0 |
| TS2339 | Property not exist | **13.0%** | 53 | 353 | P1 |
| TS1005 | Expected token | **14.5%** | 11 | 64 | P2 |
| TS2345 | Argument not assignable | **31.8%** | 85 | 182 | P2 |
| TS2322 | Type not assignable | **36.9%** | 276 | 471 | P2 |
| TS2365 | Operator not applicable | **42.1%** | 61 | 84 | P2 |
| TS2351 | Cannot use 'new' | **45.5%** | 10 | 12 | P3 |

---

## Key Findings from Test Validation

### TS2403 - All Failures Are Global Symbol Redeclarations

**CRITICAL INSIGHT:** Every single TS2403 failure (76 tests) involves redeclaring **global lib symbols**:
- `Symbol` vs `SymbolConstructor` (ES5 mode with computed properties)
- `Promise` vs `PromiseConstructor` (ES2017 mode with async)

**Implementation Implication:**
1. The check must run **after** lib symbols are loaded
2. Must detect when user-declared `var Symbol` conflicts with `SymbolConstructor` from lib
3. Must track both ambient (`declare var`) and non-ambient redeclarations

### TS2428 - Type Parameter NAME Matters

**CRITICAL INSIGHT:** TSC checks that type parameter **names** match exactly, not just structure:
```typescript
interface B<T, U> { }
interface B<U, T> { }  // ERROR: Names don't match (even though structurally same)

interface B<T, V> { }  // ERROR: Second parameter name differs
```

**Implementation Implication:**
1. Store type parameter **names** in symbol metadata
2. Compare names **in order** during declaration merging
3. Error on ALL declarations that don't match the first one

### TS1202 - Always Check in ESM Mode

**CRITICAL INSIGHT:** TSC emits TS1202 for `import = require()` **unconditionally** in ESM mode:
```typescript
import foo = require('bar');  // Always TS1202 in ESM mode
```

**Implementation Implication:**
1. Remove `report_unresolved_imports` condition
2. Check only: `is_external_module() && is_import_equals_require()`

### TS2339 - Deeper Type Computation Issues

**CRITICAL INSIGHT:** Many TS2339 failures show tsz computes **wrong types**:
- Expected: `D<string>` (generic class type)
- Got: Huge object literal with all properties

**Implementation Implication:**
1. This is **not just** a property access issue
2. Root cause is in **type computation** for classes and generics
3. Fixing property access alone won't solve this

---

## Implementation Roadmap

### Sprint 1: Quick Wins + Investigation (1-2 weeks)

1. **TS1202** - Fix import assignment check condition (**4 hours**)
   - Remove `report_unresolved_imports` condition in import_checker.rs:234
   - Tests affected: 123 tests (all `import = require()` in ESM)
   - Expected: +100-120 tests
   - Risk: Low - TSC does this unconditionally

2. **TS2428** - Add interface type parameter validation (3-5 days)
   - Add validation in `src/checker/state_checking_members.rs`
   - Don't store params on symbol - iterate declarations directly
   - Handle cross-file declarations (user + lib.d.ts)
   - Tests affected: 11 tests (all interface type param mismatches)
   - Expected: +11 tests (100% → complete fix)
   - Risk: Moderate - need to ensure cross-arena node access works

3. **Investigation** - Deep dive into TS2304 + CFA (2-3 days)
   - Categorize 615 TS2304 failures by pattern
   - Test Control Flow Analysis with const/let narrowing cases
   - Identify how many are symbol resolution vs type inference vs CFA bugs
   - Expected: Refined Sprint 2 strategy

**Expected Impact:** +120-140 tests (49.7% → 51-52%)
**Realistic**: +1-2% (TS1202 is very specific)

### Sprint 2: Core Fixes with Realistic Expectations (2-3 weeks)

1. **TS2403** - Variable redeclaration (CORRECTED approach) (**5-7 days**)
   - ✅ Check `symbol.declarations` directly (don't rely on var_decl_types cache)
   - ✅ Eagerly resolve previous declaration types when checking redeclaration
   - ✅ Handle cross-file declarations via `all_arenas` in CheckerContext
   - ⚠️ Add cycle guards to prevent recursion (Array ↔ Iterator)
   - Tests affected: 76 tests (all global symbol redeclarations)
   - Expected: +50-70 tests (not all - some may have other issues)
   - Risk: Moderate - cycle detection, cross-arena resolution

2. **TS2304 Phase 1** - Fix actual symbol resolution (5-7 days)
   - Based on Sprint 1 investigation (fix only true scope/namespace bugs)
   - Fix `resolve_symbol` in binder/state.rs scope traversal
   - Fix namespace symbol table merging
   - Expected: +50-100 tests (only true resolution bugs, not inference issues)
   - Risk: High - may unmask other errors

3. **CFA Investigation** - Control Flow Analysis (3-5 days)
   - Test Rule #42 implementation in control_flow_narrowing.rs
   - Check const narrowing in conditionals
   - Verify unreachable code detection isn't over-aggressive
   - Expected: +20-50 tests if bugs found, 0 if CFA is solid
   - Risk: Low - investigation only

**Expected Impact:** +120-220 tests (51-52% → 57-60%)
**Realistic**: +3-5% (many TS2304 are inference issues, not resolution)

### Sprint 3: Type System Deep Dive (3-4 weeks)

1. **Generic Inference** - Fix `src/solver/infer.rs` (7-10 days)
   - **CRITICAL:** This is the blocker for 60%+ pass rate
   - Investigate why inference returns `Unknown`/`Error`
   - Fix generic type argument inference
   - Fix conditional type inference
   - Expected: +100-200 tests (many TS2304 are actually inference failures)

2. **TS2339** - Property access validation (5-7 days)
   - Fix wrong type computation (getting object literal instead of class type)
   - Fix property existence checks
   - Expected: +50-100 tests

3. **TS2322/TS2345** - Assignability and function calls (5-7 days)
   - Fix assignability checks for generic types
   - Fix function argument type checking
   - Expected: +50-100 tests

4. **Parser recovery** - TS1005 improvements (3-5 days)
   - Improve error recovery in parser
   - Expected: +20-50 tests

**Expected Impact:** +220-450 tests (57-60% → 67-70%)
**Realistic:** Will hit diminishing returns without major architectural work

---

## Testing Strategy

### Before Implementing Fixes

```bash
# Establish baseline for specific error code
./scripts/conformance/run.sh --error-code=TSXXXX --pass-rate-only

# Examine specific failures
./scripts/conformance/run.sh --error-code=TSXXXX --max=5 --print-test

# Deep trace for single test
./scripts/conformance/run.sh --error-code=TSXXXX --filter="pattern" --max=1 --trace
```

### After Implementing Fixes

```bash
# Verify improvement
./scripts/conformance/run.sh --error-code=TSXXXX --pass-rate-only

# Check for regressions
./scripts/conformance/run.sh --pass-rate-only

# Full comparison
./scripts/conformance/run.sh --dump-results=before.json
# Make changes
./scripts/conformance/run.sh --dump-results=after.json
# diff before.json after.json
```

---

## Debugging Tools

### Use ask-gemini for Targeted Investigation

```bash
# Investigate specific error codes with full context
./scripts/ask-gemini.mjs --checker --include="src/checker src/binder" \
  "Why does TS2403 fail? Analyze variable redeclaration checking."

# Investigate symbol resolution
./scripts/ask-gemini.mjs --binder --include="src/binder" \
  "How does scope chain work for namespace resolution?"

# Investigate type operations
./scripts/ask-gemini.mjs --solver --include="src/solver" \
  "How does assignability check work for union types?"
```

### Use Conformance Runner for Deep Analysis

```bash
# See detailed failure information
./scripts/conformance/run.sh --error-code=TSXXXX --print-test

# Trace single test with debug output
./scripts/conformance/run.sh --filter="testName" --max=1 --trace=trace

# Get JSON results for analysis
./scripts/conformance/run.sh --dump-results=results.json
```

---

## Long-Term Conformance Ceiling Analysis

### Realistic Pass Rate Expectations

Based on the codebase analysis and AI peer review:

| Milestone | Pass Rate | What's Required | Difficulty |
|-----------|-----------|-----------------|------------|
| **Current** | 49.7% | Baseline | - |
| **Sprint 1** | 51-52% | TS1202 + TS2428 | Low |
| **Sprint 2** | 57-60% | TS2403 + TS2304 (resolution only) | Medium |
| **Sprint 3** | 67-70% | Generic inference + property access | High |
| **~70% Ceiling** | 70-75% | All above | Very High |
| **80%+** | 80%+ | Major architectural work | Extreme |

### The 60% Ceiling: Why We'll Hit It

Around 60% pass rate, we'll hit a wall caused by:

1. **Generic Inference (`infer.rs`):**
   - TypeScript's type inference is extremely complex
   - Conditional types, mapped types, generic parameter defaults
   - Requires coinductive algorithms (mutual recursion)

2. **Overload Resolution (`call_checker.rs`):**
   - Function overloads with generic constraints
   - Union type distribution in call signatures
   - Contextual typing bidirectional checking

3. **Module System Complexity:**
   - Cross-file declaration merging
   - Augmentation (merging user code with lib.d.ts)
   - Circular import handling

### Breaking Through 70%: Major Work Required

To exceed 70% pass rate, need:

1. **Complete Generic Type System:**
   - Full conditional type evaluation
   - Mapped and template literal types
   - Generic constraint propagation

2. **Bidirectional Type Checking:**
   - Contextual typing from LHS to RHS
   - Flows through object literals, arrays, functions
   - Complex inference constraints

3. **Incremental Type Checking:**
   - Cache invalidation on edits
   - Dependency tracking between declarations
   - Performance optimizations (currently 12 seconds for 12k tests)

**Estimate:** 3-6 months of focused development to reach 80%

---

## Generic Type System Deep Dive

**Related Document:** See `GENERICS_IMPLEMENTATION_PLAN.md` for comprehensive generics strategy

### Conformance Validation (2025-02-02)

After validating the generics plan against actual conformance tests, we've identified **critical bugs** blocking generic type system progress:

#### Critical Finding #1: Structural Erasure Bug (CONFIRMED)

**Test:** `conformance/classes/constructorDeclarations/constructorParameters/constructorParameterProperties.ts:19`

```typescript
class D<T> {
    constructor(a: T, private x: T, protected z: T) { }
}
declare var d: D<string>;
var r2 = d.a; // error - property 'a' does not exist
```

**TSC Error:**
```
Property 'a' does not exist on type 'D<string>'.
```

**tsz Error:**
```
Property 'a' does not exist on type '{ z: string; isPrototypeOf: { ... }; ... }'.
```

**Root Cause:** `src/solver/instantiate.rs:230-240` eagerly lowers `TypeKey::Application` to `TypeKey::Object`, erasing nominal identity.

**Impact:**
- Confusing error messages (shows object literal instead of `D<string>`)
- **Breaks private/protected member checking** (nominal information lost)
- Affects ~200-300 tests across TS2339, TS2341, TS2445

#### Critical Finding #2: Private Members in Generic Classes (BROKEN)

**Tests:** `privateNamesInGenericClasses.ts`, `privateNamesAndGenericClasses-2.ts`

**TSC Errors:**
```
TS2322: Type 'C<string>' is not assignable to type 'C<number>'.
TS18013: Property '#foo' is not accessible (3x)
```

**tsz Errors:**
```
TS2339: Property '#foo' does not exist (9x) - WRONG CODE!
```

**Analysis:** Downstream effect of structural erasure - when `C<string>` becomes object literal, private member checks fail completely.

**Impact:** ~50+ tests for generic private/protected access

#### Critical Finding #3: Array Type Instantiation (BROKEN)

**Test:** `parserObjectCreation1.ts`

```typescript
var autoToken: number[] = new Array<number[]>(1);
```

**TSC Error:**
```
TS2322: Type 'number[][]' is not assignable to type 'number[]'.
```

**tsz:** `(no errors)`

**Impact:** Generic constructor instantiation is broken

#### Critical Finding #4: Generic Call Inference (MAJOR ISSUES)

**Test:** `typeArgumentInferenceWithConstraints.ts:33`

```typescript
function someGenerics3<T extends Window>(producer: () => T) { }
someGenerics3(() => ''); // Error
```

**TSC Error:**
```
TS2322: Type 'string' is not assignable to type 'Window'.
```

**tsz Errors:**
```
TS2318: Cannot find global type 'Window'
TS2345: Argument of type '() => string' is not assignable to parameter of type '() => error'.
```

**Analysis:** Inference failing, producing `error` type instead of inferring `T = string`

**Impact:** ~150-250 tests for TS2345 + TS2322 generic failures

### Updated Sprint Plan

Based on validation, the **Sprint A (Fix Structural Erasure)** is BLOCKING all other generics work:

1. **Sprint A** (1-2 weeks): Fix `src/solver/instantiate.rs` to preserve `TypeKey::Application`
   - Expected: +200-300 tests (TS2339, private members)
   - **Prerequisite for:** All nominal type checking

2. **Sprint B** (2-3 weeks): Fix generic call inference
   - Expected: +150-250 tests (TS2345, TS2322)
   - Depends on: Sprint A

3. **Sprint C** (2-3 weeks): Conditional types
   - Expected: +100-150 tests

**Total Expected Impact:** +450-700 tests, reaching 65-74% pass rate

### Test Statistics

| Category | Tests | Status |
|----------|-------|--------|
| `generic` + TS2322 | 50 | Constraint validation + assignability |
| `generic` + TS2345 | 32 | Call inference failures |
| TS2339 | 407 | Property access + structural erasure |
| TS2322 (total) | 471 fail | Many are generic-related |
| TS2345 (total) | 182 fail | Mostly generic call inference |

**Total generic-related failures:** ~1,268 tests (20% of all failures)

---

## Appendix: File Reference

### Key Files by Issue

| Issue | Files |
|-------|-------|
| TS2428 | `src/binder/state.rs`, `src/checker/interface_type.rs` |
| TS2403 | `src/checker/type_checking.rs`, `src/checker/context.rs` |
| TS2304 | `src/binder/state.rs`, `src/checker/symbol_resolver.rs` |
| TS1202 | `src/checker/import_checker.rs` |
| TS2339 | `src/checker/property_access.rs` |
| TS2322 | `src/solver/compat.rs` |
| TS2345 | `src/checker/callable_type.rs` |
| TS1005 | `src/parser/state.rs`, `src/scanner.rs` |

### Diagnostic Codes

All error codes defined in: `src/checker/types/diagnostics.rs`

```rust
pub const TS2403: u32 = 2403;  // Subsequent variable declarations
pub const TS2428: u32 = 2428;  // Interface type parameters
pub const TS1202: u32 = 1202;  // Import assignment
pub const TS2304: u32 = 2304;  // Cannot find name
pub const TS2339: u32 = 2339;  // Property not exist
pub const TS2322: u32 = 2322;  // Type not assignable
pub const TS2345: u32 = 2345;  // Argument not assignable
pub const TS1005: u32 = 1005;  // Expected token
```

---

## Next Steps

1. **Review this document** with the team - especially the AI peer review findings
2. **Start with TS1202** - 4 hour fix for ~100 tests (quickest win!)
3. **Then TS2428** - 3-5 days for complete fix (11 tests, high confidence)
4. **Investigate CFA** - Before deep TS2304 work, check if Control Flow Analysis is the real culprit
5. **Set up conformance CI** - Track pass rate over time, catch regressions
6. **Create tracking issue** for each P0/P1 issue with implementation details

**Quick Start Path (First 2 Weeks):**
- Day 1: Fix TS1202 (+100 tests, 4 hours) - **confirmed safe**
- Days 2-6: Fix TS2428 (+11 tests, complete) - **moderate risk**
- Days 7-10: CFA investigation + TS2304 categorization

**Realistic Targets:**
- Sprint 1: 51-52% (+1-2% from TS1202)
- Sprint 2: 57-60% (+5-8% from TS2403 + partial TS2304)
- Sprint 3: 67-70% (+10% from generic inference)
- **Ceiling:** ~70-75% without major generic type system work

**Red Flags to Watch:**
- ⚠️ Pass rate might **drop** after TS2304 fixes (unmasks hidden type errors)
- ⚠️ TS2403 fix could trigger **cycles** (Array ↔ Iterator)
- ⚠️ Many TS2304 are **inference failures**, not just scope bugs

**Quick Win Validation Commands:**
```bash
# Before fix
./scripts/conformance/run.sh --error-code=1202 --pass-rate-only

# After fix (should see 9.2% → ~90%)
./scripts/conformance/run.sh --error-code=1202 --pass-rate-only

# Track overall progress
./scripts/conformance/run.sh --pass-rate-only

# Find CFA-related failures
./scripts/conformance/run.sh --filter="const.*if|let.*if" --max=20 --print-test
```

---

## AI Peer Review Summary

**Key Insights from Gemini Analysis:**

✅ **Validated Approaches:**
- TS1202 fix is sound (low risk)
- TS2428 approach is correct but needs cross-arena handling

⚠️ **Corrected Approaches:**
- TS2403: Don't use `var_decl_types` cache - check `symbol.declarations` directly
- Must handle cross-file declarations (user code + lib.d.ts)

🚨 **Missing Priorities:**
- **Control Flow Analysis** - may be causing many false positive TS2304/TS2339 errors
- **Generic Inference** - the real blocker for 60%+ pass rate
- **Type vs Value** confusion in symbol resolution

📊 **Realistic Expectations:**
- Sprint 1: +1-2% (not +5% as initially hoped)
- Sprint 2: +3-5% (not +15%)
- Ceiling: ~70% without major generic type system work
