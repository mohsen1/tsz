# Conformance Fix Plan: Parallel Agent Strategy

**Goal:** Improve conformance from 36.9% to 60%+ through parallelized AI agent work.

**Current Status:**
- Pass Rate: 36.9% (4,495/12,197)
- Failed: 7,629 tests
- Crashed: 15 | OOM: 4 | Timeout: 54

---

## Error Analysis Summary

### Extra Errors (False Positives - We Report, TSC Doesn't)

| Error Code | Count | Description | Root Cause Area |
|------------|-------|-------------|-----------------|
| TS2322 | 11,773x | Type not assignable | Type compatibility |
| TS2694 | 3,104x | Namespace has no exported member | Module/namespace resolution |
| TS1005 | 2,703x | '{0}' expected | Parser |
| TS2304 | 2,045x | Cannot find name | Symbol resolution |
| TS2571 | 1,681x | Object is of type 'unknown' | Type inference |
| TS2339 | 1,520x | Property does not exist | Property access |
| TS2300 | 1,424x | Duplicate identifier | Declaration merging |
| TS2507 | 972x | Not a constructor function type | Constructor checking |

### Missing Errors (False Negatives - TSC Reports, We Don't)

| Error Code | Count | Description | Root Cause Area |
|------------|-------|-------------|-----------------|
| TS2318 | 3,386x | Cannot find global type | Global type lookup |
| TS2307 | 2,139x | Cannot find module | Module resolution |
| TS2304 | 1,977x | Cannot find name | Symbol resolution |
| TS2488 | 1,749x | Type must have Symbol.iterator | Iterator protocol |
| TS2322 | 917x | Type not assignable | Type compatibility |
| TS2583 | 706x | Change target library? | ES version checking |
| TS18050 | 680x | Value cannot be used here | Value checking |
| TS2362 | 553x | Left-hand side arithmetic | Operator checking |

---

## Agent Work Distribution (12 Agents)

### Phase 1: High-Impact False Positives (Agents 1-4)

These agents fix cases where we report errors that TSC doesn't. This is higher priority because false positives break real codebases.

---

#### Agent 1: Type Assignability False Positives (TS2322 Extra)

**Impact:** 11,773 extra TS2322 errors
**Files to focus on:**
- `src/solver/subtype.rs` - Subtype relationship checking
- `src/solver/assignable.rs` - Assignment compatibility
- `src/checker/type_checking.rs` - Type validation calls

**Problem patterns to investigate:**
1. Union type handling - we may be too strict with union members
2. Literal type widening - we may not widen literals correctly
3. Generic instantiation - wrong substitution in generic contexts
4. Object literal excess property checks - applying when we shouldn't
5. Contextual typing - not applying context correctly

**Prompt:**
```
You are fixing TS2322 false positives in a TypeScript compiler written in Rust.

PROBLEM: We report 11,773 extra "Type not assignable" errors that TSC doesn't report.

TASK: Find and fix cases where our type assignability checking is too strict.

KEY FILES:
- src/solver/subtype.rs - Subtype relationship checking
- src/solver/assignable.rs - Assignment compatibility
- src/checker/type_checking.rs - Type validation

INVESTIGATION STEPS:
1. Run: ./conformance/run-conformance.sh --filter=<test> on failing tests with TS2322 extras
2. Use --verbose to see the actual error messages
3. Compare our logic against TSC's behavior
4. Focus on: union types, literal widening, generics, object literals

PATTERNS TO FIX:
- Union assignability: `{a: 1} | {b: 2}` should be assignable to `{a?: number, b?: number}`
- Literal types: `"foo"` should be assignable to `string` in non-const contexts
- Generic constraints: Check we're not over-constraining type parameters

SUCCESS: Reduce extra TS2322 count by at least 3,000 errors.

RULES:
- No test-specific workarounds
- Fix root causes in the type system
- Run cargo test before committing
```

---

#### Agent 2: Namespace Resolution False Positives (TS2694, TS2339 Extra)

**Impact:** 3,104 extra TS2694 + 1,520 extra TS2339 errors
**Files to focus on:**
- `src/checker/modules.rs` - Module/namespace resolution
- `src/binder/state.rs` - Symbol binding
- `src/checker/symbol_resolver.rs` - Symbol lookup

