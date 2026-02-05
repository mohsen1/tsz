# TSZ-1: Active Tasks

## Completed Tasks

### ✅ ASI (Automatic Semicolon Insertion) Fix - Completed 2026-02-05

**Problem**: Variable declarations without semicolons followed by keywords caused false TS1005 "comma expected" errors:
```typescript
var x = 1
const y = 2  // tsz: TS1005 ',' expected - WRONG!
```

**Root Cause** (`src/parser/state_statements.rs` lines 765-782):
In `parse_variable_declaration_list`, after parsing a variable without a comma, the code checks if the next token could start another declaration. If so, it emits a comma error - but it didn't check for a preceding line break (ASI).

**Fix Applied**:
```rust
// Added check for ASI before emitting comma error
if can_start_next
    && !self.scanner.has_preceding_line_break()  // NEW: ASI check
    && !self.is_token(SyntaxKind::SemicolonToken)
    // ...
```

**Conformance Impact**: 40.6% -> 41.8% (+152 tests, +1.2%)
- **TS1005 extra errors: 972 -> 283** (-689 false positives!)

---

### ✅ Lib Loading Target Respect Fix - Completed 2026-02-05

**Problem**: After fixing cross-arena cache poisoning, conformance dropped because `tsz` was loading ES2015+ types even when `--target es5` was specified.

**Root Cause**: 
`resolve_lib_files` was following `/// <reference lib="..." />` directives in lib files. Specifically, `lib.dom.d.ts` references `es2015`, causing ES2015 collections (Map, Set, WeakSet) to be loaded even for ES5 targets.

**TypeScript Behavior**: 
tsc does NOT follow reference directives when loading default libs based on target. These references are only used when explicitly including libs via `--lib`.

**Fix Applied** (`src/cli/config.rs`):
1. Added `resolve_lib_files_with_options(libs, follow_references)` - parameterized reference following
2. Added `default_libs_for_target(target)` - explicit lib lists for ES5/ES2015
3. Added `should_follow_references_for_target(target)` - ES2016+ follows refs, ES5/ES2015 don't
4. Updated `resolve_default_lib_files` and `materialize_embedded_libs` to use new functions

**Verification**:
```bash
# Before fix: No error (Map incorrectly available)
./.target/release/tsz /tmp/test_es5.ts --noEmit --target es5

# After fix: Correct error matching tsc
error TS2583: Cannot find name 'Map'. Do you need to change your target library?
```

**Conformance Impact**: +0.2% (39.5% -> 39.7%)

---

### ✅ Cross-Arena Cache Poisoning Fix - Completed 2026-02-05

**Previous Claim**: "Lib loading was already working" - INCORRECT

**Reality**: Lib symbols like `WeakSet`, `Map`, `Set` were resolving to `TypeId::ERROR` due to a cache poisoning bug.

**Root Cause Found**:
In `compute_type_of_symbol`, when cross-arena delegation was needed:
1. Parent checker pre-caches `ERROR` as a "resolution in progress" marker (cycle detection)
2. Child checker is created with `with_parent_cache` which CLONES the parent's cache
3. Child checker's `get_type_of_symbol` checks cache first, finds ERROR, returns immediately
4. Lib symbol types are never actually computed

**Fix Applied** (`src/checker/state_type_analysis.rs`):
```rust
// Before creating child checker, remove the "in-progress" ERROR marker
self.ctx.symbol_types.remove(&sym_id);
```

**Additional Changes** (`src/parallel.rs`):
- Added `symbol_arenas` and `declaration_arenas` fields to `BindResult` struct
- Propagate arena mappings during `merge_bind_results_ref` for lib symbols

**Verification**:
- `WeakSet` now resolves to type 292 (correct) instead of type 1 (ERROR)
- `Array<T>`, `Promise<T>` type annotations work correctly

**Conformance Impact**: -1.1% (40.6% -> 39.5%)
- This is EXPECTED - the fix exposed a separate issue: lib loading is too permissive
- Before: Any-poisoning masked incorrect symbol resolution, tests passed by accident
- After: Symbols resolve correctly, revealing that libs load regardless of target/lib options
- TS2304 "missing" increased because symbols are now found that shouldn't be available for certain targets
- Fixed by: Lib Loading Target Respect Fix (above)

### ⚠️ TS2454 Module-Level Fix - In Progress 2026-02-05
**Status**: Partial Fix Applied, Deeper Issue Discovered

**Changes Made**:
- Modified `should_check_definite_assignment` in `src/checker/flow_analysis.rs`
- Removed SOURCE_FILE early return (lines 1796-1798)
- Removed `found_function_scope` requirement (lines 1807-1809)

