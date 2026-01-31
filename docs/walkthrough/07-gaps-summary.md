# Implementation Gaps Summary

This document consolidates all known gaps, TODOs, and incomplete implementations across the TSZ codebase. Use this as a guide for prioritizing improvements.

## Overview by Severity

| Severity | Count | Description |
|----------|-------|-------------|
| 🔴 Critical | 5 | Blocks major functionality or causes incorrect behavior |
| 🟡 Moderate | 11 | Missing features or partial implementations |
| 🟢 Minor | 10 | Polish items, dead code, or edge cases |
| ✅ Resolved | 7 | Recently fixed (see Resolved Gaps section) |

## Conformance Error Mapping

Run `./scripts/conformance/run.sh --server` to get current conformance rate.

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
**Location**: `scanner_impl.rs` - `is_identifier_start()`, `is_identifier_part()`
**Issue**: Simplified check treats all non-ASCII (> 127) as potential identifier chars
**Should**: Use proper Unicode category tables (`ID_Start`, `ID_Continue`)
**Impact**: May incorrectly accept/reject Unicode identifiers

### ✅ RESOLVED: Octal Escapes in Templates
**Location**: `scanner_impl.rs` - `scan_template_escape_sequence()`
Octal escape sequences in template literals now properly set the `ContainsInvalidEscape` flag.

### ✅ RESOLVED: Regex Flag Validation
**Location**: `scanner_impl.rs` - `re_scan_slash_token()`
Regex flag validation now detects duplicate flags, invalid flags, and incompatible flags (u/v).
Use `get_regex_flag_error()` and `get_regex_flag_error_pos()` to retrieve error information.

### Note: Comment Nesting (Not a Gap)
JavaScript/TypeScript does **not** support nested multi-line comments. The scanner's behavior of finding the first `*/` to close a `/* */` comment is correct and matches TSC behavior.

---

## Parser Gaps

### ✅ RESOLVED: Expression Parsing Architecture
**Location**: `parser/state.rs`, `parser/parse_rules/mod.rs`

