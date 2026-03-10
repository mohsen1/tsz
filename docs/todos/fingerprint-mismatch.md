# Fingerprint Mismatch Root Cause Analysis

**Date:** 2026-03-09
**Scope:** 710 conformance tests where error code sets match but fingerprints differ
**Method:** Programmatic analysis of 75-test stratified sample + manual deep-dive of 20 tests

---

## Executive Summary

**32.1% of all conformance failures (710 / 2,214)** emit the correct set of unique error codes but still fail because the diagnostic *fingerprints* differ. Each fingerprint is a 5-tuple of `(code, file, line, column, message)` — so even when tsz produces the right error code, it can fail on position or message text.

These 710 tests represent the **lowest-hanging fruit** in the conformance suite. Unlike tests with wrong/missing error codes (which need new checker/solver logic), these tests already trigger the right diagnostic — they just need the diagnostic to be emitted at the right place with the right message.

**The root causes collapse into 5 major categories**, and fixing just 3 of them would likely recover 400+ tests.

---

## How Conformance Comparison Works

The conformance runner performs a **two-level comparison**:

1. **Error code level:** Sorted unique set of error codes (e.g., `[TS2322, TS2345]`)
2. **Fingerprint level:** Individual diagnostic instances compared on all 5 fields

A test passes **only if** both levels match. The baseline file only shows the code-level view, which is why these 710 tests appear paradoxically as `expected:[TS2322] actual:[TS2322]` — the code-level sets match, but the fingerprints underneath don't.

---

## Root Cause Taxonomy

### Distribution from 75-test sample (194 individual fingerprint mismatches)

| Root Cause | Fingerprints | % | Est. Tests (of 710) |
|---|---:|---:|---:|
| 1. Wrong position AND wrong message | 44 | 22.7% | ~160 |
| 2. Same message template, different type names | 34 | 17.5% | ~125 |
| 3. Numeric literal widening (`'0'` → `'number'`) | 32 | 16.5% | ~115 |
| 4. Column offset (same line) | 22 | 11.3% | ~80 |
| 5. Under-emitting (missing fingerprints, no extras) | 17 | 8.8% | ~60 |
| 6. Property list ordering in TS2740 messages | 13 | 6.7% | ~50 |
| 7. Unmatched missing (code present, but wrong entity) | 12 | 6.2% | ~45 |
| 8. Over-emitting (extra fingerprints, no missing) | 10 | 5.2% | ~35 |
| 9. Different line, same message | 8 | 4.1% | ~30 |
| 10. `typeof X` vs structural display | 1 | 0.5% | ~5 |

---

## Deep Dive: The 5 Major Root Causes

### RC-1: Type Display Differences in Messages (~40% of all fingerprint mismatches)

**Categories 2 + 3 + 6 + 10 combined = 80 fingerprints (41.2%)**

This is the single biggest root cause. tsz emits the right error code at the right position, but the type names in the diagnostic message differ from what tsc produces.

#### Sub-pattern A: Numeric Literal Widening (32 fingerprints) — DONE

```
tsc:  "types '0' and '1' have no overlap"
tsz:  "types 'number' and 'number' have no overlap"
```

**Affected tests:** `capturedLetConstInLoop8`, `capturedLetConstInLoop8_ES6` (16 fingerprints each)

**Status: FIXED** (commit `c8510aeba` — 2026-03-10)

**Fix:** Family-aware widening heuristic in TS2367 message generation (`binary.rs`).
The root cause was not missing literal type preservation in the solver — const variables
already had correct literal types. The issue was in the TS2367 display path which
unconditionally widened number/boolean literals via `widen_non_string_bigint_literal`.
The fix uses a two-tier heuristic: same primitive family → preserve all literals;
different family → widen only number/boolean (preserving string/bigint).

#### Sub-pattern B: String Literal / Union Widening (34 fingerprints)

```
tsc:  "Type '(val: Values) => "1" | "2" | "3" | "4" | "5" | undefined'"
tsz:  "Type '(val: Values) => string | void'"
```

```
tsc:  "Operator '+' cannot be applied to types 'I' and 'E'"
tsz:  "Operator '+' cannot be applied to types 'I' and 'number'"
```

**Affected tests:** `classPropertyErrorOnNameOnly`, `expr`, `enumBasics`, many others

**Root cause:** Multiple related issues:
- Switch-case return types widened from literal union to `string`
- Enum types displayed as `number` instead of the enum name
- `undefined` displayed as `void` in return type contexts
- Return type inference not preserving literal types from branches

**Solver location:** Return type inference (`evaluate`), enum type display, literal preservation policy.

#### Sub-pattern C: Property List Ordering (13 fingerprints)

```
tsc:  "missing properties: length, pop, push, concat, and 25 more"
tsz:  "missing properties: lastIndexOf, concat, entries, indexOf, toString, and 26 more"
```

