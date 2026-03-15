# Fingerprint-Only Failures: Deep Pattern Analysis

**Date:** 2026-03-15 (updated with fix results)
**Scope:** Originally 624 conformance tests, reduced to ~399 after fixes
**Data sources:** `conformance-detail.json`, `tsc-cache-full.json`, conformance runner source

### Fixes Applied (2026-03-15)

| Fix | Commit | Impact |
|---|---|---|
| Mapped type trailing semicolons | `fix(solver): add trailing semicolon to mapped type display` | ~10 tests |
| Preserve literal types in object literal properties | `fix(checker): preserve literal types in object literal properties` | ~215 tests |
| **Total reduction** | | **624 → ~399 (36% improvement)** |

### Remaining Root Causes (~399 tests)

After fixes, the remaining fingerprint-only failures break down into:

1. **Type name resolution** (~100 tests): Class/interface types displayed as expanded object shapes instead of their declared names (e.g., `Error` shown as `{ name: string; message: string; ... }`). Root cause: `DefinitionStore` doesn't map all class/interface TypeIds back to their names.

2. **Generic type inference display** (~80 tests): The type used in error messages is the inferred/instantiated type instead of the source type (e.g., `Action<{ type: "FOO" | "bar" }>` instead of `Action<{ type: "FOO" }>`). Root cause: type evaluation produces a different result than tsc for some generic instantiations.

3. **Parser error positioning** (~50 tests): Parser recovery produces diagnostics at different line/column than tsc.

4. **Flow analysis/narrowing** (~50 tests): Optional chain narrowing and definite assignment differ from tsc.

5. **Call signature display** (~40 tests): Overloaded functions and generic constraints formatted differently.

6. **Miscellaneous** (~80 tests): Various other diagnostic span and message differences.

---

## Executive Summary

Of ~12,591 conformance tests, **624 (4.9%)** produce the correct error codes but fail at the fingerprint level. These are tests where tsz identifies the *right problem* but reports it at the *wrong location* or with *different message text* than tsc.

This represents a **significant untapped opportunity**: these tests are one step away from full parity. Unlike tests with wrong/missing error codes (which require semantic fixes), fingerprint-only failures primarily need **diagnostic span corrections** and **message text alignment**.

### Key Finding: Three Root Causes Cover ~85% of Failures

| Root Cause | Tests Affected | % of 624 | Primary Fix Location |
|---|---|---|---|
| Assignability message formatting | ~245 | 39% | Solver diagnostics + error reporter |
| Parser error positioning | ~116 | 19% | Parser error recovery spans |
| Flow analysis diagnostic spans | ~87 | 14% | Checker flow analysis + TS2564 spans |
| Property/name resolution spans | ~96 | 15% | Checker property access + name resolution |
| Other (comparison, unused, etc.) | ~80 | 13% | Various |

---

## What Is a "Fingerprint"?

A diagnostic fingerprint is a 5-tuple: `(code, file, line, column, message_key)`. The conformance runner compares these between tsc and tsz using set difference. A test passes at fingerprint level only when every tsc fingerprint has an exact match in tsz output and vice versa.

**Source:** `crates/conformance/src/tsc_results.rs:43-49`

```
DiagnosticFingerprint { code, file, line, column, message_key }
```

The message_key is normalized (whitespace-collapsed, path segments stripped) to reduce false negatives, but type names and structural message content must still match exactly.

---

## Root Cause 1: Assignability Message Formatting (~245 tests, 39%)

### The Pattern

**240 tests** involve TS2322 ("Type 'X' is not assignable to type 'Y'"). This is by far the dominant fingerprint failure pattern. The error code is correct—tsz detects the same type incompatibility as tsc—but the *message text* differs because tsz formats type names differently.

### Error Code Distribution (assignability family)