**Problem patterns:**
1. Namespace member resolution not following re-exports
2. Declaration merging for namespaces incomplete
3. `export =` and `export default` handling
4. Ambient module declarations (`declare module "x"`)

**Prompt:**
```
You are fixing namespace/module member resolution false positives.

PROBLEM: We report 3,104 extra "Namespace has no exported member" (TS2694) and
1,520 extra "Property does not exist" (TS2339) errors.

TASK: Fix cases where we fail to resolve valid namespace/module members.

KEY FILES:
- src/checker/modules.rs - Module/namespace resolution
- src/binder/state.rs - Symbol binding
- src/checker/symbol_resolver.rs - Symbol lookup

INVESTIGATION:
1. Find tests with extra TS2694 errors
2. Trace how we resolve namespace members
3. Check declaration merging logic

PATTERNS TO FIX:
- Re-exports: `export { foo } from './bar'` members should be visible
- Declaration merging: `namespace N {}` + `namespace N {}` should merge
- Export assignment: `export = obj` members should be accessible
- Ambient modules: `declare module "x" { export const y: number }`

SUCCESS: Reduce extra TS2694 by 2,000+ and TS2339 by 1,000+
```

---

#### Agent 3: Parser Error False Positives (TS1005, TS2300 Extra)

**Impact:** 2,703 extra TS1005 + 1,424 extra TS2300 errors
**Files to focus on:**
- `src/parser/` - Parser implementation
- `src/binder/scope.rs` - Scope management for duplicates

**Problem patterns:**
1. Parser too strict on optional syntax
2. Duplicate identifier detection not respecting declaration merging
3. Error recovery creating spurious errors

**Prompt:**
```
You are fixing parser and duplicate identifier false positives.

PROBLEM: We report 2,703 extra "'{0}' expected" (TS1005) and
1,424 extra "Duplicate identifier" (TS2300) errors.

TASK: Fix overly strict parsing and duplicate detection.

KEY FILES:
- src/parser/ - Parser implementation
- src/scanner/ - Lexical analysis
- src/binder/scope.rs - Scope management

INVESTIGATION:
1. Find tests with extra TS1005 - these are parser errors
2. Find tests with extra TS2300 - duplicate identifier errors
3. Compare against TSC's parser behavior

PATTERNS TO FIX:
- TS1005: Optional semicolons, ASI (automatic semicolon insertion)
- TS1005: Trailing commas in various contexts
- TS2300: Function overloads are NOT duplicates
- TS2300: Interface merging is NOT a duplicate
- TS2300: Namespace + function/class merging

SUCCESS: Reduce extra TS1005 by 2,000+ and TS2300 by 1,000+
```

---

#### Agent 4: Unknown Type & Constructor False Positives (TS2571, TS2507 Extra)

**Impact:** 1,681 extra TS2571 + 972 extra TS2507 errors
**Files to focus on:**
- `src/checker/type_computation.rs` - Type inference
- `src/solver/lower.rs` - Type lowering
- `src/checker/class_type.rs` - Class constructor handling

**Problem patterns:**
1. Variables inferred as `unknown` when they should have a type
2. Class expressions not recognized as constructors
3. `new` operator being too strict

**Prompt:**
```
You are fixing unknown type and constructor false positives.

PROBLEM: We report 1,681 extra "Object is of type 'unknown'" (TS2571) and
972 extra "Not a constructor function type" (TS2507) errors.

TASK: Fix cases where we incorrectly infer unknown or reject valid constructors.

KEY FILES:
- src/checker/type_computation.rs - Type inference
- src/solver/lower.rs - Type lowering
- src/checker/class_type.rs - Class constructor handling

INVESTIGATION:
1. Find tests with extra TS2571 - we're inferring unknown when we shouldn't
2. Find tests with extra TS2507 - we're rejecting valid new expressions

PATTERNS TO FIX:
- TS2571: Catch clause variables should be `unknown` only in strict mode
- TS2571: Type guards should narrow away unknown
- TS2507: Class expressions are valid constructors
- TS2507: Functions with prototype property can be constructors
- TS2507: Generic constraints on constructor types

SUCCESS: Reduce extra TS2571 by 1,000+ and TS2507 by 500+
```

---