**Affected tests:** `arrayAssignmentTest1` (5 fingerprints), `noInferUnionExcessPropertyCheck1`

**Root cause:** When listing missing properties in TS2740 messages, tsz iterates object properties in a different order than tsc. tsc lists them in declaration order; tsz appears to use a different traversal order.

**Fix location:** Property enumeration order in the solver's "missing properties" diagnostic helper.

#### Sub-pattern D: `typeof` vs Structural Display (1 fingerprint, but widespread pattern)

```
tsc:  "Type 'typeof A' is not assignable to type 'new () => A'"
tsz:  "Type '{ new (): { ; }; prototype: { ; }; }' is not assignable to type 'new () => A'"
```

**Root cause:** tsz prints the structural expansion of a class constructor type instead of using the `typeof ClassName` shorthand.

---

### RC-2: Error Span Targeting (~27% of fingerprint mismatches)

**Categories 1 + 4 + 9 combined = 74 fingerprints (38.1%)**

tsz places the error at the wrong source location. This has three sub-patterns:

#### Sub-pattern A: Container vs Element (44 fingerprints — the largest single category)

```
tsc:  TS2322 test.ts:1:51  "Type 'number' not assignable to type '{ id: number; }'"
tsz:  TS2322 test.ts:1:36  "Type '(number | { id: number; })[]' not assignable to type '{ id: number; }[]'"
```

```
tsc:  TS2322 test.ts:22:13  "Type 'number' not assignable to type 'number[]'"
tsz:  TS2322 test.ts:22:1   "Type 'number[][]' not assignable to type 'number[][][]'"
```

**Affected tests:** `contextualTyping21`, `arraySigChecking`, `conditionalReturnExpression`, `contextualTypeArrayReturnType`, many more

**Root cause:** tsc **elaborates** assignability failures — when an array/object is not assignable, it drills into the specific element or property that caused the failure and points the error there. tsz reports the error on the outer container expression.

This is the **highest-impact single issue**. The elaboration logic determines both the error span AND the message text, so fixing it would simultaneously fix RC-1 sub-pattern B for many tests.

**Checker/Solver location:** Assignability error elaboration in the checker's diagnostic rendering path. tsc has `elaborateError` which recursively narrows the error span to the deepest failing constituent.

#### Sub-pattern B: Column Offset (22 fingerprints)

```
tsc:  TS2352 test.ts:3:23  (points to the expression being cast)
tsz:  TS2352 test.ts:3:1   (points to the entire type assertion)
```

```
tsc:  TS1011 test.ts:10:30
tsz:  TS1011 test.ts:10:36  (off by 6 columns)
```

**Root cause:** For type assertions, tsz uses the span of the entire assertion expression instead of the right-hand operand. For element access with bracket syntax, there appear to be column calculation differences related to whitespace handling.

#### Sub-pattern C: Wrong Line (8 fingerprints)

```
tsc:  TS2403 test.ts:4:29  (duplicate identifier on declaration)
tsz:  TS2403 test.ts:5:1   (different declaration chosen)
```

**Root cause:** When multiple declarations of the same name exist, tsz picks a different one to report the error on. This is a binder/checker issue in choosing which declaration to flag.

---

### RC-3: Missing Fingerprints / Under-Emitting (~9%)

**Category 5: 17 fingerprints**

Tests where tsz emits at least one of each expected error code (so the code set matches), but emits fewer instances than tsc expects.

```
Example: accessors_spec_section-4.5_error-cases.ts
  tsc expects 4x TS2322 (lines 3, 5, 9, 11)
  tsz emits  2x TS2322 (lines 3, 5 only — misses getter/setter pair)
```

```
Example: constructorOverloads1.ts
  tsc expects 2x TS2392 (lines 2, 3)
  tsz emits  0x TS2392 at those locations (but emits at different locations)
```

**Root cause:** Incomplete checking of certain patterns:
- Accessor getter/setter type consistency (not checking both directions)
- Multiple constructor overload validation
- Duplicate identifier reporting for merged declarations

---

### RC-4: Extra Fingerprints / Over-Emitting (~5%)

**Category 8: 10 fingerprints**

Tests where tsz emits more diagnostic instances than tsc expects.

```
Example: contextualTypeAny.ts
  tsc expects 1x TS2322
  tsz emits  2x TS2322 (extra one through `any` context)
```

```
Example: deleteExpressionMustBeOptional.ts
  tsc expects 0x TS2790 at lines 28,30
  tsz emits  2x TS2790 at lines 28,30
```

**Root cause:**
- `any` propagation not silencing downstream errors properly
- Incomplete narrowing leaving types too wide, triggering extra errors
- False positive diagnostics in edge cases

---

### RC-5: Entity Name Resolution (~6%)

**Category 7: 12 fingerprints**

```
tsc:  "Namespace 'foo.bar.baz' has no exported member 'bar'"
tsz:  "Namespace 'booz' has no exported member 'bar'"
```

