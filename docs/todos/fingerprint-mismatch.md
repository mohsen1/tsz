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
different family → widen all literals to primitive types (string, number, etc.).
**Updated** (2026-03-11): The different-family branch was originally only widening
number/boolean but preserving string/bigint. Fixed to widen ALL literal types in
the different-family case, matching tsc's behavior (e.g., `'"foo"'` → `'string'`
when compared against `'number'`). Fixes `stringLiteralsAssertionsInEqualityComparisons02`
and `stringEnumLiteralTypes3`.

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
- ~~Enum types displayed as `number` instead of the enum name~~ **FIXED** (commit `6f44652f3` — 2026-03-10)
  Fix: `widen_type_for_operator_display` in `operator_errors.rs` tries enum member→parent
  widening before `get_base_type_for_comparison` (which destroys enum identity).
  Also fixed enum member `E.A` → parent `E` widening (reordered TypeData::Enum check).
- ~~`undefined` displayed as `void` in return type contexts~~ **FIXED** (2026-03-10)
  Fix: Changed `infer_return_type_from_body_inner` in `return_type.rs` to push
  `TypeId::UNDEFINED` instead of `TypeId::VOID` when building return type unions
  for functions with mixed returning/fall-through paths.
- ~~Return type inference not preserving literal types from branches~~ **FIXED** (commit `9edc5acf2` — 2026-03-11)
  Fix: `infer_return_type_from_body_inner` in `return_type.rs` now preserves literal types
  from return expressions instead of widening them, matching tsc's inference behavior.

**Solver location:** Return type inference (`evaluate`), enum type display, literal preservation policy.

#### Sub-pattern C: Property List Ordering (13 fingerprints) — DONE

```
tsc:  "missing properties: length, pop, push, concat, and 25 more"
tsz:  "missing properties: lastIndexOf, concat, entries, indexOf, toString, and 26 more"
```

**Affected tests:** `arrayAssignmentTest1` (5 fingerprints), `noInferUnionExcessPropertyCheck1`

**Status: FIXED** (2026-03-11)

**Fix:** Four changes to correctly track and preserve property declaration order:
1. **Threshold fix**: Changed `.take(5)` to `.take(4)` in TS2740 message formatting to show
   first 4 properties + "and N more", matching tsc behavior.
2. **Lowering path**: Added forward declaration order assignment in `TypeLowering` for
   merged interface declarations. The reverse iteration needed for overload resolution
   was causing later declarations' properties to appear first.
3. **Instantiation preservation**: Fixed `instantiate_properties` which was zeroing out
   `declaration_order` during generic type instantiation (e.g., `Array<T>` → `Array<any>`).
4. **Object.prototype filtering**: Filter Object.prototype methods (`toString`,
   `toLocaleString`, `valueOf`, `hasOwnProperty`, `isPrototypeOf`,
   `propertyIsEnumerable`, `constructor`) from the TS2739/TS2740 "missing" property list.
   tsc never lists these as "missing" because every object inherits them via
   Object.prototype. Applied in both `assignability.rs` and `call_errors.rs`.
   Fixes `arrayAssignmentTest1` (5 fingerprints).

#### Sub-pattern D: Infinity/NaN Display — DONE

```
tsc:  "Type 'number' is not assignable to type 'Infinity'."
tsz:  "Type 'number' is not assignable to type 'inf'."
```

**Affected tests:** `fakeInfinity1` (1 fingerprint)

**Status: FIXED** (commit `3fb298116` — 2026-03-10)

**Fix:** Rust's `f64::INFINITY` formats as `"inf"` but TypeScript uses `"Infinity"`.
Added special-case handling in `format_literal()` in `diagnostics/format.rs` to
display `Infinity`, `-Infinity`, and `NaN` using JavaScript conventions.

#### Sub-pattern E-pre: Angle-Bracket Assertion Type Display — DONE

```
tsc:  "Conversion of type 'B' to type 'T' may be a mistake"
tsz:  "Conversion of type 'B' to type 'T>' may be a mistake"
```

**Affected tests:** `genericTypeAssertions4`, `genericTypeAssertions5`, and others with `<T>expr`

**Status: FIXED** (commit `9a5d78e04` — 2026-03-11)

