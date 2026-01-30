# Conformance Improvement Ideas (Jan 30, 2026)

## Executive Summary

**Current Pass Rate: 35.7% (4,415/12,379)** - Up from 32.2% but with concerning regressions

### Key Changes Since Last Update:
- ✅ **Timeouts improved**: 114 (down from 321, -64%)
- ✅ **TS2304 improved**: 3,447x (down from 4,994x, -31%)
- ✅ **TS1005 improved**: 2,678x (down from 3,141x, -15%)
- ✅ **TS2339 improved**: 1,489x (down from 1,974x, -25%)
- ✅ **TS2307 improved**: 1,129x (down from 1,841x, -39%)
- 🔴 **TS2322 regressed**: 11,598x (up from 2,606x, +345% - CRITICAL)
- 🔴 **TS2695 regressed**: 763x (was eliminated, now back)
- 🔴 **Worker crashes**: 115 crashes/respawns (stability issue)
- 🔴 **Many categories regressed**: compiler (39.0% vs 45.6%), jsdoc (30.8% vs 54.0%), symbols (14.7% vs 79.1%)

### Immediate Priority:
1. **URGENT**: Investigate and fix TS2322 regression (blocking ~1,500–2,000 tests)
2. **URGENT**: Fix worker crash/respawn pattern (115 crashes)
3. **URGENT**: Re-fix TS2695 regression (763x extra errors)

---

## Current State

**Pass Rate: 35.7% (4,415/12,379 tests)**
Latest test run (Jan 30, 2026):
- **Passed**: 4,415 tests (includes 25 where TSC crashes but tsz succeeds)
- **Failed**: 7,849 tests
- **Timeout**: 114 tests (down from 321 - significant improvement!)
- **Worker crashes**: 115 crashes/respawns (needs investigation)
- **Both TSC and tsz crash**: 1 test

**Top Extra Errors (tsz emits, tsc does not):**
- TS2322: 11,598x (Type not assignable - major regression)
- TS2304: 3,447x (Cannot find name - improved from 4,994x)
- TS1005: 2,678x (Parser: ',' expected - improved from 3,141x)
- TS2339: 1,489x (Property does not exist - improved from 1,974x)
- TS7010: 1,257x (Function lacking return type)
- TS2749: 1,224x (Refers to type, used as value)
- TS2307: 1,129x (Cannot find module - improved from 1,841x)
- TS2695: 763x (Comma operator side-effect - regressed, was eliminated)

**Top Missing Errors (tsc emits, tsz does not):**
- TS2488: 1,576x (Must have [Symbol.iterator]() method)
- TS2585: 923x (Type instantiation is excessively deep)
- TS2322: 892x (Type not assignable - some cases)
- TS2318: 786x (Cannot find global type)
- TS18050: 679x (Value cannot be used)
- TS2300: 631x (Duplicate identifier)
- TS2339: 614x (Property does not exist - some cases)
- TS2304: 550x (Cannot find name - some cases)

**Completed Fixes (Jan 30):**
- **TS2695**: Fixed false positives by removing tagged templates from side-effect-free list
  - Root cause: Tagged templates are function calls with side effects
  - Fix: Removed TAGGED_TEMPLATE_EXPRESSION from is_side_effect_free
  - Impact: ~470 false positives eliminated (but regressed to 763x - needs re-investigation)

- **TS2362**: Fixed false positives for numeric enums
  - Root cause: Enum types (TypeKey::Ref) rejected as invalid for arithmetic
  - Fix: Treat Ref types conservatively as number-like/bigint-like
  - Impact: Reduced from 448x to 344x (~23% improvement)
  - Note: Conservative approach - may allow some string enum arithmetic but reduces false positives significantly

- **TS2304**: Fixed lib loading for conformance tests
  - Root cause: tsz_server didn't fall back to embedded libs when disk files unavailable
  - Fix: Added embedded lib fallback in load_lib_recursive
  - Impact: Reduced from 4,994x to 3,447x (~31% improvement, but still high)

