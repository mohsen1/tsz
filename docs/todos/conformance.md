# Conformance TODO

**Goal**: `./scripts/conformance.sh` prints ZERO failures.
**Current score**: ~14769/19201 (76.9%) — full suite, error-code level
**Fingerprint score**: ~8099/12574 (64.4%) — full suite, fingerprint level

---

## High Impact — Core Type System

### TS2322/TS2339/TS2345 — Type mismatch / property access / argument type (ongoing)
- **Tests**: Hundreds across the suite (TS2322: ~222, TS2339: ~47, TS2345: ~40 single-code)
- **Status**: Partially implemented, ongoing solver/checker type relation work
- **Root cause**: Core assignability, property resolution, and argument type checking gaps
- **Difficulty**: HIGH (broad, incremental)

### ~~TS2353 — Intersection freshness false positives~~ RESOLVED
- Fixed: intersection merging now uses AND logic for FRESH_LITERAL propagation

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

### ~~TS1382 — Unexpected token `>` in JSX text~~ PARTIALLY RESOLVED
- **Fixed**: Scanner now emits TS1382 (`>`) and TS1381 (`}`) during JSX text scanning
- **Remaining**: Tests that expect TS1382 also need other JSX diagnostics (TS1003, TS17014, etc.) to pass

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

### TS6133 — Unused variable detection remaining patterns (9 tests)
- **Remaining patterns** (each requires a different fix):
  - `import *` as unused
  - for-of/for-in loop `const _` suppression
  - ~~ES private fields (`#unused`)~~ RESOLVED — `name.starts_with('#')` check + reference tracking in private property access and `#name in expr`
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

### ~~TS2792 — "Did you mean to set moduleResolution to nodenext?"~~ PARTIALLY RESOLVED
- **Fixed**: Added `implied_classic_resolution` flag to `CheckerOptions`, computed from
  `effective_module_resolution()` at config resolution time. Updated all 5 TS2792 emission
  points (import_checker, module resolution, driver) to use the flag instead of `ModuleKind`
  pattern matching. (+3 tests)
- **Remaining**: 28 missing, 70 extra TS2792 — many tests have multiple error mismatches
  beyond just the TS2792/TS2307 code swap.

#### Run note (2026-02-24)
- **Deferred**: `tests/conformance/suite/types` slices for `TS2322/TS2345/TS2339` remain out-of-scope for this pass; they still require cross-layer Solver/Checker compatibility-gate refactors (`query_boundaries`, `CompatChecker`, `Lazy(DefId)`-aware relation traversal).

#### Run note (2026-02-25)
- **Fixed**: TS5103 — removed erroneous "6.0" from valid ignoreDeprecations values. tsc 6.0 only accepts "5.0"; "6.0" is NOT yet valid per tsc's conservative deprecation strategy (+48 tests).
- **Fixed**: TS1131 — parser now emits "Property or signature expected" instead of silent skip or generic TS1012 for invalid tokens in interface/type literal member positions (+tests via fingerprint improvement).
- **Investigated**: TS7017 — "Element implicitly has 'any' because type has no index signature." Diagnostic defined but not emitted. Implementation needs ~20-30 lines in `property_access_type.rs` to distinguish dot-notation (TS7017) from bracket-notation (TS7053) under `noImplicitAny`. 6-8 tests. Deferred for next session.
- **Investigated**: TS2657 — "JSX expressions must have one parent element." JSX parser needs sibling-element detection after first JSX element parse. MEDIUM difficulty, ~50-100 lines. 5-8 tests.
- **Investigated**: TS1389 — "'{0}' is not allowed as a variable declaration name." Partially implemented (strict mode only). Needs expanded reserved keyword list. LOW-MEDIUM, ~80-150 lines. 5-7 tests.

#### Run note (2026-02-24, session 2)
- **Fixed**: TS5103 — removed bogus "5.5" from valid ignoreDeprecations list (+1 test).
- **Fixed**: TS2435/TS1035 — module augmentations inside ambient external modules no longer false-positive TS2435 or TS1035 (+4 tests).
- ~~**Investigated but deferred**: TS5071~~ RESOLVED below.