| Code | Tests | Total Fingerprints | Description |
|---|---|---|---|
| TS2322 | 240 | 1,012 | Type not assignable |
| TS2345 | 68 | 302 | Argument not assignable to parameter |
| TS2741 | 22 | — | Property missing in type |
| TS2769 | 17 | — | No overload matches this call |
| TS2353 | 20 | 71 | Excess property in object literal |
| TS2352 | 12 | — | Conversion may be a mistake |
| TS2430 | 10 | 115 | Class incorrectly implements interface |
| TS2411 | 8 | — | Property type incompatible with index |

### Message Pattern Breakdown (TS2322 only, 1,012 fingerprints)

| Pattern | Count | Example |
|---|---|---|
| Simple primitive types | 486 | `Type 'number' is not assignable to type 'string'` |
| Generic type parameters | 192 | `Type 'T' is not assignable to type 'object'` |
| Nullish types | 132 | `Type 'boolean \| undefined' is not assignable to type 'boolean'` |
| Union/intersection types | 96 | `Type 'string \| number' is not assignable to type 'string'` |
| Object literal types | 75 | `Type '{ fooProp: "frizzlebizzle"; } & Bar' is not assignable...` |
| Function types | 11 | `Type '(x: number) => number[]' is not assignable to type '<T>...'` |

### Confirmed Root Causes (from verbose test runs)

Verbose conformance runs on ~16 sample tests confirmed **6 specific type-printer bugs** that account for the vast majority of TS2322/TS2345 fingerprint failures:

#### Bug 1: Literal Types Widened to Base Types in Messages

tsz prints the widened type instead of preserving the literal type in error messages.

```
errorMessagesIntersectionTypes02.ts:
  tsc:  Type '{ fooProp: "frizzlebizzle"; } & Bar' is not assignable...
  tsz:  Type '{ fooProp: string; } & Bar' is not assignable...
                         ^^^^^^ should be "frizzlebizzle"

excessPropertyCheckWithUnions.ts:
  tsc:  Type '{ tag: "D"; }' is not assignable to type 'ADT'
  tsz:  Type '{ tag: string; }' is not assignable to type 'ADT'
                    ^^^^^^ should be "D"
```

**Impact:** Affects every test where literal types appear in error messages. This is likely the single highest-volume bug.

#### Bug 2: `boolean` Not Narrowed to `false` in Type Guard Branches

When a type guard narrows `boolean` to `false` in the failing branch, tsz still prints `boolean`.

```
typeGuardOfFormIsType.ts (EVERY mismatch in this test):
  tsc:  Type 'string | false' is not assignable to type 'string'
  tsz:  Type 'string | boolean' is not assignable to type 'string'
                       ^^^^^^^ should be false
```

**Impact:** Affects all type guard tests (15+ tests at 4.81x over-representation).

#### Bug 3: `Array<T>` Printed Instead of `T[]` Shorthand

tsz uses the generic form `Array<T>` where tsc uses the shorthand `T[]`.

```
assignmentCompatWithCallSignatures3.ts:
  tsc:  Type '(x: Base[], y: Derived2[]) => Derived[]' is not assignable...
  tsz:  Type '(x: Array<Base>, y: Array<Derived2>) => Array<Derived>' is not assignable...
```

**Impact:** Affects any test with array types in error messages.

#### Bug 4: Missing Semicolons in Mapped/Object Type Display

tsz omits trailing semicolons inside mapped and object type literals.

```
mappedTypeErrors.ts:
  tsc:  { [P in keyof T]: T[P]; }
  tsz:  { [P in keyof T]: T[P] }
                                ^ missing ;
```

**Impact:** Affects mapped type tests (10+ tests).

#### Bug 5: Alias Name Shown Instead of Resolved Constituent Type

In discriminated union excess property checks, tsz prints the union alias name where tsc prints the specific resolved constituent.

```
excessPropertyCheckWithUnions.ts:
  tsc:  ...does not exist in type '{ tag: "A"; a1: string; }'
  tsz:  ...does not exist in type 'ADT'
```