- **TSC crashes**: Fixed multi-file tests with relative imports
  - Root cause: File names not resolved relative to test file path
  - Fix: Added rootFilePath parameter, resolved to absolute paths
  - Impact: 25 tests now pass where TSC crashes but tsz succeeds

**Critical Issues:**
- **TS2322 explosion**: Increased from 2,606x to 11,598x - major regression in type assignment checking
- **Worker crashes**: 115 crashes/respawns indicate stability issues
- **TS2695 regression**: Back to 763x after being eliminated - fix may have been reverted or incomplete

---

## 1. Highest-Impact: Reduce False Positive Errors

The single biggest conformance blocker is **tsz emitting errors that tsc does not**. These false positives cause thousands of tests to fail. Fixing even one of these could flip hundreds of tests from FAIL to PASS.

### Top Extra Errors (tsz emits, tsc does not)

| Error Code | Count | Description | Estimated Impact | Trend |
|------------|-------|-------------|-----------------|-------|
| **TS2322** | **11,598x** | Type 'X' is not assignable to type 'Y' | ~1,500–2,000 tests | 🔴 **Major regression** (was 2,606x) |
| **TS2304** | **3,447x** | Cannot find name 'X' | ~500–800 tests | 🟢 Improved (was 4,994x, -31%) |
| **TS1005** | **2,678x** | ',' expected (parser) | ~400–600 tests | 🟢 Improved (was 3,141x, -15%) |
| **TS2339** | **1,489x** | Property 'X' does not exist on type 'Y' | ~200–400 tests | 🟢 Improved (was 1,974x, -25%) |
| **TS7010** | **1,257x** | Function lacking return type in .d.ts | ~200–300 tests | ➡️ Stable (was 1,240x) |
| **TS2749** | **1,224x** | 'X' refers to a type, but is being used as a value | ~200–300 tests | ➡️ Stable (was 1,192x) |
| **TS2307** | **1,129x** | Cannot find module 'X' | ~200–300 tests | 🟢 Improved (was 1,841x, -39%) |
| **TS2695** | **763x** | Comma operator left side unused | ~100–200 tests | 🔴 **Regressed** (was eliminated, now back) |

### Actionable Ideas

#### 1a. TS2304 "Cannot find name" (4,994x) — LOW-HANGING FRUIT

This is the #1 false positive. tsz reports it can't find names that tsc resolves fine. Root causes likely include:
- **Global type/value resolution gaps**: Names from lib.d.ts (`console`, `Promise`, `Symbol`, `Array`, etc.) not found when `@target` or `@lib` directives change the available libs
- **Namespace member resolution**: Qualified names like `M.n` or `A.foo()` not resolving through namespace declarations
- **Cross-file name resolution**: In multi-file tests, names exported from one file not visible in another
- **Declaration merging**: Names from merged declarations (interface + namespace, class + namespace) not found

**Quick wins:**
- Audit how `@lib` and `@target` directives map to lib files — many tests fail because tsz loads the wrong set of lib files
- Check if `console` is available when target libs include `dom`
- Verify namespace-qualified name resolution works in all positions

#### 1b. TS1005 "',' expected" (3,141x) — PARSER FIX

The parser is over-reporting syntax errors. This is likely a cascade effect where one unrecognized syntax construct causes the parser to emit many downstream TS1005 errors. Key areas:
- **Import attributes / `with` syntax**: `import ... with { type: "json" }` not parsed correctly
- **`using` declarations in some contexts** may confuse the parser
- **Decorators on various targets** may produce parser errors
- **Exponentiation operator `**=`**: Some compound assignment forms may not be parsed

**Quick win:** Identify the specific syntax constructs triggering parser cascades. A single parser fix could eliminate hundreds of TS1005 errors.

#### 1c. TS2322 "Type not assignable" (2,606x)