### Phase 2: High-Impact Missing Errors (Agents 5-8)

These agents add error detection that TSC has but we're missing.

---

#### Agent 5: Global Type Resolution (TS2318, TS2583 Missing)

**Impact:** 3,386 missing TS2318 + 706 missing TS2583 errors
**Files to focus on:**
- `src/checker/state.rs` - Global type lookup (`get_global_type`)
- `src/checker/context.rs` - Checker options and lib loading
- `src/binder/globals.rs` - Global symbol initialization

**Problem patterns:**
1. Not loading the right lib files based on `target`/`lib` options
2. Not reporting error when global type lookup fails (returning Any instead)
3. Not suggesting lib changes for ES features

**Prompt:**
```
You are adding missing global type error detection.

PROBLEM: We're missing 3,386 "Cannot find global type" (TS2318) and
706 "Do you need to change target library?" (TS2583) errors.

TASK: Ensure we properly detect and report missing global types.

KEY FILES:
- src/checker/state.rs - Global type lookup (get_global_type method)
- src/checker/context.rs - Compiler options
- src/binder/globals.rs - Global symbol initialization

INVESTIGATION:
1. Find how get_global_type handles missing types - it likely returns Any silently
2. Check how lib files are loaded based on compiler options
3. See how TSC detects ES version mismatches

IMPLEMENTATION:
- When get_global_type fails to find a type, emit TS2318
- Track which global types require which ES versions
- When type requires newer ES, emit TS2583 with suggestion

EXAMPLE:
```typescript
// @target: ES5
const p = new Promise<void>((r) => r());  // Should error: Cannot find global type 'Promise'
```

SUCCESS: Add detection for at least 2,500 TS2318 and 500 TS2583 errors
```

---

#### Agent 6: Module Resolution (TS2307 Missing)

**Impact:** 2,139 missing TS2307 errors
**Files to focus on:**
- `src/module_resolver.rs` - Module resolution
- `src/checker/modules.rs` - Module checking
- `src/binder/imports.rs` - Import processing

**Problem patterns:**
1. Not reporting errors when module resolution fails
2. Silently treating unresolved modules as Any
3. Not checking `moduleResolution` option correctly

**Prompt:**
```
You are adding missing module resolution error detection.

PROBLEM: We're missing 2,139 "Cannot find module" (TS2307) errors.

TASK: Ensure we properly detect and report when modules can't be resolved.

KEY FILES:
- src/module_resolver.rs - Module resolution logic
- src/checker/modules.rs - Module checking
- src/binder/imports.rs - Import processing

INVESTIGATION:
1. Trace what happens when `import { x } from './missing'` is processed
2. Check if we silently return Any for unresolved modules
3. Look at moduleResolution option handling

IMPLEMENTATION:
- When module resolution fails, emit TS2307
- Don't silently create Any type for missing modules
- Respect moduleResolution: "node", "node16", "bundler", etc.

EXAMPLE:
```typescript
import { foo } from './does-not-exist';  // Should error: Cannot find module './does-not-exist'
```

SUCCESS: Add detection for at least 1,500 TS2307 errors
```

---

#### Agent 7: Symbol Resolution (TS2304 Balance)

**Impact:** 1,977 missing + 2,045 extra TS2304 errors
**Files to focus on:**
- `src/binder/state.rs` - Symbol table construction
- `src/checker/symbol_resolver.rs` - Symbol lookup
- `src/checker/state.rs` - Name resolution calls

**Problem patterns:**
1. Some symbols we can't find that TSC finds (missing)
2. Some symbols we report unfound that TSC finds (extra)
3. Scope chain traversal differences

**Prompt:**
```
You are balancing symbol resolution - fixing both false positives AND false negatives.

PROBLEM: We have 1,977 missing "Cannot find name" (TS2304) errors AND
2,045 extra TS2304 errors. Both need fixing.

TASK: Align our symbol resolution with TSC exactly.

KEY FILES:
- src/binder/state.rs - Symbol table construction
- src/checker/symbol_resolver.rs - Symbol lookup
- src/checker/state.rs - Name resolution

INVESTIGATION:
1. Find tests where we MISS TS2304 - we find a symbol we shouldn't
2. Find tests where we have EXTRA TS2304 - we don't find a symbol we should
3. Compare scope chain behavior

EXTRA (false positive) PATTERNS:
- Type-only imports should still resolve for type positions
- Declaration hoisting (var, function)
- Ambient declarations
- Merged declarations across files

MISSING (false negative) PATTERNS:
- Using undeclared variables (we might be creating implicit Any)
- Block-scoped let/const before declaration (TDZ)
- Using private members outside class

SUCCESS: Reduce both extra AND missing TS2304 by 1,000+ each
```