**Fix:** For angle-bracket assertions `<T>expr`, the parser's type_node span includes
the closing `>`, causing `node_text()` to return `T>` instead of `T`. Added bracket-balanced
`>` stripping in `assertion_declared_type_texts` in `error_reporter/generics.rs`: only strips
a trailing `>` when brackets are unbalanced, preserving legitimate generic types like `Array<T>`.

#### Sub-pattern E: `typeof` vs Structural Display (1 fingerprint, but widespread pattern)

```
tsc:  "Type 'typeof A' is not assignable to type 'new () => A'"
tsz:  "Type '{ new (): { ; }; prototype: { ; }; }' is not assignable to type 'new () => A'"
```

**Root cause:** tsz prints the structural expansion of a class constructor type instead of using the `typeof ClassName` shorthand.

#### Sub-pattern F: Optional Parameter `| undefined` Display — DONE

```
tsc:  "Type '(p1?: string | undefined) => I1' is not assignable to type 'I1'."
tsz:  "Type '(p1?: string) => I1' is not assignable to type 'I1'."
```

**Affected tests:** `optionalParamTypeComparison`, `optionalParamAssignmentCompat`, `functionSignatureAssignmentCompat1`, `assertionFunctionWildcardImport1`

**Status: FIXED** (2026-03-10)

**Fix:** tsc includes `| undefined` in optional parameter display in error messages even
though the `?` already implies optionality. tsz was stripping `undefined` from optional
param types. Changed `format_params()` in `diagnostics/format.rs` to preserve/append
`| undefined` for optional parameters, matching tsc output.

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
- ~~False positive diagnostics in edge cases (TS5088)~~ **PARTIALLY FIXED** (commit `305d5508` — 2026-03-11)
  Fix: `declaration_type_references_cyclic_structure` in solver `traversal.rs` now only
  reports cycles as TS5088 when the traversal path goes through a conditional type's
  true/false branch, matching tsc's behavior where object/function type cycles are
  silently elided via symbol depth limits. Fixes 6 conformance tests
  (`declarationEmitInferredTypeAlias4/8`, `declarationEmitTypeAliasWithTypeParameters3/4/6`,
  `importCallExpressionDeclarationEmit1`).
- ~~False positive TS2314 in heritage clauses~~ **FIXED** (2026-03-11)
  Fix: Expanded the too-narrow allow-list (`Array|ReadonlyArray|ConcatArray`) for omitted
  type args in extends clauses. Now checks JS files (never require type args) and symbols
  with `VARIABLE` flag (constructor values infer type args from construct signatures).
  Fixes 4 conformance tests (`overrideInterfaceProperty`, `extendingCollectionsWithCheckJs`,
  `extendsTag1`, `jsdocAugments_withTypeParameter`).
- ~~False positive TS2833 in `typeof` nested qualified name resolution~~ **FIXED** (2026-03-11)
  Fix: `get_type_from_type_query` in `type_analysis/core.rs` used `get_type_of_node` for
  the left side of QualifiedName, which dispatched to namespace resolution for nested
  qualified names (e.g., `typeof l.nested.readonlyNestedType`). Added
  `resolve_typeof_qualified_value_chain` helper that recursively resolves nested
  QualifiedName nodes as value property access chains instead of namespace lookups.
  Fixes 2 conformance tests (`uniqueSymbols`, `uniqueSymbolsDeclarations`).
- ~~False positive TS18055 in `isolatedModules` enum member classification~~ **FIXED** (2026-03-11)
  Fix: `classify_isolated_enum_initializer` in `declarations.rs` had two bugs:
  (1) The `_` fallback branch called `variable_initializer_widened_kind` which returned
  `NonLiteralString` based on TYPE rather than VALUE — runtime expressions like
  `2..toFixed(0)` have string TYPE but no compile-time string VALUE, so tsc skips
  TS18055 for them. Fixed by returning `Other` for unrecognized syntax.
  (2) `classify_symbol_backed_enum_initializer` treated all symbols as cross-file in
  project mode because `cross_file_symbol_targets` contains same-file symbols too.
  Added `is_cross_file` check comparing `cross_file_idx != current_file_idx`. Same-file
  const references like `const LOCAL = 'LOCAL'` are now traced through to their
  initializer, matching tsc's `evaluateEntityNameExpression` behavior.
  Fixes 1 conformance test (`computedEnumMemberSyntacticallyString`); also improves
  3 net tests via removed false positives.
  **Known remaining issue:** `computedEnumMemberSyntacticallyString2` still fails due
  to a config coercion bug — our parser coerces `"isolatedModules": "true"` (string)
  to boolean `true`, but tsc's `convertJsonOption` returns `undefined` for type
  mismatches (TS5024) and does NOT apply the value. Fixing this properly requires
  addressing 36+ non-strict-mode conformance gaps first.
