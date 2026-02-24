# Conformance TODO

**Goal**: `./scripts/conformance.sh` prints ZERO failures.
**Current score**: ~8075/12574 (64.2%) — full suite, fingerprint level

---

## High Impact — Core Type System

### TS2322/TS2339/TS2345 — Type mismatch / property access / argument type (ongoing)
- **Tests**: Hundreds across the suite (TS2322: ~222, TS2339: ~47, TS2345: ~40 single-code)
- **Status**: Partially implemented, ongoing solver/checker type relation work
- **Root cause**: Core assignability, property resolution, and argument type checking gaps
- **Difficulty**: HIGH (broad, incremental)

### TS2353 — Intersection freshness false positives (~76 tests)
- **Root cause**: `tsz-solver/src/intern/intersection.rs` propagates `FRESH_LITERAL` flag
  via OR when merging objects in an intersection. Intersected types appear fresh when they
  shouldn't, triggering false excess property checks.
- **Fix needed**: Change freshness propagation to AND instead of OR for intersection merging.
- **Difficulty**: MEDIUM-HIGH

### TS2353 — Remaining excess property gaps
- **Spread freshness**: Objects via spread (`{...a}`) should be non-fresh — requires freshness tracking through spread
- **Recursive array types**: `interface Foo extends Array<Foo>` patterns need recursive recognition in solver
- **Union excess check for valid assignments**: Discriminant narrowing needed in success path (not just failure path)

---

## High Impact — Not Implemented Error Codes

### TS2411 — Index signature property compatibility (18 single-code tests)
- **Diagnostic**: "Property '{0}' of type '{1}' is not assignable to '{2}' index type '{3}'."
- **Needed**: Check that all properties of an interface/class are assignable to the index signature type
- **Difficulty**: MEDIUM-HIGH

### TS2343 — tslib emit helpers (47 single-code tests)
- **Diagnostic**: "This syntax requires an imported helper named '{1}' which does not exist in '{0}'."
- **Needed**: Check tslib exports when `importHelpers: true`
- **Note**: ES decorator helpers (`__esDecorate`, `__runInitializers`, etc.) ARE implemented separately
- **Difficulty**: HIGH (module resolution required)

### TS2433 — Namespace-style import cannot be called/constructed (10 tests)
- **Diagnostic**: Message constant exists in `diagnostics/data.rs` but NO checker code emits it
- **Difficulty**: MEDIUM

### TS2497 — Module can only be referenced with ECMAScript imports (13 tests)
- **Needed**: Detect `export =` modules imported via ESM syntax; check `esModuleInterop`/`allowSyntheticDefaultImports`
- **Difficulty**: MEDIUM

### TS2550 — Property needs newer lib target (9 tests)
- **Diagnostic**: "Property 'X' does not exist on type 'Y'. Do you need to change your target library?"
- **Needed**: Lib-awareness to suggest `--lib es2015` etc.
- **Difficulty**: MEDIUM-HIGH

### TS2585 — Symbol at ES5 target (10-15 tests)
- **Root cause**: Transitive lib loading. `lib.dom.d.ts` has `/// <reference lib="es2015" />`
  which pulls ES2015 Symbol value bindings even at ES5 target. Symbol resolves as a value,
  so no TS2585 is emitted.
- **Fix needed**: Lib loading architecture must respect target level during transitive loading
- **Difficulty**: HIGH

### TS2729 — Property used before initialization (6 single-code tests)
- **Diagnostic**: "Property '{0}' is used before its initialization."
- **Needed**: Class member ordering analysis with `useDefineForClassFields`
- **Difficulty**: MEDIUM

### TS2875 — JSX runtime module not found (14 tests)
- **Needed**: JSX pragma parsing (`@jsxImportSource`), `getJSXImplicitImportBase()`,
  `getJSXRuntimeImport()`, module resolution for implicit imports
- **Difficulty**: HIGH