#### Bug 6: Conditional/Mapped Type Alias Resolution Depth Mismatch

tsz resolves type aliases to different depths than tsc in diagnostic messages.

```
conditionalTypes1.ts:
  tsc:  Type 'NonFunctionProperties<T>'    →  tsz:  Type 'T'
  tsc:  Type 'DeepReadonlyObject<Part>'    →  tsz:  Type 'DeepReadonly<Part>'
  tsc:  Type 'DeepReadonlyArray<Part>'     →  tsz:  Type 'DeepReadonlyArray'  (drops generic params!)
  tsc:  Type 'T[keyof T] | undefined'     →  tsz:  Type 'Partial<T>[keyof T]'
```

### Additional Issues Found

- **Destructuring binding element names wrong in TS7031**: tsz reports the wrong identifier name for binding elements (e.g., `'number'` instead of `'x'`)
- **Catch block scoping (TS2300)**: tsz over-reports duplicate identifiers in catch blocks — it lacks the special scoping rule that `catch` variables shadow outer variables without duplicate error
- **Super-before-this flow**: `checkSuperCallBeforeThisAccess.ts` has 9 missing TS17009/17011 fingerprints — tsz doesn't detect `this`/`super` access before `super()` call in all control-flow paths

### Fix Strategy

- **Highest priority:** Fix the type printer to preserve literal types (Bug 1) and narrow `boolean` to `false` (Bug 2). These two bugs alone likely account for 150+ tests.
- **Second priority:** Add `T[]` shorthand for array types (Bug 3) and semicolons in mapped types (Bug 4). Another ~30-50 tests.
- **Third priority:** Fix alias resolution depth (Bug 5/6) — requires careful alignment with tsc's `typeToString` logic for when to show an alias vs its expansion.
- **Leverage:** 112 tests have TS2322 as their *only* error code, making them ideal for regression testing type printer fixes.

---

## Root Cause 2: Parser Error Positioning (~116 tests, 19%)

### The Pattern

**116 tests** involve parser-level diagnostics (TS1xxx codes). These tests have correct error detection but wrong line/column in the diagnostic.

### Code Distribution

| Code | Tests | Description |
|---|---|---|
| TS1005 | 51 | `'X' expected` (missing token) |
| TS1109 | 13 | `Expression expected` |
| TS1128 | 7 | `Declaration or statement expected` |
| TS1434 | 6 | Various parser errors |
| TS1003 | — | `Identifier expected` |

### TS1005 Message Variants (51 tests)

| Message | Count |
|---|---|
| `',' expected.` | 16 |
| `';' expected.` | 10 |
| `':' expected.` | 7 |
| `'(' expected.` | 5 |
| `'=' expected.` | 3 |
| `'=>' expected.` | 2 |

### Confirmed Root Causes (from verbose test runs)

1. **Off-by-one line errors in error recovery**: Parser error recovery produces diagnostics at adjacent lines.

```
enumErrors.ts:
  tsc expects TS1357 at line 48, col 18
  tsz emits  TS1357 at line 49, col 24
  (Off-by-one line for malformed enum members with semicolons/colons)
```

2. **Extra cascade diagnostics**: Parser recovery path resolves different identifiers, producing extra TS2304 errors.

```
parserClassDeclaration1.ts:
  tsc expects: TS2304 at test.ts:1:17 "Cannot find name 'A'"
  tsz emits:   TS2304 at test.ts:1:27 "Cannot find name 'B'" (extra — wrong identifier)
  (In "extends A extends B", tsz resolves B instead of A)
```

3. **Missing diagnostics at shifted positions**: Error codes match in count but diagnostics are at wrong locations.

```
commonMissingSemicolons.ts:
  4 missing fingerprints at lines 71, 72, 76, 79
  0 extra fingerprints
  (Same codes emitted but at different line/column positions)

letAsIdentifierInStrictMode.ts:
  tsc expects TS1212 at line 4:1 — tsz places it elsewhere
```