#### Run note (2026-02-25, session 4)
- **Fixed**: TS2792→TS2307 code swap — Added `implied_classic_resolution` to CheckerOptions, fixed all 5 emission points. TS2792 only fires when effective resolution is Classic. (+3 tests, 8077/12574)
- **Investigated but reverted**: TS5103 false positive removal — tsc only emits TS5103 when there are TS5101/TS5107 deprecated options to suppress. Removing unconditional TS5103 emission was correct behavior but caused net -48 regression because 43 conformance tests expect TS5103 for `@ignoreDeprecations: 6.0` pragmas.
- **Analysis**: 2449 tests have diff=0 (matching error codes, different fingerprints). These are diverse — no single fix flips many. Top patterns: TS2322 column offsets (error at wrong node), TS2769 span at callee vs first arg, message text differences (type alias expansion, union member ordering).

#### Run note (2026-02-25, session 2)
- **Fixed**: TS5071 — `moduleResolution: bundler` now implies `resolveJsonModule=true`. When combined with `module: none/system/umd`, TS5071 is now emitted. Error position falls back to `module` key when `resolveJsonModule` is absent from tsconfig (+1 test).
- **Investigated**: TS7017 — Only emitted by tsc for `globalThis` dot-access (not element access). Element access always uses TS7053 regardless of whether object has index signatures. Previous session's analysis was incorrect about TS7017 being a generic "no index signature" diagnostic. Implementation would require detecting `globalThis` symbol in property access paths.
- **Investigated but deferred**: TS2323 — "Cannot redeclare exported variable." Missing for exported default function redeclarations. The `has_variable_conflict` check only covers `VARIABLE` flag, not `FUNCTION`. Attempted fix (expanding to include FUNCTION) caused -3 regression because it changed TS2300→TS2323 for cases that should remain TS2300. Needs more careful condition logic.
- **Investigated but deferred**: TS2439 — "Import or export declaration in an ambient module declaration cannot reference module through relative module name." Already implemented in `import_equals_checker.rs` but 4 tests still fail. Likely test runner or multi-file resolution issue, not a checker gap.
- **Investigated but deferred**: TS2451 — multi-file block-scoped variable redeclaration. Cross-file symbol resolution only adds local declarations to conflict set. Fixing requires project-level aggregation of conflicts.

#### Run note (2026-02-25, session 3)
- **Fixed**: TS2469 — "The '{0}' operator cannot be applied to type 'symbol'." Was using wrong diagnostic constant (TS2736 generic operator error instead of TS2469 symbol-specific). Also added missing unary +/-/~ and compound += symbol checks. Fixed solver `evaluate_plus_chain` fast-path bypassing symbol errors, and added relational operator pre-check in binary.rs. Net improvement: +5 tests (4432 failing, down from 4437).
- ~~**Investigated but deferred**: TS1389~~ RESOLVED in session 5.
- **Investigated but deferred**: TS1181 — "Array element destructuring pattern expected." Parser-level issue. MEDIUM effort.

#### Run note (2026-02-25, session 4)
- **Fixed**: TS2661 — "Cannot export '{0}'. Only local declarations can be exported from a module." Rewrote locality check in `module_checker.rs` to use `decl_file_idx` for multi-file mode and scope-table lookup for `declare module "m"` contexts. Key insight: `file_locals` includes merged globals from all files via `create_binder_from_bound_file`, so a simple `file_locals.get()` check was insufficient (+7 tests, 4082→4089).