tsz is too strict in some type assignment checks, reporting errors where tsc accepts the code. Likely causes:
- **Generic type parameter inference**: tsz may not properly instantiate type parameters during assignment checks
- **Literal type widening**: String/number literals not widened appropriately
- **Union type distribution**: Assignment checks not distributing over unions correctly
- **Tuple assignability**: Tuple/array covariance rules not matching tsc

#### 1d. TS2339 "Property does not exist" (1,974x)

tsz can't find properties that tsc can. Previous investigation (see `ts2339_investigation.md`) identified architectural gaps with hardcoded member lists instead of lib.d.ts lookup. Additional causes:
- **Index signature access**: Properties accessed on types with index signatures
- **Prototype chain walking**: Methods inherited from base classes/interfaces
- **Type narrowing after guards**: Properties available after type guard refinement

#### 1e. TS2307 "Cannot find module" (1,841x)

Module resolution is emitting false errors. The full module resolution system needs:
- **Relative import resolution** in multi-file tests
- **Non-relative module resolution** (node_modules lookup)
- **Path mapping** (`@paths`, `@baseUrl` directives)
- **Module resolution mode** (`node16`, `nodenext`, `bundler`)

#### 1f. TS2362 "Left-hand side of arithmetic" (1,331x)

tsz incorrectly reports that the left side of arithmetic expressions must be `any`, `number`, `bigint`, or enum type. This suggests:
- The arithmetic operator type checker is not recognizing enum types as valid
- OR it's triggering on code where the type should be widened to number
- May be a cascade from TS2304 (name not found → unknown type → arithmetic check fails)

#### 1g. TS7010 "Function lacking return type" (1,240x)

This is a declaration emit diagnostic. tsz may be:
- Emitting declaration-mode diagnostics when not requested
- Or reporting this for function declarations that have inferred return types

#### 1h. TS2749 "Refers to type, used as value" (1,192x)

tsz confuses types and values. This indicates:
- Type-only imports not distinguished from value imports
- Enum values confused with types
- Class references used as both types and values not handled

---

## 2. Missing Error Codes (tsz should emit but doesn't)

These represent missing type-checking features. Each one is a specific check that needs to be implemented. Ordered by estimated impact across the test suite.

### High-Impact Missing Checks

| Error Code | Count | Description | Where It Matters | Difficulty |
|------------|-------|-------------|-----------------|------------|
| **TS2488** | **1,576x** | Must have `[Symbol.iterator]()` method | Destructuring, spread, for-of | Medium |
| **TS2585** | **923x** | Type instantiation is excessively deep | Generic type resolution | Hard |
| **TS2322** | **892x** | Type not assignable (some cases) | Assignment compat, generics | Hard |
| **TS2318** | **786x** | Cannot find global type | Global type resolution | Medium |
| **TS18050** | **679x** | Value 'null'/'undefined' cannot be used | Exponentiation, strict | **Easy** |
| **TS2300** | **631x** | Duplicate identifier | Declaration merging, scoping | Medium |
| **TS2339** | **614x** | Property does not exist (some cases) | Property lookup edge cases | Medium |
| **TS2304** | **550x** | Cannot find name (some cases) | Name resolution edge cases | Medium |
| **TS2695** | (see extra errors) | Comma operator left side unused | All operator tests | **Easy** |
| **TS2872/TS2873** | | Expression always truthy/falsy | Conditional, control flow | **Easy** |
| **TS17004** | | Cannot use JSX without `--jsx` flag | All JSX tests without flag | **Easy** |
| **TS2341** | | Property is private | Accessibility tests | Medium |
| **TS2365** | | Operator '+' cannot be applied | Addition operator tests | Medium |
| **TS2506** | | Referenced in own base expression | Circular class hierarchies | Medium |
| **TS2515/TS2654** | | Non-abstract class missing implementations | Abstract class tests | Medium |
| **TS5076** | | `??` and `\|\|` cannot be mixed | Nullish coalescing tests | **Easy** |
| **TS1345** | | Void expression tested for truthiness | Strict mode tests | **Easy** |
| **TS1121** | | Octal literals not allowed | Strict mode tests | **Easy** |
| **TS2584** | | Cannot find name 'console' | Tests targeting ES5 without DOM | **Easy** |
| **TS2468** | | Cannot find global value 'Promise' | Dynamic import, async (ES5 target) | Medium |
| **TS2712** | | Dynamic import needs Promise (ES5) | Dynamic import tests | Medium |
| **TS2362/TS2363** | | Arithmetic operand type checking | Exponentiation tests | Medium |
| **TS1228** | | Type predicate only in return position | Type guard tests | **Easy** |
| **TS2303** | | Circular import alias | Type-only circular imports | Medium |
| **TS2456** | | Circular type alias | Type-only circular types | Medium |

