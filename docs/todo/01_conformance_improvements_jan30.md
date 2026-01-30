# Conformance Improvement Ideas (Jan 30, 2026)

## Current State

**Pass Rate: 46.1% (5,706 / 12,379 tests)**
- Failed: 6,352
- Timed out: 321
- Skipped: 3 (harness directives)

Progress from Jan 29: **38.0% → 46.1%** (+8.1 pp, +21% relative improvement)

---

## 1. Highest-Impact: Reduce False Positive Errors

The single biggest conformance blocker is **tsz emitting errors that tsc does not**. These false positives cause thousands of tests to fail. Fixing even one of these could flip hundreds of tests from FAIL to PASS.

### Top Extra Errors (tsz emits, tsc does not)

| Error Code | Count | Description | Estimated Impact |
|------------|-------|-------------|-----------------|
| **TS2304** | **4,994x** | Cannot find name 'X' | ~800–1,200 tests |
| **TS1005** | **3,141x** | ',' expected (parser) | ~500–800 tests |
| **TS2322** | **2,606x** | Type 'X' is not assignable to type 'Y' | ~400–600 tests |
| **TS2339** | **1,974x** | Property 'X' does not exist on type 'Y' | ~300–500 tests |
| **TS2307** | **1,841x** | Cannot find module 'X' | ~300–500 tests |
| **TS2362** | **1,331x** | Left-hand side of arithmetic must be number/bigint | ~200–300 tests |
| **TS7010** | **1,240x** | Function lacking return type in .d.ts | ~200–300 tests |
| **TS2749** | **1,192x** | 'X' refers to a type, but is being used as a value | ~200–300 tests |

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

| Error Code | Description | Where It Matters | Difficulty |
|------------|-------------|-----------------|------------|
| **TS2488** | Must have `[Symbol.iterator]()` method | Destructuring, spread, for-of | Medium |
| **TS2322** | Type not assignable (some cases) | Assignment compat, generics | Hard |
| **TS2695** | Comma operator left side unused | All operator tests | **Easy** |
| **TS2872/TS2873** | Expression always truthy/falsy | Conditional, control flow | **Easy** |
| **TS17004** | Cannot use JSX without `--jsx` flag | All JSX tests without flag | **Easy** |
| **TS2341** | Property is private | Accessibility tests | Medium |
| **TS2365** | Operator '+' cannot be applied | Addition operator tests | Medium |
| **TS2506** | Referenced in own base expression | Circular class hierarchies | Medium |
| **TS2515/TS2654** | Non-abstract class missing implementations | Abstract class tests | Medium |
| **TS5076** | `??` and `\|\|` cannot be mixed | Nullish coalescing tests | **Easy** |
| **TS1345** | Void expression tested for truthiness | Strict mode tests | **Easy** |
| **TS1121** | Octal literals not allowed | Strict mode tests | **Easy** |
| **TS2584** | Cannot find name 'console' | Tests targeting ES5 without DOM | **Easy** |
| **TS2468** | Cannot find global value 'Promise' | Dynamic import, async (ES5 target) | Medium |
| **TS2712** | Dynamic import needs Promise (ES5) | Dynamic import tests | Medium |
| **TS18050** | Value 'null'/'undefined' cannot be used | Exponentiation, strict | **Easy** |
| **TS2362/TS2363** | Arithmetic operand type checking | Exponentiation tests | Medium |
| **TS1228** | Type predicate only in return position | Type guard tests | **Easy** |
| **TS2303** | Circular import alias | Type-only circular imports | Medium |
| **TS2456** | Circular type alias | Type-only circular types | Medium |

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

## 3. Timeouts (321 tests)

321 tests time out, indicating infinite loops or excessive computation.

### Known Timeout Causes

1. **Circular class inheritance** (4+ tests): `classExtendsItself.ts`, `classExtendsItselfIndirectly*.ts`
   - Root cause: Recursive resolution without cycle detection
   - Previous attempts added caching but the issue persists
   - Needs architectural fix: detect cycles at bind time before type resolution

2. **Recursive type evaluation**: Some complex mapped types, conditional types, or template literal types cause stack overflow in the TypeScript compiler itself (TSC also crashes on these)

3. **`thisPropertyOverridesAccessors.ts`**: Specific test triggers infinite loop

4. **~316 other timeouts**: These need investigation — may be a mix of:
   - Infinite loops in type resolution for complex types
   - Performance issues in constraint solving
   - Missing memoization in recursive type operations

### Action Items
- Add timeout/depth limits to recursive type operations
- Implement cycle detection at the binder level for class hierarchies
- Profile the top timeout tests to identify hot paths

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