---

#### Agent 8: Iterator Protocol (TS2488 Missing)

**Impact:** 1,749 missing TS2488 errors
**Files to focus on:**
- `src/checker/iterators.rs` - Iterator checking
- `src/checker/statements.rs` - For-of loops
- `src/checker/expr.rs` - Spread operators

**Problem patterns:**
1. For-of loops not checking iterator protocol
2. Spread on non-iterables not reported
3. Array destructuring of non-iterables

**Prompt:**
```
You are adding missing iterator protocol error detection.

PROBLEM: We're missing 1,749 "Type must have a '[Symbol.iterator]()' method" (TS2488) errors.

TASK: Ensure we check iterator protocol where required.

KEY FILES:
- src/checker/iterators.rs - Iterator checking (if exists, or create)
- src/checker/statements.rs - For-of loops
- src/checker/expr.rs - Spread operators

INVESTIGATION:
1. Find how we handle for-of loops with non-iterable types
2. Check spread operator handling: [...nonIterable]
3. Check array destructuring: const [a, b] = nonIterable

IMPLEMENTATION:
- for-of: Check that expression type has [Symbol.iterator]
- Spread: Check that spread argument is iterable
- Destructuring: Array patterns require iterable on RHS

EXAMPLE:
```typescript
const obj = { a: 1 };
for (const x of obj) {}  // Should error: Type '{ a: 1 }' must have a '[Symbol.iterator]()' method
const arr = [...obj];    // Should error: same
```

SUCCESS: Add detection for at least 1,200 TS2488 errors
```

---

### Phase 3: Medium-Impact Issues (Agents 9-11)

---

#### Agent 9: Type Assignability Missing (TS2322 Missing)

**Impact:** 917 missing TS2322 errors
**Files to focus on:**
- `src/solver/subtype.rs`
- `src/solver/assignable.rs`

**Problem patterns:**
1. Not checking certain assignment contexts
2. Skipping type checks in some expressions
3. Not enforcing strict null checks

**Prompt:**
```
You are adding missing type assignability error detection.

PROBLEM: We're missing 917 "Type not assignable" (TS2322) errors where TSC reports them.

TASK: Find contexts where we skip assignability checks that TSC performs.

KEY FILES:
- src/solver/subtype.rs - Subtype checking
- src/solver/assignable.rs - Assignment compatibility
- src/checker/type_checking.rs - Type validation calls

INVESTIGATION:
1. Find tests where we MISS TS2322 errors
2. Identify what assignment context we're not checking
3. Look for cases where we return early or skip checks

PATTERNS:
- Return statements: return value must match return type
- Property assignments: obj.prop = value
- Variable initialization with type annotation
- Argument passing to functions
- Strict null checks: assigning null to non-nullable

SUCCESS: Add detection for at least 600 TS2322 errors
```

---

#### Agent 10: Value Usage Errors (TS18050, TS2362 Missing)

**Impact:** 680 missing TS18050 + 553 missing TS2362 errors
**Files to focus on:**
- `src/checker/expr.rs` - Expression checking
- `src/checker/statements.rs` - Statement checking

**Problem patterns:**
1. Using types as values
2. Arithmetic on non-numeric types

**Prompt:**
```
You are adding missing value usage and arithmetic error detection.

PROBLEM: We're missing 680 "The value cannot be used here" (TS18050) and
553 "Left-hand side of arithmetic must be number/bigint" (TS2362) errors.

TASK: Add checks for invalid value usage and arithmetic operations.

KEY FILES:
- src/checker/expr.rs - Expression type checking
- src/checker/statements.rs - Statement checking

INVESTIGATION (TS18050):
- Find cases where type-only imports are used as values
- Check for using interfaces/types as values

INVESTIGATION (TS2362):
- Find binary arithmetic operations (+, -, *, /, %)
- Check left-hand operand type validation

IMPLEMENTATION:
- TS18050: When resolving a name in value position, check it's not type-only
- TS2362: For arithmetic operators, ensure operands are number/bigint/any/enum

EXAMPLE:
```typescript
type Foo = { a: number };
const x = new Foo();  // TS18050: 'Foo' only refers to a type, but is being used as a value