#### Run note (2026-02-25, session 5)
- **Fixed**: TS1389 — "'{0}' is not allowed as a variable declaration name." Parser now emits TS1389 instead of generic TS1359 when a reserved word appears as a var/let/const/using declaration name. Added `error_reserved_word_in_variable_declaration()` and intercept in `parse_variable_declaration_name()` (+2 tests, 4089→4091).
- **Fixed**: TS1382/TS1381 — Scanner now emits TS1382 (bare `>`) and TS1381 (bare `}`) inside JSX text content. Prerequisite for JSX conformance; no immediate test gains (tests need additional JSX fixes).
- **Fixed**: TS2354 — False positive tslib helper detection. `required_helpers()` now respects target level: `__extends` only needed at target < ES2015. Prevents false TS2354 when `--importHelpers` is set but class extends is native (+2 tests, 4090→4092).
- **Investigated but reverted**: TS2497 — "Module can only be referenced with ECMAScript imports/exports." Implementation detected `export=` in module exports table for namespace imports, but was too aggressive (8 false positives). Needs deeper solver integration to check if exported value is namespace-like before emitting. Deferred.
- **Remaining TS2354 false positives (4 tests)**: Multi-target test configurations (es5+es2015), inline tslib file detection, and decorator helper awareness at es2022+ target.

#### Run note (2026-02-25, session 6)
- **Fixed**: TS1436 — "Decorators must precede the name and all keywords of property declarations." Parser now emits TS1436 for two patterns: (a) decorator after keyword modifiers (`public @dec prop`), and (b) decorator after property name (`private prop @decorator`). Both patterns consume the misplaced decorator for recovery, preventing cascading TS1146/TS1005 errors (+9 conformance tests at error-code level, +3 at fingerprint level).
- **Investigated**: TS18033 — "Type is not assignable as required for computed enum member values." Diagnostic defined but not emitted. Needs type evaluation of enum member initializers via solver and assignability check to `number`. ~4-9 tests. MEDIUM difficulty, deferred — requires solver boundary integration.
- **Investigated**: TS2497 (13 tests), TS2433 (10 tests), TS2550 (9 tests), TS1382 (8 tests), TS17019 (7 tests), TS7017 (6 tests) — all defined in diagnostic data but not emitted. Each requires different checker/solver integration. See previous session notes for TS2497 investigation.

#### Run note (2026-02-25, session 7) — expressions/functionCalls area
- **Area**: expressions/functionCalls (25.0% → 41.7%, 6/24 → 10/24 on old framework)
- **Net gain**: +5 tests on new TSC cache framework (6516 → 6521)
- **Fixed**: TypeQuery resolution in new-expressions — When `typeof ClassName` comes through an interface/object property (e.g., `interface C { prop: typeof B; }`), the checker now resolves the TypeQuery before constructor resolution in `get_type_of_new_expression`. Without this, `new c.prop(1)` produced false TS2351 ("not constructable"). Fix: added `self.resolve_type_query_type(constructor_type)` call in `complex.rs` before the existing pre-resolution chain. (+4 tests: newWithSpread, newWithSpreadES5, newWithSpreadES6 + 1 other)
- **Fixed**: Trailing void parameter optionality — In TypeScript, parameters of type `void` (or unions containing `void`) are implicitly optional when trailing. Modified `arg_count_bounds` in `call_args.rs` to use `rposition` to find the rightmost required non-void param, plus `param_type_contains_void` helper for union checking. (+1-2 tests: callWithMissingVoidUndefinedUnknownAnyInJs)
- **Investigated but deferred**: Generic spread + void inference — `call<TS extends unknown[]>` pattern where void-optionality needs to propagate through generic type parameter inference. Lines 81-83 of callWithMissingVoid.ts. Requires changes to generic inference, not just arg count bounds.
- **Investigated but deferred**: TS2556 — spread arguments not tuple type. ~5 tests in callWithSpread2-5. Requires implementing spread-to-tuple expansion in call argument resolution.
- **Investigated but deferred**: Overload resolution — ~3 tests (overloadResolution, overloadResolutionConstructors, overloadResolutionClassConstructors). Complex multi-signature resolution gaps.
- **Investigated but deferred**: TS2347 vs TS2349 — SubFunc extends Function not callable with type arguments. functionCalls.ts expects TS2347 for `subFunc<number>(0)` but we emit TS2349.

### ~~TS2469 — Symbol operator errors~~ RESOLVED
- Was using wrong diagnostic constant (TS2736 instead of TS2469) for all binary operator symbol checks
- Also missing unary (+, -, ~) and compound (+=) symbol checks entirely
- See "Completed Work" table below