- ~~False positive TS2304 for class property and JSX attribute names~~ **FIXED** (2026-03-11)
  Fix: `direct_diagnostic_source_expression` in `error_reporter/core.rs` now returns
  `None` when the diagnostic anchor is a `PROPERTY_DECLARATION` or `JSX_ATTRIBUTE` name.
  Previously, the error reporter treated declaration name identifiers as source expressions
  and called `get_type_of_node` on them during TS2322 message formatting, which triggered
  identifier resolution → TS2304 "Cannot find name". Fixes 11 conformance tests
  (`classWithoutExplicitConstructor`, `derivedClassWithoutExplicitConstructor` (3 variants),
  `memberVariableDeclarations1`, `classPropertyErrorOnNameOnly`, `tsxAttributeErrors`,
  `tsxAttributeResolution1/9/10/14`).

- ~~False positive TS2351 in cross-file class+namespace declaration merge~~ **FIXED** (2026-03-11)
  Fix: `new_target_is_class_symbol` in `complex.rs` only checked the current binder
  when deciding whether to suppress TS2351 for circular class resolution. In multi-file
  mode with cross-file class+namespace merges (e.g., `class Point` in file A, `namespace Point`
  in file B), the identifier resolved to the namespace symbol (no CLASS flag), causing a
  false TS2351. The fix walks up enclosing namespace declarations and searches all binders'
  namespace exports for a CLASS symbol with the same name. Fixes 2 conformance tests
  (`ClassAndModuleWithSameNameAndCommonRoot`, `ClassAndModuleWithSameNameAndCommonRootES6`)
  and removes false TS2351 from `ModuleAndClassWithSameNameAndCommonRoot` (still fails
  for other reasons).

- ~~Cascading TS1128 false positives from orphaned `)` and `]` tokens~~ **FIXED** (2026-03-11)
  Fix: Widened the `is_stray_close` detection in `parse_source_file_statements`
  (`state_statements.rs`) to suppress TS1128 "Declaration or statement expected" for
  `CloseParenToken` and `CloseBracketToken` whenever ANY prior parse error exists,
  regardless of distance. These tokens can never start a valid statement, so after a
  prior error they are always artifacts of bracket-mismatch recovery. Previously the
  suppression required distance ≤ 3 characters from the last error, which was too
  restrictive when the parser advanced past intermediate tokens before reaching the
  orphaned close token. Fixes ~8 conformance tests including
  `templateStringInFunctionParameterType`, `templateStringInFunctionParameterTypeES6`,
  and several parser error recovery tests.

- ~~Missing TS2300 for cross-file TYPE_ALIAS + INTERFACE conflicts~~ **FIXED** (2026-03-11)
  Fix: `can_merge_symbols_cross_file` in `parallel.rs` was missing the TYPE_ALIAS +
  INTERFACE merge case. When `type A = {}` in one file conflicts with `interface A {}`
  in another, tsc expects the symbols to be merged first (so the checker sees the
  conflict) and then TS2300 to be emitted. Without the merge case, the symbols were
  silently invisible across files. Added bidirectional TYPE_ALIAS + INTERFACE merge.
  Fixes `noSymbolForMergeCrash.ts` at error-code level (TS2300 now emitted).
  **Known remaining issue:** Fingerprint for `namespace A {}` declaration not emitted —
  tsc reports TS2300 on ALL declarations of a poisoned merged symbol, but our pairwise
  conflict checker only reports on the directly conflicting pair.

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
- ~~Import aliases are not resolved to their original namespace paths~~ **FIXED** (2026-03-10)
  Fix: `get_symbol_qualified_name` in `type_analysis/core.rs` walks the resolved symbol's
  parent chain to build the fully qualified dotted name (e.g., `foo.bar.baz`) instead of
  using the source-text alias name (`booz`). Applied to both TS2694 call sites.
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

**FIXED** (2026-03-11): Property declaration ordering and Object.prototype filtering now
match tsc behavior. Four fixes: threshold (take 4), lowering order, instantiation
preservation, and Object.prototype method filtering in diagnostic rendering.

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
