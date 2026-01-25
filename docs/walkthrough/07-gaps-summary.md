# Implementation Gaps Summary

This document consolidates all known gaps, TODOs, and incomplete implementations across the TSZ codebase. Use this as a guide for prioritizing improvements.

## Overview by Severity

| Severity | Count | Description |
|----------|-------|-------------|
| 🔴 Critical | 8 | Blocks major functionality or causes incorrect behavior |
| 🟡 Moderate | 15 | Missing features or partial implementations |
| 🟢 Minor | 12 | Polish items, dead code, or edge cases |

## Conformance Error Mapping

Current conformance: **37.0%** (4,508 / 12,197 tests)

### False Positives (We report, TSC doesn't)

| Error | Count | Root Cause | Gap |
|-------|-------|------------|-----|
| TS2322 | 11,773x | Type not assignable | Solver: subtype rules, freshness |
| TS2694 | 3,104x | Namespace no exported member | Binder: module resolution |
| TS1005 | 2,703x | '{0}' expected | Parser: error recovery |
| TS2304 | 2,045x | Cannot find name | Binder: symbol resolution |
| TS2571 | 1,681x | Object is 'unknown' | Checker: type narrowing |
| TS2339 | 1,520x | Property doesn't exist | Solver: object types |
| TS2300 | 1,424x | Duplicate identifier | Binder: merge rules |

### False Negatives (TSC reports, we don't)

| Error | Count | Root Cause | Gap |
|-------|-------|------------|-----|
| TS2318 | 3,386x | Cannot find global type | Binder: lib loading |
| TS2307 | 2,139x | Cannot find module | Binder: module resolution |
| TS2488 | 1,749x | Must have Symbol.iterator | Checker: iterator protocol |
| TS2583 | 706x | Change target library? | CLI: lib suggestion |
| TS18050 | 680x | Value cannot be used here | Checker: value/type distinction |

---

## Scanner Gaps

### 🟡 Unicode Support
**Location**: `scanner_impl.rs` in `char_code_unchecked()`
**Issue**: Simplified check treats all non-ASCII as potential identifier chars
```rust
if ch > 127 { /* treat as identifier */ }
```
**Should**: Use proper Unicode category tables (`ID_Start`, `ID_Continue`)
**Impact**: May incorrectly accept/reject Unicode identifiers

### 🟢 Comment Nesting
**Location**: `scanner_impl.rs` in comment scanning
**Issue**: Doesn't track nested `/* */` comments
```typescript
/* outer /* inner */ */  // Edge case not handled
```
**Impact**: Rare edge case, differs from TSC

### 🟢 Octal Escapes in Templates
**Location**: `scanner_impl.rs` in template escape handling
**Issue**: Not fully implemented (comment: "octal in template is complex")
**Impact**: May misparse legacy code with octal escapes

### 🟢 Regex Flag Validation
**Location**: `scanner_impl.rs` in regex scanning
**Issue**: Lists valid flags but doesn't validate combinations
**Impact**: Invalid flag combinations accepted

---

## Parser Gaps

### 🟡 Expressions Module Disabled
**Location**: `parser/parse_rules/mod.rs`
```rust
// expressions module has incompatible API - commented out until fixed
// mod expressions;
```
**Impact**: All expression logic inline in `state.rs`, harder to maintain

### 🟢 Incremental Parsing
**Location**: `parser/state.rs` `parse_source_file_statements_from_offset()`
**Issue**: Method exists but appears to have limited testing
**Impact**: Incremental parsing infrastructure may not be fully utilized

### 🟢 Dead JSX Fragment Code
**Location**: `parser/state.rs`
```rust
#[allow(dead_code)]
fn look_ahead_is_jsx_fragment(&mut self) -> bool
```
**Impact**: Dead code, possible oversight

---

## Binder Gaps

### 🔴 Import Resolution Requires External Setup
**Location**: `binder/state.rs` in `resolve_import_with_reexports()`
**Issue**: Method depends on pre-populated `module_exports`
**Impact**: Binder doesn't do file system resolution; requires external module resolver