### TS18046 — 'x' is of type 'unknown' — remaining paths
- **Implemented**: Property access (dot, element, private identifier) works
- **Deferred paths**: Calls (`x()` on unknown), constructors (`new x()`), binary ops (`x + 1`),
  unary ops (`-x`, `+x`)
- **Blocker**: `TypeId::UNKNOWN` is used both for genuine user-declared `unknown` AND as
  fallback for unresolved types. Until we distinguish these (e.g., `TypeId::UNRESOLVED` or a
  flag), expanding TS18046 causes regressions on multi-file tests.
- **Difficulty**: MEDIUM-HIGH (requires TypeId architecture decision)

### TS1382 — Unexpected token in decorator context (7 tests)
- **Needed**: Parser-level fix for expression start in decorated declarations
- **Difficulty**: MEDIUM

### TS17019 — Resolving expression in computed property (6 tests)
- **Difficulty**: MEDIUM

---

## Medium Impact — Diagnostic Gaps

### TS2304 — Extra "cannot find name" emissions (204 tests, 25 pure)
- **Root cause**: tsz emits TS2304 for identifiers that should be resolved from lib types
  or through more advanced module resolution
- **Specific patterns**:
  - Computed property names in parse error contexts (11 tests): `{ [e] }` emits false TS2304
    because `is_in_computed_property` guard prevents suppression. Needs `ThisNodeHasError` equivalent.
  - UMD global identifiers (4 tests): UMD globals not resolved — module resolution gap
  - `declare` keyword misparse (8 tests): In invalid modifier positions, parser treats `declare`
    as identifier, emitting false "Cannot find name 'declare'". Suppression requires `has_parse_errors()`
- **Difficulty**: MEDIUM (each pattern is different)

### TS7006 — Contextual typing gaps (16 tests)
- **Root cause**: tsz fails to contextually type parameters in some generic/mapped-type scenarios
- **Specific gaps**:
  - Generic constraint contextual typing (2 pure + 6 mixed): Solver doesn't use apparent type
    (constraint) of type params for contextual typing during generic inference
  - Module augmentation (7 mixed): Callbacks like `arr.map(x => ...)` not contextually typed
    through augmented interface methods
  - Binding pattern references (1 test): Cross-reference between binding elements not implemented
- **Difficulty**: MEDIUM-HIGH (solver-level)

### TS2454 — Variable used before assignment — remaining patterns (16 quick-win tests)
- 9 "pure" tests (tsz emits zero errors) and 7 multi-file tests
- **Patterns**: try/catch destructuring, ES5 Symbol var, for-of pre-loop usage,
  computed property names, JSDoc type annotations
- **Difficulty**: MEDIUM (each requires targeted flow analysis work)

### TS2454/TS2564 — Over-emission (16 tests)
- We emit more "used before assigned" / "not definitely assigned" errors than tsc
- Flow analysis precision gaps
- **Difficulty**: MEDIUM

### TS6133 — Unused variable detection remaining patterns (9+ tests)
- **Remaining patterns** (each requires a different fix):
  - `import *` as unused
  - for-of/for-in loop `const _` suppression
  - ES private fields (`#unused`)
  - `infer` positions
  - JSDoc `@template` tags
  - Self-references
  - Dynamic property names
  - Type parameter merging
- **Difficulty**: MEDIUM (high payoff if done systematically)

### TS2403 — False positives (9 single-diff tests)
- **Three root causes**:
  - (a) Overload resolution incorrectly picks first overload for `any`-typed arguments
  - (b) Getter/setter paired type inference missing — setter param inferred as `any`
  - (c) Mapped types (Pick, Readonly, Partial) not fully evaluated before redeclaration identity check
- **Difficulty**: HARD (each requires deep solver/checker work)

### TS2741 — Property missing in type (36 missing, 13 extra)
- Already implemented for basic cases
- Remaining failures involve class-to-class assignment where member resolution gaps prevent detecting missing properties
- **Difficulty**: MEDIUM