### TS2451 — False positives (7 tests)
- Two patterns:
  - (a) Wrong code choice (TS2451 vs TS2300) for var/let redeclaration conflicts
  - (b) JS file declarations with `@typedef` and late-bound assignments
  - (c) Multi-file `let` redeclaration detection (6 tests)
- **Difficulty**: MEDIUM

---

## Parser Issues

### ~~TS1191 — Import modifier diagnostic position~~ RESOLVED
- Fixed: parser now emits TS1191 at `export` keyword (column 1)

### ~~TS1206 — `decoratorOnUsing.ts`~~ RESOLVED
- Fixed: parser no longer emits TS1206 for `@dec using`; lets TS1134 through instead

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

### ~~TS5071 — resolveJsonModule incompatible with module kind~~ PARTIALLY RESOLVED
- Bundler-implied resolveJsonModule now triggers TS5071 for none/system/umd
- Remaining: 3 TS5095 tests need TS5071 + TS5109, plus syntheticDefaultExports and noBundledEmitFromNodeModules tests
- **Difficulty**: EASY-MEDIUM (remaining cases)

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

### ~~TS2693 — Remaining false positives (9 tests)~~ RESOLVED
- Fixed: TS2693 suppressed when identifier is expression of element access with missing argument

### TS2702 — Namespace-scoped type-as-namespace resolution (remaining tests)
- `errorForUsingPropertyOfTypeAsType01.ts` Tests 1-5: Checker resolves `Foo.bar` inside namespace
  via namespace member lookup (emitting TS2694) instead of the type-as-namespace path
- **Difficulty**: MEDIUM

### ~~TS2661 — Cross-file re-export~~ RESOLVED
- Fixed: non-local export specifier detection using `decl_file_idx` for multi-file and scope-table check for `declare module "m"` contexts
- See "Completed Work" table below

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

### ~~TS2846 — Message text: .js extension suggestion~~ RESOLVED
- Fixed: TS2846 message now includes .js/.mjs/.cjs (or .ts/.mts/.cts with allowImportingTsExtensions)

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

### ~~Intersection freshness propagation~~ RESOLVED
Already uses AND logic for FRESH_LITERAL propagation in intersection merging.

### ~~file_locals flat scope (TS2430)~~ RESOLVED
Binder's `file_locals` scope issue resolved. Unit test confirms correct behavior.

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
| TS5103 | Invalid ignoreDeprecations value (only "5.0" valid; reject "5.5" and "6.0") | +16, +48 tests |
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
| TS2435/TS1035 | Module augmentation in ambient modules: skip TS2435 for string-named parents, skip TS1035 in ambient context | +4 tests |
| TS5103 | Reject ignoreDeprecations "6.0" (not yet valid in tsc 6.0) | +48 tests |
| TS1131 | Emit "Property or signature expected" in parser for invalid interface/type literal members | +tests |
| TS5071 | Bundler-implied resolveJsonModule with none/system/umd module | +1 test |
| TS5102 | Remove incorrect ignoreDeprecations suppression of TS5102 for removed options | +1 test |
| TS2469 | Symbol operator errors: wrong constant (TS2736→TS2469), unary +/-/~, compound +=, solver fast-path fix | +5 tests |
| TS2661 | Non-local export specifier detection (decl_file_idx + module scope table) | +7 tests |
| TS1389 | Reserved word as variable declaration name (TS1389 instead of generic TS1359) | +2 tests |
| TS6133 | ES private names (`#foo`): recognize `#`-prefix as private + reference tracking in private property access and `#name in expr` | +22 tests |
| TS1382/TS1381 | Scanner emits bare `>` / `}` diagnostics in JSX text content | prerequisite |
| TS2354 | Target-aware tslib helper detection (skip __extends at ES2015+) | +2 tests |
| TS1436 | Misplaced decorator in class members: after modifiers (`public @dec prop`) and after property name (`prop @dec`) | +9 tests |