### Easy Wins (parser/checker additions)

These are straightforward checks that don't require deep type system changes:

1. **TS2695 — Comma operator side-effect check**: Warn when left side of comma has no side effects. This is a simple expression analysis. Would fix ~20+ tests across multiple categories.

2. **TS2872/TS2873 — Always truthy/falsy**: Check if a conditional expression's condition is a type that's always truthy (non-empty string literal, object literal) or always falsy (empty string `""`). Would fix ~30+ tests in conditional operator category.

3. **TS17004 — JSX flag check**: Emit error when JSX syntax is used but `--jsx` flag is not provided. Simple flag check. Would fix many JSX tests.

4. **TS5076 — Mixed `??` with `||`/`&&`**: Parser-level check that `??` cannot be mixed with `||` or `&&` without parentheses. Would fix nullish coalescing tests.

5. **TS1345 — Void truthiness check**: Emit error when an expression of type `void` is used in a boolean context. Would fix strict mode tests.

6. **TS1121 — Octal literal check**: Emit error for legacy octal literals (`01`) in strict mode / modules. Would fix strict mode tests.

7. **TS18050 — Null/undefined not usable here**: Check that `null` and `undefined` are not used in contexts that require values (e.g., exponentiation operands).

8. **TS1228 — Type predicate position**: Emit error when type predicate (`x is T`) appears outside of return type position.

---

## 3. Timeouts (114 tests) 🟢 **Improved from 321**

114 tests time out (down from 321 - 64% reduction!), indicating infinite loops or excessive computation.

### Known Timeout Causes

1. **Circular class inheritance** (4+ tests): `classExtendsItself.ts`, `classExtendsItselfIndirectly*.ts`
   - Root cause: Recursive resolution without cycle detection
   - Previous attempts added caching but the issue persists
   - Needs architectural fix: detect cycles at bind time before type resolution

2. **Recursive type evaluation**: Some complex mapped types, conditional types, or template literal types cause stack overflow in the TypeScript compiler itself (TSC also crashes on these)

3. **Control flow tests**: Several control flow tests timeout:
   - `controlFlowOptionalChain.ts`
   - `dependentDestructuredVariables.ts`
   - `symbolType3.ts`

4. **~100 other timeouts**: These need investigation — may be a mix of:
   - Infinite loops in type resolution for complex types
   - Performance issues in constraint solving
   - Missing memoization in recursive type operations

### Action Items
- Add timeout/depth limits to recursive type operations
- Implement cycle detection at the binder level for class hierarchies
- Profile the top timeout tests to identify hot paths
- Investigate worker crash/respawn pattern (115 crashes may be related to timeouts)

---

## 4. Category-Level Analysis

### Already at 100% (maintain these)