### Fix Strategy

- **Primary:** Compare parser error recovery spans between tsz and tsc for the TS1005 family. Focus on where the parser creates the diagnostic: is it at `scanner.pos` (current position), `scanner.token_start` (start of current token), or the end of the previous token?
- **Quick wins:** 25 tests have TS1005 as their only error code — fix the span for each message variant and a batch will pass.

---

## Root Cause 3: Flow Analysis Diagnostic Spans (~87 tests, 14%)

### The Pattern

Tests involving flow-sensitive diagnostics where the error is detected correctly but reported at the wrong source location.

### Code Distribution

| Code | Tests | Total Fingerprints | Description |
|---|---|---|---|
| TS2564 | 62 | 182 | Property not definitely assigned in constructor |
| TS2454 | 29 | 250 | Variable used before being assigned |
| TS18048 | — | 57 | Value is possibly 'undefined' |
| TS2722 | — | — | Cannot invoke possibly 'undefined' |

### TS2564 Analysis (62 tests)

TS2564 errors point to the **property declaration** that lacks initialization. The position must match the exact property name token. Common message variants:

| Message | Count |
|---|---|
| `Property 'foo' has no initializer...` | 20 |
| `Property 'x' has no initializer...` | 10 |
| `Property 'p' has no initializer...` | 5 |
| `Property 'id' has no initializer...` | 5 |
| `Property 'name' has no initializer...` | 4 |

**Key insight:** TS2564 never appears alone in fingerprint-only failures (0 tests with TS2564 as the sole code). It always co-occurs with other codes, most commonly TS2322 (24 tests). This suggests TS2564 positioning may be correct but the co-occurring TS2322 messages are what fail.

### TS2454 Analysis (29 tests)

TS2454 points to the **use site** of a variable that might not be assigned. Flow analysis must track definite assignment through all control flow paths.

Co-occurrence: TS2454 appears with TS2322 in 19 tests, suggesting that many of these failures are actually caused by the TS2322 message formatting issue (Root Cause 1) rather than TS2454 positioning.

### Confirmed: Optional Chain Narrowing Divergence

Verbose run of `controlFlowOptionalChain.ts` (61 expected fingerprints) revealed:

```
12 missing fingerprints (tsc expects, tsz doesn't emit):
  Lines: 15, 106, 113, 359, 368, 371, 380, 443, 452, 455, 464, 502
  (tsz narrows TOO aggressively — removes 'undefined' where tsc keeps it)

5 extra fingerprints (tsz emits, tsc doesn't expect):
  Lines: 97, 287, 510, 513, 603
  (tsz narrows TOO LITTLE — keeps 'undefined' where tsc narrows it away)
```

This is a **bidirectional narrowing accuracy issue** for optional chains: tsz both over-narrows and under-narrows at different branch points. The error codes match because the total count of each code happens to be the same, but the specific locations differ.

### Fix Strategy

- **Primary:** Many TS2564/TS2454 tests actually fail because of co-occurring TS2322 messages. Fixing Root Cause 1 (type printer) will resolve many as a side effect.
- **TS2564:** Verify diagnostic span points to the property *name* token.
- **Optional chains:** The narrowing logic for optional chains needs systematic comparison with tsc's control flow analysis, particularly around which branches preserve `undefined` possibility.

---

## Root Cause 4: Property/Name Resolution Spans (~96 tests, 15%)

### The Pattern

| Code | Tests | Description |
|---|---|---|
| TS2339 | 50 | Property does not exist on type |
| TS2304 | 37 | Cannot find name |
| TS2307 | 10 | Cannot find module |

### TS2339 Analysis

"Property 'X' does not exist on type 'Y'" — the diagnostic must point to the property access expression. The position depends on:
- Which AST node the diagnostic attaches to (the property name vs the dot expression)
- The type name in the message (how the narrowed type is printed after type guards)