### TS2688 — False positive reference types (26 tests, 14 single-code)
- `/// <reference types="..." />` resolver doesn't handle:
  - (a) `node_modules` walk-up from referencing file
  - (b) `package.json` `types`/`typings` fields for non-`index.d.ts` entries
  - (c) Node16+ `exports` resolution
  - (d) Scoped `@types` mangling (`@beep/boop` → `@types/beep__boop`)
- **Difficulty**: MEDIUM-HIGH

### TS2792 — "Did you mean to set moduleResolution to nodenext?" (41 single-code tests)
- **Root cause**: `effective_module_resolution()` in `src/config.rs` maps ES2015/ES2020/ESNext → Bundler,
  but tsc defaults these to Classic. Attempted fix caused -31 regressions.
- **Fix options**:
  - (a) Fix `effective_module_resolution()` to match tsc defaults (13 callers, broad impact)
  - (b) Add separate `tsc_diagnostic_module_resolution()` helper just for TS2792 decision
- **Difficulty**: MEDIUM-HIGH

#### Run note (2026-02-24)
- **Deferred**: `tests/conformance/suite/types` slices for `TS2322/TS2345/TS2339` remain out-of-scope for this pass; they still require cross-layer Solver/Checker compatibility-gate refactors (`query_boundaries`, `CompatChecker`, `Lazy(DefId)`-aware relation traversal).

### TS2451 — False positives (7 tests)
- Two patterns:
  - (a) Wrong code choice (TS2451 vs TS2300) for var/let redeclaration conflicts
  - (b) JS file declarations with `@typedef` and late-bound assignments
  - (c) Multi-file `let` redeclaration detection (6 tests)
- **Difficulty**: MEDIUM

---

## Parser Issues

### TS1191 — Import modifier diagnostic position (8 tests)
- Parser emits TS1191 at `import` keyword (column 8) instead of `export` keyword (column 1)
- The diagnostic span should start at the beginning of the statement
- **Difficulty**: EASY

### TS1206 — Remaining: `decoratorOnUsing.ts`
- `@dec using` still emits TS1206 from parser `parse_decorated_declaration` (UsingKeyword branch)
- TSC produces TS1134 instead
- **Difficulty**: EASY

### TS1128 — Runner line number shift (17 tests)
- Parser emits TS1128 ("Declaration or statement expected") correctly, but conformance tests
  fail because line numbers shift by 1 due to directive stripping (e.g., `// @target: es2015` header)
- **Root cause**: Runner-level issue, not a compiler bug
- **Difficulty**: EASY-MEDIUM

### TS18004 — Shorthand property false positive (5 tests)
- Emitted for parser error-recovery shorthand properties in `{ a; b; c }` (semicolons instead of commas)
- tsc suppresses this near parse errors. Attempted fix with `node_has_nearby_parse_error` didn't work —
  parse error positions don't align with shorthand property node spans.
- **Difficulty**: MEDIUM

### TS1501 — Remaining scanner regex validation (4 tests)
- `unicodeExtendedEscapesInRegularExpressions` tests need TS1198 (extended Unicode escape out of range)
  and TS1508 (unexpected `}` in regex)
- **Difficulty**: MEDIUM

---

## Config / Infrastructure

### TS5057 — Cannot find tsconfig.json / project references (52 tests)
- Requires `tsc --build` and composite project-reference support (not yet implemented)
- **Difficulty**: HIGH

### TS5071 — resolveJsonModule incompatible with module kind (not implemented)
- Needed by 3 TS5095 remaining tests, plus syntheticDefaultExports and noBundledEmitFromNodeModules tests
- **Difficulty**: EASY-MEDIUM

### TS5095 — Remaining failures
- `bundlerOptionsCompat.ts`: Needs TS5095 + TS5109
- `pathMappingBasedModuleResolution3_node.ts`: Needs TS5095 + TS18003
- **Difficulty**: EASY (once TS5071/TS5109 exist)