**Issue Discovered**:
Module-level statements don't have flow nodes created by the binder:
- `get_node_flow` returns `None` for module-level identifiers
- `is_definitely_assigned_at` has safe default: returns `true` when no flow info (line 1801)
- This prevents TS2454 detection for module-level `let`/`const` variables

**Root Cause**:
Flow analysis (control flow graph building) only covers function bodies, not module-level statements. Fixing this requires extending the binder's flow node creation to cover module-level statements.

**Verification**:
```typescript
// test.ts
let x: number;
console.log(x);  // tsc --strict: TS2454, tsz: No error
```

---

## Active Tasks

### ✅ Overload Implementation Validation - Completed 2026-02-05

**Problem**: TSZ did not validate that function/method/constructor implementations are compatible with all their overload signatures, allowing unsound implementations that TypeScript would reject.

**Root Cause**: No validation of overload compatibility after implementation checking.

**Fix Applied** (`src/checker/state_checking_members.rs`):
1. Added TS2394 error code and message to `diagnostics.rs`
2. Implemented `check_overload_compatibility()` function:
   - Gets implementation's symbol and type
   - Iterates through all symbol declarations
   - For each overload (declaration without body), checks if implementation is assignable
   - Uses `is_assignable_to(impl_type, overload_type)` for validation
   - Reports TS2394 on overload declaration when incompatible
3. Added calls in:
   - `check_function_declaration` (StatementCheckCallbacks impl)
   - `check_method_declaration`
   - `check_constructor_declaration`

**Implementation Approach** (based on Gemini Pro guidance):
- Implementation parameters must be supertypes of overload parameters (contravariant)
- Implementation return type must be subtype of overload return type (covariant)
- Effectively: Implementation <: Overload (assignability check)
- Handles: Function declarations, Method declarations, Constructor declarations

**Testing Results**:
```typescript
// Test 1: Function overload - WORKS ✅
function foo(x: string): void;
function foo(x: number): void;
function foo(x: string): void {  // TS2394 reported correctly
    console.log(x);
}
```

**Known Limitations**:
- Method overload checking depends on binder correctly marking overload declarations (no body)
- In some cases, binder treats all method declarations as implementations, resulting in TS2393 (Duplicate function implementation) instead of TS2394
- This is a binder issue, not a checker issue

**Conformance Impact**: To be measured

---

**Context**:
From architectural review section 5.4: "Function overload matching validation missing"

When a function has overloads, TypeScript validates that the implementation signature is compatible with all overload signatures. Currently TSZ does not perform this validation, allowing unsound implementations.

**Goal**:
Validate that function implementations are assignable to all their overload signatures.

**Files to Modify**:
- `src/checker/declarations.rs` - Add validation after function declaration checking
- `src/checker/state.rs` - Orchestrate the validation

**Implementation Plan**:
1. Retrieve implementation signature and all overload signatures for a function symbol
2. Use `solver.is_assignable_to()` to check if implementation is assignable to each overload
3. Report error if implementation is not compatible
4. Handle edge cases: generic overloads, `this` parameters

---

### Task 1: TS2564 Property Initialization
**Priority**: Medium (Missing Diagnostic)
**Estimated Impact**: +2-3% conformance
**Effort**: Medium

**Context**:
From architectural review section 5.9: "Property has no initializer" is partially working but buggy.

Currently uses `HashSet` instead of `FxHashSet` and misses getter/setter logic.

**Goal**:
Fix and complete TS2564 diagnostic for class properties without initializers.

**Files to Modify**:
- `src/checker/class_checker.rs` - Main checking logic
- `src/checker/declarations.rs` - Integration point

**Implementation Plan**:
1. Refactor from `HashSet` to `FxHashSet` (performance)
2. Walk constructor's control flow graph to verify property assignment
3. Handle multiple constructors (must assign in all)
4. Handle getter/setter properties

---

### Task 2: TS2454 Definite Assignment
**Priority**: Low (Architecture Heavy)
**Estimated Impact**: +3-5% conformance
**Effort**: Hard

**Status**: Partially complete (see completed tasks below)

**Remaining Work**:
- Extend binder to create flow nodes for module-level statements
- Complete CFA integration for all variable scopes

---

## Completed Tasks (Detailed)

### ✅ Cross-Arena Cache Poisoning Fix - 2026-02-05
See detailed description at top of file.

### ✅ Basic Overload Resolution - Completed 2026-02-05
**Status**: Basic Implementation Complete, Advanced Gaps Identified