**19 tests** have TS2339 without any assignability code, suggesting pure positioning issues. **31 tests** have TS2339 with assignability codes, where message formatting is also a factor.

### TS2304 Analysis

"Cannot find name 'X'" — 37 tests. Heavy-hitter tests:
- `parserRealSource8.ts`: 121 fingerprints (massive parser test)
- `parserGenericsInTypeContexts2.ts`: 51 fingerprints
- `parserGenericsInVariableDeclaration1.ts`: 18 fingerprints

These are primarily parser tests where error recovery leads to "cannot find name" cascades. The cascade positions depend on exactly how the parser recovers from the initial error.

### Fix Strategy

- **TS2339:** Ensure the diagnostic span covers only the property name identifier, not the full member expression.
- **TS2304:** For parser-cascade tests, fixing parser error recovery (Root Cause 2) will likely fix the cascade positions.

---

## Root Cause 5: Arithmetic/Comparison Operators (~23 tests, 4%)

### The Pattern

| Code | Fingerprints | Description |
|---|---|---|
| TS2362 | 381 | Left-hand side of arithmetic must be number |
| TS2363 | 361 | Right-hand side of arithmetic must be number |
| TS2365 | 83 | Operator cannot be applied to types |
| TS2367 | 121 | No overlap between types |

**Concentrated in 5 tests** with extremely high fingerprint counts:
- `arithmeticOperatorWithInvalidOperands.ts`: **554 fingerprints** (single test!)
- `compoundArithmeticAssignmentWithInvalidOperands.ts`: 60 fingerprints
- `exponentiationOperatorWithInvalidOperands.ts`: 56 fingerprints

### Likely Root Cause

The message text includes the types involved: `"The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type."` If tsz's message template slightly differs (e.g., different wording, missing 'bigint'), all 554 fingerprints in the big test will fail.

### Fix Strategy

- **Verify exact message text** for TS2362/TS2363 matches tsc. A single template fix could resolve ~742 fingerprints across 5 tests.

---

## Category Distribution

### By Test Directory

| Directory | Count | % |
|---|---|---|
| `compiler/` | 260 | 42% |
| `conformance/types/` | 114 | 18% |
| `conformance/expressions/` | 47 | 8% |
| `conformance/parser/` | 46 | 7% |
| `conformance/es6/` | 37 | 6% |
| `conformance/jsx/` | 25 | 4% |
| `conformance/classes/` | 24 | 4% |
| `conformance/jsdoc/` | 13 | 2% |
| `conformance/controlFlow/` | 7 | 1% |
| Other | 50 | 8% |

### By Fingerprint Count Per Test

| Fingerprints | Tests | Description |
|---|---|---|
| 1 | 122 | Simplest to diagnose — single diagnostic to fix |
| 2–5 | 272 | Small test — fix a few spans/messages |
| 6–20 | 178 | Medium test — likely systematic issue |
| >20 | 51 | Large test — one root cause affects many diagnostics |

---

## Top Code-Set Signatures

The most common combinations of error codes in fingerprint-only tests:

| Code Set | Tests | Notes |
|---|---|---|
| TS2322 alone | 112 | Pure assignability message issue |
| TS1005 alone | 25 | Pure parser positioning |
| TS2345 alone | 20 | Pure argument assignability |
| TS2304 alone | 12 | Pure name resolution |
| TS2322 + TS2564 | 12 | Assignability + property init |
| TS2339 alone | 10 | Pure property access |
| TS2322 + TS2454 | 9 | Assignability + definite assignment |
| TS2322 + TS2345 | 9 | Two assignability codes |
| TS1005 + TS1109 | 7 | Parser recovery cascade |
| TS7053 alone | 7 | Implicit any element access |

---

## Recommended Fix Prioritization

### Tier 1: Highest ROI (fix ~200 tests)