### 🟡 Default Export Handling
**Location**: `binder/state.rs` in export binding
```rust
// Best-effort only: export default CONFIG marks CONFIG as exported
// Doesn't synthesize separate "default" export symbol
```
**Impact**: `import X from './file'` may not resolve correctly

### 🟡 Flow Analysis - Await/Yield Points
**Location**: `binder.rs` flow flags
```rust
pub const AWAIT_POINT: u32 = 1 << 12;
pub const YIELD_POINT: u32 = 1 << 13;
```
**Issue**: Flags defined but `bind_node()` doesn't generate these flow nodes
**Impact**: Control flow doesn't account for async suspension

### 🟡 Array Mutation Flow
**Location**: `binder/state.rs`
**Issue**: `create_flow_array_mutation()` exists but isn't called
**Impact**: `arr.push(x)` doesn't create flow node for narrowing

### 🟢 Type-Only Import Validation
**Location**: `binder.rs` Symbol struct
**Issue**: `is_type_only` tracked but no validation against value usage
**Impact**: `import type { X }; new X()` may not error

### 🟢 Shorthand Ambient Modules
**Location**: `binder/state.rs`
**Issue**: `declare module "foo"` without body tracked but no symbol created
**Impact**: Module resolves to `any` but binding incomplete

---

## Checker Gaps

### 🔴 Definite Assignment Analysis
**Location**: `checker/flow_analysis.rs`
```rust
// TODO: Implement proper definite assignment checking
```
**Impact**: May not catch uninitialized variable uses (TS2454)

### 🔴 TDZ Checking
**Location**: `checker/flow_analysis.rs`
```rust
// TODO: Implement TDZ checking for static blocks
// TODO: Implement TDZ checking for computed properties
// TODO: Implement TDZ checking for heritage clauses
```
**Impact**: Temporal Dead Zone violations not caught in several contexts

### 🟡 Promise Detection
**Location**: `checker/state.rs`, `function_type.rs`
```rust
// TODO: Investigate lib loading for Promise detection
```
**Impact**: Promise type checking may fail if lib loading fails

### 🟡 Reference Tracking Disabled
**Location**: `checker/type_checking.rs`
```rust
// TODO: Re-enable and fix reference tracking system properly
```
**Impact**: Some reference tracking disabled

### 🟡 Module Validation Disabled
**Location**: `checker/mod.rs`
```rust
// mod module_validation;  // TODO: Fix API mismatches
```
**Impact**: Module/namespace validation disabled

### 🟡 Generator Call Signatures
**Location**: `checker/iterable_checker.rs`
```rust
// TODO: Check call signatures for generators when CallableShape is implemented
```
**Impact**: Generator return type checking incomplete

### 🟢 Symbol Resolution by Name
**Location**: `checker/type_checking.rs`
```rust
// TODO: Implement when get_symbol_by_name is available
```
**Impact**: Some symbol lookups may fail

---

## Solver Gaps

### 🔴 Freshness/Excess Property Checks (Rule #4)
**Location**: `solver/` - FreshnessTracker exists but not integrated
**Issue**: Object literal excess properties not fully checked
```typescript
const obj: { x: number } = { x: 1, y: 2 };  // Should error on y
```
**Impact**: Major TypeScript feature missing

### 🔴 Tracer Module Disabled
**Location**: `solver/mod.rs`
```rust
// mod tracer;  // TODO: Fix type mismatches
```
**Impact**: Diagnostic tracing disabled, harder to debug type errors

### 🟡 Keyof Contravariance (Rule #30)
**Location**: `solver/evaluate_rules/keyof.rs`
**Issue**: Union inversion incomplete
```typescript
// keyof (A | B) should equal (keyof A) & (keyof B)
```
**Impact**: Some keyof expressions evaluate incorrectly