### TS5102 — Remaining failures
- `verbatimModuleSyntaxCompat*.ts` (4 tests): Need verbatimModuleSyntax validation (TS1286, TS1484)
- `preserveValueImports.ts`, `importsNotUsedAsValues_error.ts`: Need TS1484/TS2305
- `keyofDoesntContainSymbols.ts`: Needs `keyofStringsOnly` semantic behavior
- **Difficulty**: MEDIUM

### TS18003 — Remaining failures (34 tests)
- **Windows-style paths**: `@Filename: A:/foo/bar.ts` creates subdirectories in temp dir instead of
  being treated as a separate drive root
- **node_modules @types**: Compiler discovers @types files as source files instead of type-only references
- **Difficulty**: MEDIUM (runner-level)

### TS6082 — Remaining
- When `module` is NOT explicitly set but there are external modules, tsc emits TS6089 instead of TS6082
- `commonSourceDir5.ts`: Needs TS6082 + TS18003 (Windows path issue)
- **Difficulty**: EASY-MEDIUM

### Fingerprint line number mismatch (tsconfig)
- Remaining fingerprint-level failures in config-diagnostic tests are caused by line/column positions
  from strict-family defaults placement, message text variations, and missing/extra diagnostics
- **Difficulty**: MEDIUM (runner-level)

---

## Scope / Symbol Resolution

### TS2430 — react16.d.ts false positives (RESOLVED)
- The underlying `file_locals` scope issue has been resolved by previous work.
  Unit test `test_module_namespace_same_name_interface_no_false_positive` now passes.
  Remaining TS2430 conformance failures are generic interface extension compatibility
  and diagnostic position differences, not the react16 scope issue.

### TS2506 — Cross-binder SymbolId collision (`commentOnAmbientModule.ts`)
- `resolve_heritage_symbol` resolves `D` from `a.ts` binder but looks up exports using `b.ts`
  binder, where the SymbolId indexes a different symbol
- **Fix needed**: Binder-aware cross-file symbol resolution
- **Difficulty**: HARD

### TS2693 — Remaining false positives (9 tests)
- False TS2693 for `number[]`, `string[]`, `boolean[]` in value positions (e.g., `var na = new number[]`)
- tsc emits only TS1011 for the missing bracket argument
- **Fix**: Suppress TS2693 when parent is element access with missing argument
- **Files**: `type_computation_access.rs` (lines 27-73), `type_computation_identifier.rs` (lines 867-883)
- **Difficulty**: EASY

### TS2702 — Namespace-scoped type-as-namespace resolution (remaining tests)
- `errorForUsingPropertyOfTypeAsType01.ts` Tests 1-5: Checker resolves `Foo.bar` inside namespace
  via namespace member lookup (emitting TS2694) instead of the type-as-namespace path
- **Difficulty**: MEDIUM

### TS2661 — Cross-file re-export (4 tests)
- "Cannot export '{0}'. Only local declarations can be exported from a module."
- Requires cross-file symbol resolution for re-exports
- **Difficulty**: MEDIUM

---

## JSX

### TS7026 — JSX IntrinsicElements (56 tests)
- "JSX element implicitly has type 'any' because no interface 'JSX.IntrinsicElements' exists."
- Core lookup logic exists but many tests fail due to React/JSX module resolution failures
- **Difficulty**: HIGH

### TS2875 — JSX runtime module (14 tests)
- See "Not Implemented Error Codes" section above

---

## Other Open Issues

### TS2320 — Interface extension remaining gaps
- Class visibility conflicts (`extends C, C2` with public/private x) — not detected
- Generic vs non-generic signatures need identity comparison instead of mutual assignability
- `exactOptionalPropertyTypes` compiler option not yet supported
- **Difficulty**: MEDIUM-HIGH

### TS2367 — Remaining gaps
- Empty object `{}` vs type parameter `T`: `types_have_no_overlap` doesn't handle unconstrained
  type params being assignable to `{}`