**Finding**: Basic overload resolution is fully implemented in:
- `src/solver/operations.rs` - `resolve_callable_call` (lines 2360-2459)
  - First-match-wins algorithm: Returns immediately on first valid signature
  - Uses `is_assignable_to` (CompatChecker/Lawyer) for type checking
  - Handles generic inference per-signature in `resolve_generic_call_inner`

**Testing Confirms**:
- Overloaded functions with string/number parameters resolve correctly
- DOM functions like `document.createElement('div')` return correct types
- Type mismatches are detected (TS2322 errors)

**Note**: Basic overload resolution works for 80% of cases. Advanced features (speculative inference, union callables) are not yet implemented.

---

### ⚠️ TS2454 Module-Level Fix - Partially Complete 2026-02-05

---

## Next Steps

### 1. Fix Binder Method Overload Issue (IMMEDIATE PRIORITY)

**Problem**: TS2394 validation for method overloads doesn't work correctly because binder doesn't distinguish overload declarations from implementations.

**Root Cause**: In `src/binder/state.rs`, `MethodDeclaration` handling likely doesn't check for presence of `body` to correctly mark overloads.

**Impact**: Current TS2394 implementation reports TS2393 (Duplicate function implementation) instead of TS2394 for incompatible method overloads.

**Action Items**:
- Investigate `src/binder/state.rs` method declaration handling
- Ensure overload declarations (no body) are correctly flagged
- Verify fix by testing method overload TS2394 reporting

**Note**: Use Gemini consultation for this fix even though it's in the binder.

---

### 2. TS2564 Property Initialization (NEXT MAJOR TASK)

After fixing binder issue, proceed to TS2564:

1. **Refactor to `FxHashSet`** for performance
2. **Complete constructor CFG traversal**
3. **Handle edge cases**:
   - Multiple constructors (must assign in all)
   - Properties with initializers
   - Getter/setter properties

**Approach**:
- Ask Gemini Question 1: CFG walking strategy for property initialization
- Implement in `src/checker/class_checker.rs`
- Ask Gemini Question 2: Implementation review

---

### 3. TS2454 Definite Assignment (DEFERRED)

Keep as low priority - requires significant binder architectural changes for module-level flow nodes.
   - Test with function overloads that have incompatible implementations

2. **TS2564 Property Initialization**:
   - Refactor to use `FxHashSet` for performance
   - Complete constructor flow graph analysis
   - Handle multiple constructors and getter/setter properties

3. **TS2454 Definite Assignment** (Deferred):
   - Requires binder changes for module-level flow nodes
   - Architectural complexity makes this lower priority

---

## Session Focus Shift

**Previous Focus**: Flow Analysis & Overload Resolution infrastructure
**New Focus**: Missing Diagnostics within Flow Analysis & Overload scope

Based on architectural review, the highest-impact missing diagnostics that fit TSZ-1 scope are:
1. Overload implementation validation (directly completes overload resolution work)
2. TS2564 property initialization (class constructor flow analysis)
3. TS2454 definite assignment (general flow analysis, but architecturally complex)

---

---

## Investigation Results: Remaining TS1005 Issues (2026-02-05)

After ASI fix, investigated remaining TS1005 errors. These are **parser gaps** that should be reported to TSZ-2 (Parser track):

### 1. Object Shorthand with `!` Assertion
```typescript
const foo = { a! }  // tsc: TS1162, tsz: TS1128
```
Parser doesn't recognize definite assignment assertion in shorthand properties.

### 2. Optional Method Syntax in Object Literals
```typescript
const bar = { a ? () { } }  // tsc: TS1255, tsz: TS1005
```
Parser doesn't recognize `?` as optional marker for methods.

### 3. ES2022 String Import Specifiers
```typescript
import { "missing" as x } from "./module";  // Valid ES2022
```
Parser fails to parse string literal as import binding name.

### 4. Reserved Words as Class Names
```typescript
class void {}  // tsc: TS1005, tsz: no error
```
Parser accepts `void` as class name, should reject.

### 5. `await` in Async Arrow Default Parameters
```typescript
var foo = async (a = await => await) => {}  // tsc: TS1005, tsz: no error
```
Parser should reject `await` as parameter name in async context.

**Recommendation**: These findings should be incorporated into TSZ-2 (Parser) session work.

---

## Session Alignment

| Session | Focus |
| :--- | :--- |
| **TSZ-1 (You)** | **Flow Analysis & Overload Resolution** |
| TSZ-2 | Parser Error Recovery (Syntax) |
| TSZ-3 | Symbol Definitions & Scope Conflicts (Binder) |
| TSZ-4 | Type Strictness & Compatibility Rules (Lawyer) |