### 🟡 Intersection Reduction (Rule #21)
**Location**: `solver/subtype.rs`
**Issue**: Only handles primitive intersections, not object literal disjoint detection
```typescript
type X = { kind: "a" } & { kind: "b" };  // Should reduce to never
```
**Impact**: Some impossible intersections not detected

### 🟡 Array-to-Tuple Rejection (Rule #15)
**Location**: `solver/subtype_rules/tuples.rs`
**Issue**: Tuple-to-array works, but array-to-tuple incomplete
```typescript
let arr: string[] = ["a"];
let tuple: [string] = arr;  // Should error
```
**Impact**: Unsound array/tuple assignments allowed

### 🟡 Rest Parameter Bivariance (Rule #16)
**Location**: `solver/subtype_rules/functions.rs`
**Issue**: Flag exists but full semantics incomplete
**Impact**: Some rest parameter type errors missed

### 🟡 Base Constraint Assignability (Rule #31)
**Location**: `solver/subtype_rules/generics.rs`
**Issue**: Type parameter constraint checking partial
**Impact**: Some generic constraint violations missed

### 🟡 Template Literal Cross-Product (Rule #38)
**Location**: `solver/evaluate_rules/template_literal.rs`
```rust
const TEMPLATE_LITERAL_EXPANSION_LIMIT: usize = 100_000;
```
**Issue**: No correlated access optimization for large unions
**Impact**: Large template types hit expansion limit

### 🟢 CFA Invalidation in Closures (Rule #42)
**Location**: Not implemented
```typescript
function f(x: string | null) {
    if (x !== null) {
        setTimeout(() => x.length, 0);  // x might be null now
    }
    x = null;
}
```
**Impact**: Closures may see stale narrowing

### 🟢 Import Type Erasure (Rule #39)
**Location**: Not implemented
**Issue**: Value/type space separation incomplete
**Impact**: Some import type errors missed

### 🟢 Module Augmentation Merging (Rule #44)
**Location**: Not implemented
**Issue**: Module augmentation declarations not fully merged
**Impact**: Augmented types may not resolve

### 🟢 JSX Intrinsic Lookup (Rule #36)
**Location**: Not implemented
**Issue**: JSX.IntrinsicElements not consulted
**Impact**: JSX element type checking incomplete

### 🟢 Comparison Operator Overlap (Rule #23)
**Location**: Not implemented
**Issue**: No check for `==`/`===` between incompatible types
**Impact**: Always-false comparisons not warned

---

## Emitter Gaps

### 🟡 Decorator ES5 Emission
**Location**: `emitter/special_expressions.rs`
```rust
if self.ctx.target_es5 {
    return;  // Skipped entirely
}
```
**Impact**: Decorators not downleveled to ES5

### 🟢 Interface/Type Alias Infrastructure
**Location**: `emitter/declarations.rs`
```rust
#[allow(dead_code)]
fn emit_interface_declaration(&mut self, ...) { ... }
```
**Note**: Types stripped in JS output - infrastructure only

### 🟢 Export Assignment Suppression
**Location**: `emitter/mod.rs`
**Issue**: `has_export_assignment` flag not fully integrated
**Impact**: Some export edge cases may misbehave

### 🟢 Recursion Overflow Handling
**Location**: `emitter/mod.rs`
```rust
if self.emit_recursion_depth > MAX_EMIT_RECURSION_DEPTH {
    self.writer.write("/* emit recursion limit exceeded */");
}
```
**Impact**: Very deep ASTs emit comment instead of code

---

## Critical Limits Reference