- Unreachable code after always-true comparisons in loop bodies
- **Difficulty**: MEDIUM

### TS2589 — Remaining test coverage
- Core implementation done (+12 tests). Remaining failures are tests where TS2589 co-occurs
  with other missing error codes
- **Difficulty**: LOW (organic)

### TS2300 — Remaining patterns
- **False positives (4 tests)**: Cross-file class/interface merge, JS constructor+class merge,
  numeric/string property name quoting differences
- **Missing (6 tests)**: JSDoc @typedef/@import duplicate detection, type param vs local interface
  clash, unique symbol computed property duplicates in classes
- **Difficulty**: MEDIUM

### TS2846 — Message text: .js extension suggestion
- Our TS2846 says "Did you mean to import './a'?" but tsc says "Did you mean to import './a.js'?"
- Affects several TS5097 and TS2846 tests
- **Difficulty**: EASY

### TS2589 — Remaining (9 tests, now partially fixed)
- Infrastructure is complete. Remaining failures co-occur with other missing codes.

---

## Reference: Key Architecture Notes

These notes from fixed issues contain useful context for future work:

### TypeId::UNKNOWN dual-use problem
Our solver uses `TypeId::UNKNOWN` both for genuine user-declared `unknown` types AND as fallback
for unresolved types. This blocks TS18046 expansion (calls, ops on unknown) because we can't
distinguish "user wrote `unknown`" from "resolution failed." Fix requires either `TypeId::UNRESOLVED`
or a separate flag.

### Intersection freshness propagation
`tsz-solver/src/intern/intersection.rs` line ~382 propagates `FRESH_LITERAL` flag via OR when
merging objects. This makes intersected types appear fresh, triggering false excess property checks
(~76 tests). Fix: AND instead of OR.

### file_locals flat scope (TS2430 — RESOLVED)
The binder's `file_locals` scope issue that caused false TS2430 in react16.d.ts patterns
has been resolved. Unit test confirms correct behavior.

### Lib loading and target level (TS2585)
`lib.dom.d.ts` contains `/// <reference lib="es2015" />` which pulls ES2015 bindings regardless
of target. Lib loading architecture must respect target level during transitive loading.

### effective_module_resolution defaults (TS2792)
`effective_module_resolution()` maps ES2015/ES2020/ESNext → Bundler, but tsc defaults to Classic.
This affects 41 tests. Fix has 13 callers — broad impact.

### TS2322 centralized gateway
All TS2322/TS2345/TS2416 paths must use one compatibility gateway via `query_boundaries`.
Gateway order: relation → reason → diagnostic rendering. New checker code must route through
`query_boundaries/assignability`, not call `CompatChecker` directly.

---

## Reference: Completed Work

All items below have been validated against the codebase (implementations + tests confirmed).