| Category | Tests | Notes |
|----------|-------|-------|
| awaitBinaryExpression | 15/15 | |
| awaitCallExpression | 24/24 | |
| defaultParameters | 8/8 | |
| functionExpressions | 2/2 | |
| unicodeExtendedEscapes | 64/64 | |
| accessors | 5/5 | |
| memberAccessorDeclarations | 5/5 | |
| noCatchBinding | 1/1 | |
| codeGeneration | 4/4 | |
| objectTypeLiteral | 2/2 | |
| emptyTuples | 2/2 | |
| apparentType | 2/2 | |
| classThisReference | 2/2 | |
| LabeledStatements | 4/4 | |
| ReturnStatements | 4/4 | |
| MemberFunctionDeclarations | 6/6 | |
| MemberVariableDeclarations | 5/5 | |
| MethodSignatures | 12/12 | |
| PropertyAssignments | 4/4 | |
| IndexMemberDeclarations | 10/10 | |
| VariableLists | 6/6 | |
| ecmascript3 | 7/7 | |
| identifiers | 1/1 | |
| ifDoWhileStatements | 1/1 | |
| withStatements | 1/1 | |
| scanner | 1/1 | |
| AutomaticSemicolonInsertion | 1/1 | |
| CatchClauses | 1/1 | |

### Near 100% — Quick Wins to Complete

| Category | Pass Rate | Failing | Likely Fix |
|----------|-----------|---------|------------|
| ArrayLiteralExpressions | 17/18 (94.4%) | 1 | Tuple assignment check (TS2322) |
| ContinueStatements | 14/15 (93.3%) | 1 | Investigate specific failure |
| BreakStatements | 11/12 (91.7%) | 1 | Investigate specific failure |
| fields | 11/12 (91.7%) | 1 | TS2584 console not found (lib issue) |
| arrowFunction | 42/47 (89.4%) | 5 | Parser/checker issues |
| jsxs | 8/9 (88.9%) | 1 | JSX-specific check |
| restParameters | 8/9 (88.9%) | 1 | Spread/rest type check |
| Protected | 8/9 (88.9%) | 1 | Accessibility modifier check |
| continueStatements | 8/9 (88.9%) | 1 | Investigate specific failure |
| stringLiteral | 30/34 (88.2%) | 4 | String literal type checks |
| breakStatements | 9/10 (90.0%) | 1 | Investigate specific failure |
| StrictMode | 17/20 (85.0%) | 3 | TS1210, TS1121, TS1345 checks |
| classDeclaration | 48/57 (84.2%) | 9 | Mixed issues |

### Biggest Opportunities (many tests, moderate rate)

| Category | Tests | Pass Rate | Tests to Gain | Notes |
|----------|-------|-----------|---------------|-------|
| **compiler** | 6,515 | 39.0% (2,542/6,515) | ~2,540 potential | 🔴 Regressed from 45.6% |
| **jsdoc** | 250 | 30.8% (77/250) | ~173 potential | 🔴 Regressed from 54.0% |
| **jsx** | 195 | 37.9% (74/195) | ~121 potential | ➡️ Stable |
| **salsa** | 191 | 20.9% (40/191) | ~151 potential | 🔴 Regressed from 23.6% |
| **templates** | 178 | 74.2% (132/178) | ~46 potential | 🟢 Improved from 71.9% |
| **destructuring** | 147 | 27.9% (41/147) | ~106 potential | 🔴 Regressed from 42.2% |
| **computedProperties** | 142 | 43.0% (61/142) | ~81 potential | 🔴 Regressed from 79.6% |
| **privateNames** | 125 | 21.6% (27/125) | ~98 potential | 🔴 Regressed from 60.0% |
| **Symbols** | 129 | 14.7% (19/129) | ~110 potential | 🔴 Regressed from 79.1% |
| **for-ofStatements** | 114 | 11.4% (13/114) | ~101 potential | 🔴 Regressed from 60.5% |
| **yieldExpressions** | 99 | 0.0% (0/99) | ~99 potential | 🔴 Regressed from 79.8% |
| **externalModules** | 126 | 13.5% (17/126) | ~109 potential | 🔴 Regressed from 23.8% |
| **usingDeclarations** | 89 | 2.2% (2/89) | ~87 potential | 🔴 Regressed from 53.9% |
| **declarations** | 92 | 45.7% (42/92) | ~50 potential | ➡️ Stable |
| **typeGuards** | 63 | 33.3% (21/63) | ~42 potential | ➡️ Stable |
| **controlFlow** | 57 | 24.6% (14/57) | ~43 potential | ➡️ Stable |