Expression parsing is intentionally implemented directly in `state.rs` using methods on `ParserState`
for optimal performance and simpler control flow. This was a deliberate design decision - see
[02-parser.md](./02-parser.md#resolved-design-decisions) for details.

### ✅ RESOLVED: Incremental Parsing
**Location**: `parser/state.rs` `parse_source_file_statements_from_offset()`

Incremental parsing is fully implemented and tested. Tests cover parsing from middle of file,
parsing from start (offset 0), handling offsets beyond EOF, and recovery from syntax errors.

### ✅ RESOLVED: Expression Statement Recovery
**Location**: `parser/state.rs`

Expression recovery has been enhanced with `is_expression_boundary()`, `create_missing_expression()`,
and `try_recover_binary_rhs()` to ensure the parser produces structurally valid ASTs even with errors.

### 🟢 JSX Fragment Detection
**Location**: `parser/state.rs`

JSX fragment detection (`<>`) is performed inline during `parse_jsx_opening_or_self_closing_or_fragment`
rather than via a separate lookahead function. This is intentional for efficiency - no backtracking
needed when we can check for `>` immediately after consuming `<`.

---

## Binder Gaps

### 🔴 Import Resolution Requires External Setup
**Location**: `binder/state.rs` in `resolve_import_with_reexports()`
**Issue**: Method depends on pre-populated `module_exports`
**Impact**: Binder doesn't do file system resolution; requires external module resolver

### ✅ RESOLVED: Default Export Handling
**Location**: `binder/state.rs` in export binding

Default export handling is now fully implemented:
- Synthesizes a "default" export symbol with `ALIAS | EXPORT_VALUE` flags
- Adds to `file_locals` for cross-file import resolution
- Also marks underlying local symbol as exported
- `import X from './file'` now correctly resolves default exports

### ✅ RESOLVED: Flow Analysis - Await/Yield Points
**Location**: `binder/state.rs`

Await and yield expressions now properly generate flow nodes:
- `create_flow_await_point()` creates `AWAIT_POINT` flow nodes
- `create_flow_yield_point()` creates `YIELD_POINT` flow nodes
- Called during `bind_node()` for await/yield expressions
- Control flow analysis now accounts for async suspension points

### ✅ RESOLVED: Local Symbol Shadowing
**Location**: `binder/state.rs`

Local declarations now properly shadow lib symbols. When a local declaration
conflicts with a lib symbol, a new local symbol is created that shadows the lib symbol.

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

### ✅ RESOLVED: Freshness/Excess Property Checks (Rule #4)
**Location**: `solver/`

FreshnessTracker is now integrated with excess property checking in `check_object_literal_excess_properties`.
Only fresh object literals (direct object literal expressions) trigger excess property errors:
```typescript
const obj: { x: number } = { x: 1, y: 2 };  // Now correctly errors on y
```

### ✅ RESOLVED: Tracer Module
**Location**: `solver/mod.rs`, `solver/tracer.rs`

The tracer module is now enabled and working. Fixed type mismatches:
- Updated function/tuple/object parameter types to use shape IDs
- Fixed union/intersection to use TypeListId
- Corrected intrinsic subtype checking

### ✅ RESOLVED: Keyof Contravariance (Rule #30)
**Location**: `solver/evaluate_rules/keyof.rs`

Union inversion is correctly implemented:
- `keyof (A | B) = (keyof A) & (keyof B)` - distributive contravariance
- `keyof (A & B) = (keyof A) | (keyof B)` - covariance

### 🟡 Intersection Reduction (Rule #21)
**Location**: `solver/subtype.rs`
**Issue**: Only handles primitive intersections, not object literal disjoint detection
```typescript
type X = { kind: "a" } & { kind: "b" };  // Should reduce to never
```
**Impact**: Some impossible intersections not detected

### ✅ RESOLVED: Array-to-Tuple Rejection (Rule #15)
**Location**: `solver/subtype_rules/tuples.rs`

Array-to-tuple rejection is correctly implemented:
- Arrays (`T[]`) are NOT assignable to tuple types
- Exception: `never[]` can be assigned to tuples that allow empty
```typescript
let arr: string[] = ["a"];
let tuple: [string] = arr;  // Now correctly errors
```

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
**Location**: `emitter/special_expressions.rs` - `emit_decorator()`

Decorator ES5 lowering is not fully implemented. The emitter emits a warning comment instead of silently skipping.
**Impact**: Decorators not downleveled to ES5, but warning is visible in output
**Note**: Full decorator ES5 lowering requires class-level coordination with `__decorate` helper.

### 🟢 Interface/Type Alias Infrastructure
**Location**: `emitter/declarations.rs` - `emit_interface_declaration()`, `emit_type_alias_declaration()`
**Note**: Types stripped in JS output - infrastructure only for potential .d.ts emission

### 🟢 Export Assignment Suppression
**Location**: `emitter/mod.rs`
**Issue**: `has_export_assignment` flag not fully integrated
**Impact**: Some export edge cases may misbehave

### 🟢 Recursion Overflow Handling
**Location**: `emitter/mod.rs`
**Behavior**: When `emit_recursion_depth > MAX_EMIT_RECURSION_DEPTH`, emits comment instead of code
**Impact**: Very deep ASTs (>1000 levels) emit comment marker (intentional safety limit)

---

## Critical Limits Reference

All constants are defined in `src/limits.rs`:

| Constant | Value | Purpose |
|----------|-------|---------|
| MAX_SUBTYPE_DEPTH | 100 | Subtype recursion |
| MAX_TOTAL_SUBTYPE_CHECKS | 100,000 | Total checks |
| MAX_INSTANTIATION_DEPTH | 50 | Generic instantiation |
| MAX_CALL_DEPTH | 20 | Call resolution |
| MAX_EMIT_RECURSION_DEPTH | 1,000 | Code generation |
| MAX_PARSER_RECURSION_DEPTH | 1,000 | Parse recursion |
| MAX_TYPE_RESOLUTION_OPS | 100,000 (native) / 20,000 (WASM) | Fuel counter |
| MAX_EVALUATE_DEPTH | 50 | Type evaluation |
| MAX_TOTAL_EVALUATIONS | 100,000 | Evaluation count |
| TEMPLATE_LITERAL_EXPANSION_LIMIT | 100,000 (native) / 2,000 (WASM) | Template expansion |

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

### Partially Implemented (6/44)

- ⚠️ Function Bivariance (method bivariance only)
- ⚠️ Intersection Reduction
- ⚠️ Rest Parameter Bivariance
- ⚠️ Base Constraint Assignability
- ⚠️ Primitive Boxing (apparent types partial)
- ⚠️ Typeof Type Queries

### Recently Fixed (3/44)

- ✅ Freshness/Excess Property Checks (now integrated)
- ✅ Keyof Contravariance (union inversion implemented)
- ✅ Tuple-Array Assignment (array-to-tuple rejection working)

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

1. **Definite Assignment Analysis** - Common runtime errors (TS2454)
2. **TDZ Checking** - JavaScript semantics correctness
3. **Intersection Reduction** - Disjoint object literal detection

### Phase 2: Conformance Improvement

4. **Module Resolution** - Many test failures (TS2307)
5. **Symbol Resolution** - TS2304 errors
6. **Iterator Protocol** - TS2488 errors
7. **Lib Loading** - TS2318 errors

### Phase 3: Type System Completeness

8. **Template Literal Optimization** - Large union handling
9. **Rest Parameter Bivariance** - Full semantics
10. **Base Constraint Assignability** - Generic constraint checking

### Phase 4: Edge Cases

11. **Unicode Identifiers** - Proper ID_Start/ID_Continue
12. **CFA in Closures** - Stale narrowing detection
13. **Module Augmentation** - Declaration merging
14. **JSX Intrinsics** - JSX.IntrinsicElements lookup
15. **Array Mutation Flow** - Narrowing after `.push()`

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
- [docs/specs/TS_UNSOUNDNESS_CATALOG.md](../specs/TS_UNSOUNDNESS_CATALOG.md) - Full unsoundness catalog
- [AGENTS.md](../../AGENTS.md) - Contribution guidelines

---

## Recently Resolved Gaps

The following gaps were resolved in recent PRs:

### PR #161 - Parser Improvements
- **Expression Parsing Architecture**: Documented as intentional design decision (not disabled)
- **Incremental Parsing**: Fully implemented with comprehensive tests
- **Expression Statement Recovery**: Enhanced with boundary detection and placeholder nodes

### PR #160 - Binder Improvements
- **Default Export Handling**: Synthesizes proper "default" export symbol
- **Await/Yield Flow Analysis**: Now generates AWAIT_POINT/YIELD_POINT flow nodes

### PR #159 - Solver Improvements
- **Tracer Module**: Re-enabled with fixed type signatures
- **Keyof Contravariance**: Union inversion correctly implemented
- **Array-to-Tuple Rejection**: Arrays no longer assignable to tuples
- **Freshness/Excess Property Checks**: FreshnessTracker integrated

### PR #158 - Emitter Improvements
- **Decorator Handling**: Now emits warning comment in ES5 mode instead of silent skip
- **Special Expressions**: Improved null safety and spread handling

### Other Fixes
- **Local Symbol Shadowing**: Local declarations properly shadow lib symbols
- **TS2507 Constructor Checking**: Improved constructor type checking

---

*Last updated: January 2026*