1. **Audit TS2322 message text formatting** — Fix how types are rendered in "Type 'X' is not assignable to type 'Y'" messages. This likely involves the type printer/serializer. Addressing this single root cause could fix ~112 single-code tests and contribute to fixing ~240 total.

2. **Fix TS2362/TS2363 message template** — If the arithmetic error message template has a wording difference, fixing it resolves ~742 fingerprints across 5 tests for minimal effort.

### Tier 2: Medium ROI (fix ~100 tests)

3. **Parser error recovery spans** — Systematically audit where TS1005 diagnostics point. The 25 single-code TS1005 tests are ideal for regression testing. Focus on the `;`/`,`/`:` expected variants first (33 tests).

4. **TS2345 message formatting** — Same root cause as TS2322 but for function arguments. 20 single-code tests.

### Tier 3: Targeted fixes (fix ~50 tests)

5. **TS2339 diagnostic span** — Ensure property access errors point to the property name token.
6. **TS2564 diagnostic span** — Ensure property-not-initialized errors point to the property name.
7. **TS2304 spans in parser recovery contexts** — Cascade positions after parser errors.

### Tier 4: Long tail

8. TS7053 message text (7 tests)
9. TS2430 message text (10 tests)
10. TS6133 unused declaration spans (4 tests)
11. TS2683 'this' implicit any (6 tests)
12. JSX-specific diagnostics (25 tests)

---

## Over-Represented Test Categories

Type-system-heavy categories fail at fingerprint level at **3-6x** the baseline rate:

| Category | Over-representation | Root Cause |
|---|---|---|
| `conformance/types/intersection/` | 5.89x | Type printer alias resolution |
| `conformance/expressions/typeGuards/` | 4.81x | `boolean` not narrowed to `false` |
| `conformance/types/tuple/` | 4.76x | Array/tuple type display |
| `conformance/types/literal/` | 4.59x | Literal type widening in messages |
| `conformance/types/spread/` | 4.04x | Object spread type display |
| `conformance/types/union/` | 3.23x | Union member ordering/display |
| `conformance/types/typeRelationships/` | 3.22x | Assignability message formatting |
| `conformance/es6/destructuring/` | 2.61x | Binding element names + type display |
| `conformance/jsx/` | 2.39x | Component type validation messages |

This confirms the type printer is the dominant root cause — the categories with highest over-representation are exactly those requiring complex type-to-string serialization.

## Methodology Notes

- Statistical analysis based on **offline snapshot data** (`conformance-detail.json`, `tsc-cache-full.json`)
- **Validated by verbose test runs** on 16 sample tests spanning all major categories, confirming 6 specific type-printer bugs and 3 parser/flow issues
- "Fingerprint-only" = error codes match exactly (`missing: [], extra: []`) but the test still fails because position or message text differs in at least one diagnostic
- To validate specific hypotheses, use `./scripts/conformance/conformance.sh run --filter "testname" --verbose`

---

## Appendix: Data Queries

```bash
# Re-run this analysis
python3 scripts/conformance/query-conformance.py

# Get fingerprint-only tests
python3 -c "
import json
with open('scripts/conformance/conformance-detail.json') as f:
    d = json.load(f)
for t, data in sorted(d['failures'].items()):
    if not data.get('m') and not data.get('x'):
        print(t)
"

# Check tsc expectations for a specific test
python3 -c "
import json
with open('scripts/conformance/tsc-cache-full.json') as f:
    c = json.load(f)
test = 'compiler/errorMessagesIntersectionTypes02.ts'
for fp in c[test]['diagnostic_fingerprints']:
    print(f\"TS{fp['code']} {fp['file']}:{fp['line']}:{fp['column']} {fp['message_key']}\")
"

# Run verbose comparison for a specific test
./scripts/conformance/conformance.sh run --filter "errorMessagesIntersectionTypes02" --verbose
```