### Completely Failing Categories (0%)

These categories have zero passing tests:

| Category | Tests | Likely Root Cause |
|----------|-------|-------------------|
| assignmentCompatibility | 4/70 (5.7%) | Deep type relationship checking |
| classExpression | 0/18 | Named evaluation / class expression scoping |
| namedEvaluation | 0/11 | ES2022+ named evaluation not implemented |
| additionOperator | 0/11 | TS2365 binary operator type checking |
| commaOperator | 0/13 | TS2695 + TS1109 missing checks |
| arithmeticOperator | 1/10 | TS2362/TS2363 operand type checking |
| binaryAndOctalIntegerLiteral | 0/7 | Numeric literal type checking |
| bitwiseNotOperator | 0/6 | Unary operator type checking |
| logicalNotOperator | 0/6 | Unary operator type checking |
| negateOperator | 0/6 | TS2695 comma side-effect (not negate itself!) |
| plusOperator | 0/6 | TS2695 comma side-effect |
| typeofOperator | 0/6 | TS2695 comma side-effect |
| newTarget | 0/5 | new.target not implemented |
| importAttributes | 0/11 | Import attributes syntax not parsed |
| moduleResolution/node | 0/73 | Node16/NodeNext resolution not implemented |
| typeOnly | 6/65 (9.2%) | Type-only import/export handling |

**Note on unary operator categories:** Many of the 0% unary operator categories (negate, plus, typeof, bitwise, logical not) fail NOT because of the unary operators themselves, but because the test files also contain comma expressions that trigger the missing **TS2695** check. Implementing TS2695 alone would fix many of these.

---

## 5. Prioritized Action Plan

### Tier 1: Critical Regressions (URGENT - blocking progress)

1. **🔴 CRITICAL: Fix TS2322 explosion** — Type assignment checking has regressed dramatically from 2,606x to 11,598x (4.4x increase). This is blocking thousands of tests. Root cause investigation needed:
   - Check recent changes to type assignment compatibility logic
   - Verify generic type parameter instantiation
   - Check literal type widening behavior
   - Review union type distribution in assignments
   - **Impact: ~1,500–2,000 tests blocked**

2. **Fix worker crash/respawn pattern** — 115 worker crashes indicate stability issues that may be causing test failures. Investigate:
   - Memory pressure from embedded lib loading
   - Stack overflow in recursive type operations
   - Panic conditions in type checking
   - **Impact: Unknown but likely significant**

3. **Fix TS2695 regression** — Was eliminated but now back to 763x. Verify fix wasn't reverted or incomplete. **Impact: ~100–200 tests.**

### Tier 2: Highest Impact / Easiest (could gain 500+ tests)

4. **Fix TS2304 false positives** — Already improved from 4,994x to 3,447x but still high. Continue auditing lib file loading with `@target`/`@lib` directives. **Impact: ~500–800 tests.**

5. **Fix TS1005 parser cascades** — Already improved from 3,141x to 2,678x. Continue identifying unrecognized syntax patterns. **Impact: ~400–600 tests.**

6. **Fix TS2307 module resolution** — Already improved from 1,841x to 1,129x. Continue ensuring multi-file tests resolve relative imports correctly. **Impact: ~200–300 tests.**

### Tier 3: Medium Impact / Medium Difficulty (could gain 200–400 tests)