| Constant | Value | Module | Purpose |
|----------|-------|--------|---------|
| MAX_SUBTYPE_DEPTH | 100 | Solver | Subtype recursion |
| MAX_TOTAL_SUBTYPE_CHECKS | 100,000 | Solver | Total checks |
| MAX_INSTANTIATION_DEPTH | 50 | Checker | Generic instantiation |
| MAX_CALL_DEPTH | 20 | Checker | Call resolution |
| MAX_EMIT_RECURSION_DEPTH | 1,000 | Emitter | Code generation |
| MAX_RECURSION_DEPTH | 1,000 | Parser | Parse recursion |
| MAX_TYPE_RESOLUTION_OPS | 500,000 | Checker | Fuel counter |
| MAX_EVALUATE_DEPTH | 50 | Solver | Type evaluation |
| MAX_TOTAL_EVALUATIONS | 100,000 | Solver | Evaluation count |
| TEMPLATE_LITERAL_EXPANSION_LIMIT | 100,000 | Solver | Template expansion |

---

## TypeScript Unsoundness Rules Status

From `solver/unsoundness_audit.rs`:

### Fully Implemented (28/44)

- ✅ The "Any" Type
- ✅ Error Poisoning
- ✅ Covariant Mutable Arrays
- ✅ Void Return Exception
- ✅ The Object/object/{} Trifecta
- ✅ Literal Widening
- ✅ Covariant this Types
- ✅ Optionality vs Undefined
- ✅ Index Signature Consistency
- ✅ Distributivity Disabling
- ✅ Key Remapping & Filtering
- ✅ Split Accessor Variance
- ✅ BCT Inference
- ✅ Open Numeric Enums
- ✅ Cross-Enum Assignment
- ✅ String Enums to String
- ✅ Nominal Classes
- ✅ Static Side Compatibility
- ✅ Abstract Instantiation
- ✅ Constructor Depth
- ✅ Instantiation Depth
- ✅ Template String Limits
- ✅ Unique Symbol Opacity
- And more...

### Partially Implemented (9/44)

- ⚠️ Function Bivariance (method bivariance only)
- ⚠️ Freshness/Excess Property Checks
- ⚠️ Keyof Contravariance
- ⚠️ Intersection Reduction
- ⚠️ Tuple-Array Assignment
- ⚠️ Rest Parameter Bivariance
- ⚠️ Base Constraint Assignability
- ⚠️ Primitive Boxing (apparent types partial)
- ⚠️ Typeof Type Queries

### Not Implemented (7/44)

- ❌ Import Type Erasure
- ❌ Module Augmentation Merging
- ❌ JSX Intrinsic Lookup
- ❌ Comparison Operator Overlap
- ❌ CFA Invalidation in Closures
- ❌ Correlated Unions Cross-Product
- ❌ Declaration Emit Inference

---

## Recommended Priority Order

### Phase 1: Critical Path (Highest Impact)

1. **Freshness/Excess Property Checks** - Major TypeScript feature
2. **Definite Assignment Analysis** - Common runtime errors
3. **Tracer Module** - Enables better debugging
4. **TDZ Checking** - JavaScript semantics correctness

### Phase 2: Conformance Improvement

5. **Module Resolution** - Many test failures
6. **Symbol Resolution** - TS2304 errors
7. **Iterator Protocol** - TS2488 errors
8. **Lib Loading** - TS2318 errors

### Phase 3: Type System Completeness

9. **Keyof Contravariance**
10. **Intersection Reduction**
11. **Array-to-Tuple Rejection**
12. **Template Literal Optimization**

### Phase 4: Edge Cases

13. **Unicode Identifiers**
14. **CFA in Closures**
15. **Module Augmentation**
16. **JSX Intrinsics**

---

## Contributing

When fixing a gap:

1. Search for the TODO/FIXME in the codebase
2. Add conformance tests that exercise the gap
3. Implement the fix
4. Verify conformance improvement
5. Update this document

**See also**:
- [PROJECT_DIRECTION.md](../../PROJECT_DIRECTION.md) - Conformance improvement plan
- [specs/TS_UNSOUNDNESS_CATALOG.md](../../specs/TS_UNSOUNDNESS_CATALOG.md) - Full unsoundness catalog
- [AGENTS.md](../../AGENTS.md) - Contribution guidelines

---

*Last updated: January 2025*