| Error Code | Description | Impact |
|-----------|-------------|--------|
| TS2693 | Suppress parse-recovery cascades for `new number[]` | Fixed |
| TS5025 | Canonical option name mapping (53 entries) | +23 tests |
| TS2300 | Duplicate identifier (parameter+var, interface all-occurrences, export default class, Symbol properties, namespace merge) | +3 tests each fix |
| TS1206 | ES decorators on class expressions | +19 tests |
| TS2454 | Variable used before assignment (parent-walk fallback + compound read-write fix) | +14, +7 tests |
| TS6133 | Write-only parameters + underscore suppression for destructuring | +4, +1 tests |
| TS2305/TS2459/TS2460/TS2614 | Module name quoting in diagnostics | +11 tests |
| TS2882 | noUncheckedSideEffectImports default (false→true) | +10 tests |
| TS2506 | False circular reference in heritage checking | +8 tests |
| TS2688 | Cannot find type definition file (tsconfig types array) | +35 tests |
| TS2430/TS6053 | .lib/ diagnostic filtering in conformance runner | +2 tests |
| TS5095 | Option 'bundler' requires compatible module kind | +15, +4 tests |
| TS5103 | Invalid ignoreDeprecations value (accepts "5.0", "5.5") | +16, +48 tests |
| TS18003 | No inputs found in config file (fingerprint alignment) | +36 tests |
| TS5052 | checkJs requires allowJs | +1 test |
| TS1194 | Export declarations in ambient namespaces | +2 tests |
| TS5097 | Import .ts extension without allowImportingTsExtensions | +1 test |
| TS2839 | Object reference comparison always false/true | +1 test |
| TS7036 | Dynamic import specifier type | +3 tests |
| TS1202 | False TS1202/TS1203 (module_explicitly_set flag) | +29 tests |
| TS5102 | Option has been removed (deprecated/removed options) | +4 tests |
| TS2683 | 'this' implicitly has type 'any' (explicit this param, nested functions, any receivers) | +2, +12, +4 tests |
| TS2320 | Interface extension (optionality, hierarchy traversal, cross-declaration, type args) | +1, +2 tests |
| TS2397 | Global identifier declaration conflict (undefined, globalThis) | +8 tests |
| TS7041 | Arrow function captures global this | +2 tests |
| TS2481 | Cannot initialize outer scoped variable in block scope | +4 tests |
| TS2343 | ES decorator helpers (esDecorate, runInitializers, setFunctionName, propKey) | +34 tests |
| TS7057 | Yield implicit any | +6 tests |
| TS6082 | Only 'amd' and 'system' modules alongside --outFile | +17 tests |
| TS2721/TS2722/TS2723 | Cannot invoke possibly null/undefined object | +4 tests |
| TS2451 | Block-scoped variable redeclaration ordering (source position) | +1 test |
| TS1501 | Regex flag target message text (lowercase forms) | +15 tests |
| TS2589 | Excessive instantiation depth (eager evaluate_application_type) | +12 tests |
| TS2385/TS2383/TS2386 | Overload modifier consistency (access, export, optional) | +3 tests |
| TS2450 | Const enum forward reference exemption | +3 tests |
| TS1323 | Dynamic import module flag validation | +4 tests |
| TS2384 | Overload ambient consistency (skip implementations) | +3 tests |
| TS2702 | Type-as-namespace distinction (TS2702 vs TS2713) | 0 regression |
| TS2540 | Parenthesized readonly property assignment | +8 tests |
| TS7006 | null/undefined default parameters suppress TS7006 | +2 tests |
| TS2367 | Duplicate overlap check removal (code cleanup) | 0 tests |
| TS18050 | String concatenation with null/undefined suppression | included in score |
| TS2353 | Discriminated union excess check + type alias name display | +76 tests |
| TS2774 | Truthiness check for uncalled functions in conditionals | +5 tests |
| TS1118 | Duplicate get/set accessors (TS1118 instead of TS1117) | +6 tests closer |
| TS18046 | 'x' is of type 'unknown' (property access paths only) | +2 tests |
| TS2440 | Import conflicts with local declaration | implemented |
| TS2580 | Cannot find name (TS2580 vs TS2591 distinction) | implemented |
| TS6046 | Argument for option must be (config validation) | implemented |
| TS2304 | File-level syntax error suppression | +66 tests |
| TS2524→TS1109 | Bare await in parameter defaults | +38 tests |
| TS2713 | Skip false positives for ALIAS symbols and parse error contexts | +32 tests |
| skipLibCheck | Skip .d.ts type checking when enabled | +6 tests |
| checkJs | Fix redundant checker.check_js propagation | +11 tests |
| TS5069/TS5053 | Config checks for declaration-related options | +7 tests |
| TS5070/TS5071/TS5098 | resolveJsonModule/resolvePackageJson validation | +9 tests |
| TS2528 | Multiple default exports position fix | +1 test |
| TS18003 | Windows-style path handling in conformance runner | +10 tests |