```
tsc:  "'foo' is referenced directly or indirectly in its own type annotation"
tsz:  "'c1' is referenced directly or indirectly in its own type annotation"
```

**Root cause:** When resolving names for diagnostic messages, tsz uses the wrong symbol:
- Import aliases are not resolved to their original namespace paths
- Circularity detection blames the containing variable instead of the accessor
- Duplicate identifier checking picks the wrong declaration in the symbol chain

---

## Impact-Ordered Action Plan

### Phase 1: Elaboration Depth (est. ~200 tests recovered)

**Priority: HIGHEST | Difficulty: HIGH | Location: Checker + Solver boundary**

Implement recursive assignability error elaboration matching tsc's `elaborateError` behavior:
1. When an array assignment fails, drill into the specific element index that fails
2. When an object assignment fails, drill into the specific property that fails
3. Update both the error span and the message text to reflect the deepest failure

This single fix addresses both RC-2A (container-vs-element spans) and much of RC-1B (message text differences caused by reporting at wrong granularity).

**Key files to investigate:**
- Checker's diagnostic rendering for TS2322/TS2345/TS2741
- Solver's relation failure reasons (needs to expose the failing constituent path)
- `query_boundaries` assignability gate

### Phase 2: Literal Type Preservation (est. ~120 tests recovered)

**Priority: HIGH | Difficulty: MEDIUM | Location: Solver (evaluate/narrowing)**

1. **Const narrowing:** `const x = 0` should have type `0`, not `number`. This is a solver narrowing issue — const bindings in for-loops need literal type preservation.
2. **Return type inference:** Switch/case branches returning string literals should produce a union of literals, not `string`.
3. **Enum display:** Enum values should display as the enum type name, not `number`.
4. **`undefined` vs `void`:** In return type positions, use `undefined` not `void` when that's what the control flow produces.

### Phase 3: Error Span Fixes (est. ~80 tests recovered)

**Priority: MEDIUM | Difficulty: LOW-MEDIUM | Location: Checker**

1. **Type assertion spans:** For `<T>expr` and `expr as T`, point the error at `expr` (the right-hand side), not the entire assertion expression.
2. **Column calculation:** Audit element access expression column offsets.
3. **Declaration choice:** When reporting duplicate identifiers, match tsc's heuristic for which declaration to flag.

### Phase 4: Property Enumeration Order (est. ~50 tests recovered)

**Priority: MEDIUM | Difficulty: LOW | Location: Solver diagnostic helper**

Match tsc's property iteration order (declaration order) when listing missing properties in TS2740 messages. This is likely a simple sort/ordering fix in the property enumeration used by the "missing properties" diagnostic.

### Phase 5: Entity Name Resolution (est. ~45 tests recovered)

**Priority: LOW-MEDIUM | Difficulty: MEDIUM | Location: Checker/Binder**

1. Resolve import aliases to their original namespace paths in diagnostic messages
2. Fix circularity detection to blame the accessor, not the containing variable
3. Fix duplicate identifier reporting to pick the same declaration as tsc

### Phase 6: Emission Count Fixes (est. ~95 tests recovered)

**Priority: LOW-MEDIUM | Difficulty: VARIES | Location: Checker**

1. Complete accessor getter/setter bidirectional type checking
2. Complete constructor overload validation
3. Fix `any` propagation to properly silence downstream diagnostics

---

## Verification Strategy

After each phase, run targeted conformance:

```bash
# Run only same-codes failures to measure recovery
./scripts/conformance/conformance.sh run --filter "PATTERN" --verbose

# After all phases, update snapshot
./scripts/conformance/conformance.sh snapshot
```

Expected total recovery: **~400-500 tests** from the 710 fingerprint-only failures, representing a **~3-4 percentage point** improvement in overall conformance (from 82.4% toward ~86%).

---

## Appendix: Top 10 Error Codes in Same-Codes Failures

| Code | Count | Primary Pattern |
|---|---:|---|
| TS2322 | 123 | Elaboration depth + type display |
| TS2345 | 34 | Elaboration depth + type display |
| TS1005 | 22 | Parser column offsets |
| TS2339 | 16 | Entity name resolution |
| TS2304 | 13 | Entity name resolution |
| TS2564 | 13 | Under-emitting |
| TS2353 | 7 | Excess property check elaboration |
| TS7053 | 5 | Index signature display |
| TS6133 | 5 | Unused variable detection |
| TS2454 | 5 | Definite assignment |

## Appendix: Test Area Distribution

| Area | Same-Codes Failures |
|---|---:|
| compiler | 376 |
| types | 107 |
| parser | 44 |
| expressions | 41 |
| es6 | 34 |
| classes | 31 |
| jsx | 24 |
| jsdoc | 16 |
| externalModules | 13 |
| statements | 8 |
| controlFlow | 5 |