7. **Fix TS2339 property resolution** — Already improved from 1,974x to 1,489x. Continue the architectural work to use lib.d.ts symbols instead of hardcoded lists. Focus on inherited members and index signatures.

8. **Fix TS2749 type/value confusion** — Ensure class references work as both types and values, and type-only imports are properly distinguished.

9. **Fix TS2362 arithmetic type checking** — Fix the false positive where tsz incorrectly rejects valid arithmetic operands (enums, union with number).

10. **Implement TS2488 iterator checking** — Check that types used in `for..of`, spread, and destructuring have `[Symbol.iterator]()`. This would fix spread, destructuring, and for-of test categories. **High priority: 1,576x missing errors.**

11. **Fix TS2585 type instantiation depth** — Handle excessively deep type instantiation (923x missing errors). May need depth limits or better memoization.

### Tier 4: Easy Missing Checks (could gain 100–200 tests)

12. **TS2695** — Comma operator unused side-effect check. Simple AST analysis. Fixes 6+ categories at 0%. (Also in extra errors - needs fixing on both sides)

13. **TS2872/TS2873** — Always truthy/falsy expression detection. Fixes conditional operator tests.

14. **TS17004** — JSX flag check. Trivial flag check. Fixes JSX tests missing `--jsx`.

15. **TS5076** — Mixed nullish/logical operator check. Parser-level.

16. **TS1345** — Void in boolean context check. Fixes strict mode tests.

17. **TS18050** — Value cannot be used check (679x missing errors). Easy check for null/undefined in invalid contexts.

18. **TS2515/TS2654** — Abstract member implementation check. Fixes class abstract keyword tests.

19. **TS2341** — Private accessibility check. Fixes class accessibility tests.

20. **TS2365** — Binary operator type checking for `+` operator. Fixes addition operator tests.

### Tier 5: Architectural (required for long-term progress)

21. **Timeout/cycle detection** — Add robust cycle detection for class hierarchies and recursive types. Would recover 114 timed-out tests (already improved from 321).

22. **Node16/NodeNext module resolution** — Required for the 73 node module resolution tests (0% pass rate).

23. **Type-only import/export tracking** — Required for 65 typeOnly tests (4.6% pass rate).

24. **Named evaluation** — ES2022+ feature, required for 11 namedEvaluation tests (0% pass rate).

25. **Import attributes parsing** — Required for 11 importAttributes tests (27.3% pass rate).

---

## 6. Estimated Path to 60%

**Current: 35.7% (4,415/12,379)**

### Critical Path (must fix regressions first):

1. **Fix TS2322 regression** — Revert to previous behavior or fix root cause
   - Current: 11,598x extra errors (was 2,606x)
   - If fixed: +1,500–2,000 tests (conservative)
   - **New pass rate: ~48–50%**

2. **Fix worker crashes** — Stabilize worker processes
   - Unknown impact but likely significant

3. **Fix TS2695 regression** — Re-apply fix or complete implementation
   - Current: 763x extra errors (was eliminated)
   - If fixed: +100–200 tests
   - **New pass rate: ~49–51%**

### Then continue with improvements:

4. **Continue TS2304 fixes** — Already improved 31%, continue
   - Current: 3,447x (was 4,994x)
   - If reduced to ~1,000x: +400–600 tests
   - **New pass rate: ~53–57%**

5. **Continue TS1005 fixes** — Already improved 15%, continue
   - Current: 2,678x (was 3,141x)
   - If reduced to ~1,000x: +300–400 tests
   - **New pass rate: ~56–60%**

6. **Continue TS2307 fixes** — Already improved 39%, continue
   - Current: 1,129x (was 1,841x)
   - If reduced to ~500x: +100–200 tests
   - **New pass rate: ~57–62%**

7. **Implement TS2488** — Iterator checking (1,576x missing errors)
   - If implemented: +200–400 tests
   - **New pass rate: ~59–66%**

**Projected: ~60–66% pass rate** after fixing critical regressions and continuing improvements.