const y = "hello" - 5;  // TS2362: The left-hand side must be of type 'any', 'number', 'bigint'
```

SUCCESS: Add detection for at least 500 TS18050 and 400 TS2362 errors
```

---

#### Agent 11: Extra Symbol Resolution (TS2304 Extra - focused)

**Impact:** 2,045 extra TS2304 (shared with Agent 7 but different focus)
**Files to focus on:**
- `src/checker/state.rs` - Where we emit TS2304
- `src/binder/` - Symbol table construction

**Problem patterns:**
1. We emit TS2304 but the symbol should be found
2. Focus on WHY we can't find symbols that exist

**Prompt:**
```
You are fixing false positive TS2304 "Cannot find name" errors.

PROBLEM: We report 2,045 extra TS2304 errors for names that TSC successfully resolves.

TASK: Find why we fail to resolve valid symbols.

KEY FILES:
- src/checker/state.rs - Where we call symbol resolution
- src/binder/state.rs - Symbol table construction
- src/binder/scope.rs - Scope management

INVESTIGATION:
1. Find specific tests with extra TS2304
2. For each, trace why we can't find the symbol
3. Identify common patterns

LIKELY CAUSES:
- Globals not loaded (globalThis, window, document)
- Type parameters not in scope
- Declaration hoisting not respected
- Augmented modules not merged
- Namespace members not visible

SUCCESS: Reduce extra TS2304 by 1,500+
```

---

### Phase 4: Stability (Agent 12)

---

#### Agent 12: Crashes, OOM, and Timeouts

**Impact:** 15 crashed + 4 OOM + 54 timeout tests
**Files to focus on:**
- `src/solver/` - Type solving (infinite recursion)
- `src/checker/` - Recursion depth limits

**Prompt:**
```
You are fixing stability issues: crashes, out-of-memory, and timeouts.

PROBLEM:
- 15 tests crash
- 4 tests run out of memory
- 54 tests timeout

CRASHED TESTS (samples):
- destructuringParameterDeclaration10.ts - "invalid type: string 'true, false'"
- forwardRefInTypeDeclaration.ts - same parsing issue
These are compiler option parsing failures.

OOM TESTS:
- recursiveTypes1.ts
- genericDefaultsErrors.ts
- infiniteExpansionThroughInstantiation2.ts
- thislessFunctionsNotContextSensitive3.ts

TIMEOUT TESTS (samples):
- parenthesizedContexualTyping1.ts
- typeofOperatorInvalidOperations.ts
- moduleExportAssignment7.ts

TASK: Fix crashes, add recursion limits, fix infinite loops.

KEY FILES:
- src/checker/context.rs - Compiler option parsing (crashes)
- src/solver/instantiate.rs - Type instantiation (OOM)
- src/solver/lower.rs - Type resolution (timeouts)

INVESTIGATION:
1. Crashes: Parse "true, false" as valid boolean option values
2. OOM: Add depth limits to recursive type expansion
3. Timeouts: Find infinite loops in type resolution

SUCCESS: Zero crashes, zero OOM, reduce timeouts to <10
```

---

## Execution Guidelines

### For Each Agent

1. **Setup:**
   ```bash
   git checkout claude/conformance-fix-plan-dJ3CE
   git pull origin claude/conformance-fix-plan-dJ3CE
   ```

2. **Find Failing Tests:**
   ```bash
   # Find tests with specific error patterns
   ./conformance/run-conformance.sh --max=1000 --verbose 2>&1 | grep "TS2322"

   # Run a single test to debug
   ./conformance/run-conformance.sh --filter=compiler/someTest.ts --verbose
   ```

3. **Make Changes:**
   - Edit the relevant files
   - Run `cargo test --lib` to ensure no regressions
   - Run `cargo clippy` to fix warnings

4. **Validate:**
   ```bash
   ./conformance/run-conformance.sh --max=500
   ```