| Category | Tests | Pass Rate | Tests to Gain |
|----------|-------|-----------|---------------|
| **compiler** | 6,515 | 45.6% | ~3,542 potential |
| **jsdoc** | 250 | 54.0% | ~115 potential |
| **jsx** | 195 | 36.9% | ~123 potential |
| **salsa** | 191 | 23.6% | ~146 potential |
| **templates** | 178 | 71.9% | ~50 potential |
| **destructuring** | 147 | 42.2% | ~85 potential |
| **computedProperties** | 142 | 79.6% | ~29 potential |
| **privateNames** | 125 | 60.0% | ~50 potential |
| **Symbols** | 129 | 79.1% | ~27 potential |
| **for-ofStatements** | 114 | 60.5% | ~45 potential |
| **yieldExpressions** | 99 | 79.8% | ~20 potential |
| **externalModules** | 126 | 23.8% | ~96 potential |
| **usingDeclarations** | 89 | 53.9% | ~41 potential |
| **declarations** | 92 | 50.0% | ~46 potential |
| **typeGuards** | 63 | 44.4% | ~35 potential |
| **controlFlow** | 57 | 29.8% | ~40 potential |

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

### Tier 1: Highest Impact / Easiest (could gain 500+ tests)

1. **Fix TS2304 false positives** — Audit lib file loading with `@target`/`@lib` directives. Many tests fail because tsz doesn't load the right set of lib files, causing "Cannot find name" for standard globals. **Impact: ~800–1,200 tests.**

2. **Fix TS1005 parser cascades** — Identify the specific unrecognized syntax patterns that cause parser error cascades. Import attributes, exponentiation compound assignment, and decorator syntax are likely triggers. **Impact: ~500–800 tests.**

3. **Fix TS2307 module resolution** — Ensure multi-file tests resolve relative imports correctly. Many external module tests fail because tsz can't find sibling files. **Impact: ~300–500 tests.**

### Tier 2: Medium Impact / Medium Difficulty (could gain 200–400 tests)

4. **Fix TS2339 property resolution** — Continue the architectural work to use lib.d.ts symbols instead of hardcoded lists. Focus on inherited members and index signatures.

5. **Fix TS2749 type/value confusion** — Ensure class references work as both types and values, and type-only imports are properly distinguished.

6. **Fix TS2362 arithmetic type checking** — Fix the false positive where tsz incorrectly rejects valid arithmetic operands (enums, union with number).

7. **Implement TS2488 iterator checking** — Check that types used in `for..of`, spread, and destructuring have `[Symbol.iterator]()`. This would fix spread, destructuring, and for-of test categories.

### Tier 3: Easy Missing Checks (could gain 100–200 tests)

8. **TS2695** — Comma operator unused side-effect check. Simple AST analysis. Fixes 6+ categories at 0%.

9. **TS2872/TS2873** — Always truthy/falsy expression detection. Fixes conditional operator tests.

10. **TS17004** — JSX flag check. Trivial flag check. Fixes JSX tests missing `--jsx`.

11. **TS5076** — Mixed nullish/logical operator check. Parser-level.

12. **TS1345** — Void in boolean context check. Fixes strict mode tests.

13. **TS2515/TS2654** — Abstract member implementation check. Fixes class abstract keyword tests.

14. **TS2341** — Private accessibility check. Fixes class accessibility tests.

15. **TS2365** — Binary operator type checking for `+` operator. Fixes addition operator tests.

### Tier 4: Architectural (required for long-term progress)

16. **Timeout/cycle detection** — Add robust cycle detection for class hierarchies and recursive types. Would recover 321 timed-out tests.

17. **Node16/NodeNext module resolution** — Required for the 73 node module resolution tests.

18. **Type-only import/export tracking** — Required for 65 typeOnly tests.

19. **Named evaluation** — ES2022+ feature, required for 11 namedEvaluation tests.

20. **Import attributes parsing** — Required for 11 importAttributes tests.

---

## 6. Estimated Path to 60%

If the top 3 false-positive sources are fixed:
- TS2304 fixes: +800 tests (conservative)
- TS1005 fixes: +400 tests (conservative)
- TS2307 fixes: +200 tests (conservative)

That alone would bring the pass rate to approximately:
- **(5,706 + 1,400) / 12,379 = 57.4%**

Adding the easy missing checks (Tier 3):
- +150 tests from new error checks

**Projected: ~59% pass rate**, which with a few Tier 2 fixes reaches **60%+**.