5. **Commit:**
   ```bash
   git add -A
   git commit -m "fix(checker): <description of fix>"
   git push origin claude/conformance-fix-plan-dJ3CE
   ```

### Coordination Rules

1. **File Ownership:** Each agent owns specific files. If you need to touch another agent's files, coordinate.
2. **Commit Often:** Small, focused commits that can be reviewed independently.
3. **Don't Break Others:** Run tests before pushing. If tests fail, fix before pushing.
4. **Document Changes:** Commit messages should explain WHY, not just WHAT.

### Success Metrics

| Agent | Target Reduction | Key Metric |
|-------|-----------------|------------|
| Agent 1 | -3,000 TS2322 extra | Assignability fixes |
| Agent 2 | -2,000 TS2694, -1,000 TS2339 | Namespace resolution |
| Agent 3 | -2,000 TS1005, -1,000 TS2300 | Parser fixes |
| Agent 4 | -1,000 TS2571, -500 TS2507 | Unknown/constructor |
| Agent 5 | +2,500 TS2318, +500 TS2583 | Global types |
| Agent 6 | +1,500 TS2307 | Module resolution |
| Agent 7 | Balance TS2304 | Symbol resolution |
| Agent 8 | +1,200 TS2488 | Iterator protocol |
| Agent 9 | +600 TS2322 missing | Assignability checks |
| Agent 10 | +500 TS18050, +400 TS2362 | Value/arithmetic |
| Agent 11 | -1,500 TS2304 extra | Symbol lookup |
| Agent 12 | 0 crashes, 0 OOM, <10 timeout | Stability |

**Combined Target:** Improve from 36.9% to 60%+ pass rate

---

## Priority Order

If running fewer than 12 agents, prioritize in this order:

1. **Agent 1** (TS2322 extra) - Highest impact single issue
2. **Agent 12** (Stability) - Unblocks other tests
3. **Agent 5** (Global types) - High impact missing errors
4. **Agent 2** (Namespace) - High impact false positives
5. **Agent 6** (Module resolution) - High impact missing
6. **Agent 7** (Symbol resolution) - Balances both directions
7. **Agent 3** (Parser) - Parser fixes help everything
8. **Agent 8** (Iterator) - Clear missing feature
9. **Agent 4** (Unknown/constructor) - Medium impact
10. **Agent 9** (TS2322 missing) - Medium impact
11. **Agent 10** (Value/arithmetic) - Medium impact
12. **Agent 11** (TS2304 focused) - Overlaps with Agent 7

---

## Appendix: Error Code Reference

| Code | Message Template |
|------|-----------------|
| TS1005 | '{0}' expected |
| TS2300 | Duplicate identifier '{0}' |
| TS2304 | Cannot find name '{0}' |
| TS2307 | Cannot find module '{0}' |
| TS2318 | Cannot find global type '{0}' |
| TS2322 | Type '{0}' is not assignable to type '{1}' |
| TS2339 | Property '{0}' does not exist on type '{1}' |
| TS2362 | Left-hand side of arithmetic must be 'any', 'number', 'bigint', or enum |
| TS2488 | Type '{0}' must have '[Symbol.iterator]()' method |
| TS2507 | Type '{0}' is not a constructor function type |
| TS2571 | Object is of type 'unknown' |
| TS2583 | Cannot find name '{0}'. Do you need to change target library? |
| TS2694 | Namespace '{0}' has no exported member '{1}' |
| TS18050 | '{0}' only refers to a type, but is being used as a value |

## Commands Reference

```bash
# Build
cargo build                              # Native build
wasm-pack build --target nodejs          # WASM build

# Test
cargo test --lib                         # All unit tests
cargo test --lib solver::subtype_tests   # Specific module

# Conformance
./conformance/run-conformance.sh --all --workers=8    # Full suite
./conformance/run-conformance.sh --max=100            # Quick check
./conformance/run-conformance.sh --native             # Use native (faster)
./conformance/run-conformance.sh --filter=path/test   # Single test
```

---

## Rules

| Don't | Do Instead |
|-------|------------|
| Chase pass percentages | Fix root causes systematically |
| Add test-specific workarounds | Fix underlying logic |
| Suppress errors to pass tests | Understand why error is wrong |


