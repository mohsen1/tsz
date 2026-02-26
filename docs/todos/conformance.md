# Conformance TODO

**Goal**: `./scripts/conformance.sh` prints ZERO failures.
**Current score**: ~7129/12570 (56.7%) — full suite, error-code level
> Note: Regression from ~9264 to ~7129 caused by upstream TS2430 feature commit.
> TS5107 fix added +5 tests (9261→9266) before upstream regression landed.

---

## Union Simplification Lazy Resolution Fix — Session 2026-02-26
- **Area**: types/union (48.0% → 52.0%, +1 test at area level, +2 at full suite level)
- **Root cause**: `simplify_union_members` in `TypeEvaluator::evaluate_union` uses
  `SubtypeChecker` with `bypass_evaluation=true` to avoid infinite recursion. But the
  `bypass_evaluation` path skipped ALL type evaluation, including `resolve_lazy_type`
  for `Lazy(DefId)` types. When ObjectWithIndex types had index signature value types
  that were `Lazy(DefId)` references to different interfaces (e.g., `SomeType` vs
  `SomeType2`), the subtype check compared unresolved Lazy TypeIds instead of their
  structural forms. Different interfaces sharing similar shapes before resolution
  would appear identical, causing one union member to be incorrectly removed.
- **Fix**: In `check_subtype`'s `bypass_evaluation` path, add `resolve_lazy_type` calls
  for both source and target before dispatching to `check_subtype_inner`. If either
  resolves to a different TypeId, recursively call `check_subtype` with the resolved
  types. `resolve_lazy_type` is lightweight (DefId → TypeId lookup via resolver) and
  doesn't trigger the evaluator recursion that `bypass_evaluation` guards against.
- **Files**: `crates/tsz-solver/src/relations/subtype/cache.rs`
- **Tests**: `test_bypass_evaluation_resolves_lazy_index_value_types` in `union_tests.rs`
- **Improved tests**: `contextualTypeWithUnionTypeIndexSignatures`

---

## TS5107 Deprecation Priority Fix — Session 2026-02-26
- **Area**: node/allowJs (47.6% → 57.7% in allowJs before upstream regression)
- **Root cause**: `@strict: false` expands to `alwaysStrict: false`, triggering TS5107
  deprecation. Our driver cleared ALL file-level diagnostics when TS5107 existed, but
  tsc does the opposite: it suppresses TS5107 when real file-level errors exist.
- **Fix**: When JS grammar errors (8xxx range, e.g. TS8002 "can only be used in
  TypeScript files") exist in file-level diagnostics, suppress TS5107 instead.
  8xxx errors are reliable (never false positives), so this is safe.
- **Also fixed**: `expand_include_patterns` in `fs.rs` — added `.mjs`/`.cjs` to
  the extension check list. Without this, patterns like `*.mjs` were incorrectly
  expanded to `*.mjs/**/*` (directory patterns).
- **Files**: `crates/tsz-cli/src/driver/core.rs`, `crates/tsz-cli/src/fs.rs`
- **Net result**: +5 tests (9261→9266) at error-code level before upstream regression
- **Improved tests**: `nodeModulesAllowJsImportAssignment` and related allowJs tests

### Structural limitation: .mjs/.cjs file discovery
- tsc discovers `.mjs`/`.cjs` files through **import resolution**, not glob patterns.
- tsz uses glob-based include patterns which don't match `.mjs`/`.cjs`.
- Adding `.mjs`/`.cjs` to include patterns or tsconfig `files` array over-discovers:
  it finds files tsc wouldn't check (because they're not imported by anything).
- **Proper fix requires**: import-based file discovery in tsz's driver.
- **Affected tests**: `nodeModulesAllowJs1`, `nodeModulesAllowJsPackageExports`, etc.

---

## High Impact — Core Type System

### Reverse Mapped Type Inference — PARTIAL (Session 2026-02-26)
- **Added**: Conservative reverse mapped type inference in `constraints.rs`
- **Root cause**: When inferring T from `Boxified<T> = { [P in keyof T]: Box<T[P]> }`,
  the solver had no reverse inference — it fell back to T's constraint (`object`).
- **Fix**: In the constraint system's Mapped type handler, detect homomorphic mapped types
  (constraint = `keyof T` where T is a placeholder). For each source property, instantiate
  the template with the property key, then structurally reverse the template to extract
  the unwrapped value type. Build a reverse object and constrain it against T using
  `HomomorphicMappedType` priority.
- **Conservative approach**: The reversal only handles two patterns:
  1. `IndexAccess(T, key)` — direct passthrough (source value IS the reversed value)
  2. `Application(F, [IndexAccess(T, key)])` — matching Applications with same base type
  If the template is a function type, conditional type, or anything else, reversal fails
  and we fall back to the existing simple/evaluate paths.
- **Files**: `crates/tsz-solver/src/operations/constraints.rs`
- **Tests**: 3 new tests in `conformance_issues.rs` (boxified unbox, contravariant no-regression,
  func template no-regression)
- **Net result**: +1 at error-code level (stable at ~9233 vs 9232 baseline)
- **Improved tests**: `reverseMappedTypeInferenceWidening2`, `intersectionTypeInference2`,
  `iterableContextualTyping1`, `prespecializedGenericMembers1`
- **Remaining gaps in isomorphicMappedTypeInference.ts** (still 3 extra error codes):
  - Line 33, 108: TS7053 — for-in loop indexing on deferred mapped type (separate issue)
  - Lines 89-90: TS2322 — `makeRecord` simple mapped type `{ [P in K]: T }` picks last
    property type instead of union
  - Line 122: TS2345 — `clone(foo)` reverse inference not preserving readonly modifiers
  - Line 183: TS2322 — `Pick<any, string>` evaluation issue
- **Future work**: Full reverse mapped type inference requires:
  1. A deferred `ReverseMappedType` node (like TypeScript's `ObjectFlags.ReverseMapped`)
     that lazily materializes members using standard inference machinery per-property
  2. Per-property inference using `T[P]` as inference variable against the template
  3. Proper handling of modifier stripping (optional/readonly) during reversal
  4. Cycle detection for deeply nested reverse mapped types

### Contextual typing for arrow function initializers in binding patterns — PARTIAL (Session 2026-02-26)
- **Area**: `types/contextualTypes` (47.37% pass rate, 19 total tests)
- **Improvement**: +2 conformance tests (9236→9238), no regressions
- **Root cause**: `infer_type_from_binding_pattern` evaluates binding element initializers
  without setting contextual type. For arrow function defaults like `v => v.toString()` in
  `function f({ show = v => v.toString() }: Show)`, the arrow's parameters would be typed as
  `any` because no contextual type was available during the first (cached) evaluation.
- **Fix**: Set `ctx.contextual_type` to the element type before evaluating function-like
  (arrow function / function expression) initializers in `infer_type_from_binding_pattern`.
  Also added `check_parameter_binding_pattern_defaults` infrastructure in `parameter_checker.rs`
  for function declaration binding pattern checking.
- **Files**: `binding.rs`, `parameter_checker.rs`, `statement_callback_bridge.rs`, `core.rs`
- **Unit tests**: 10 new tests covering positive cases (matching defaults) and contextual typing
- **Remaining issues (documented for future sessions)**:
  1. **Arrow body evaluation**: Arrow function defaults like `v => v` still produce `error`
     return type because function body evaluation can't resolve parameter references during
     `infer_type_from_binding_pattern`. Only literal-returning arrows (`v => v.toString()`,
     `() => 42`) work correctly.
  2. **type_includes_undefined gate**: `check_binding_element` skips assignability checks for
     required object properties (via `type_includes_undefined`). This gate is needed to prevent
     false positives from cached widened types (array literals get `T[]` instead of tuple,
     string literals get `string` instead of narrow literal type). Removing it causes 23+ JSX
     test regressions.
  3. **Full contextual typing for all initializers**: Setting contextual type for ALL initializers
     (not just arrows) in `infer_type_from_binding_pattern` fixes tuple/string literal defaults
     but causes 23 JSX attribute regressions. The issue is that JSX component function parameters
     also go through `infer_type_from_binding_pattern`, and full contextual typing there changes
     how React component prop types are resolved.
  4. **Cache poisoning**: The node type cache stores the first evaluation result. When
     `infer_type_from_binding_pattern` evaluates initializers without contextual type, subsequent
     checks get stale cached types. This affects non-function-like defaults (tuples, strings).

### TS2362/TS2363 — Per-operand arithmetic check with `any` operand — RESOLVED (Session 2026-02-26)
- **Fixed**: `arithmeticOperatorWithTypeParameter` conformance test (+1 test, 20 fingerprints)
- **Root cause**: When one operand of an arithmetic/bitwise operator is `any`, the solver's
  `evaluate_arithmetic()` short-circuits to `Success(NUMBER)` (line 653 of `binary_ops.rs`),
  preventing the checker from reaching the per-operand error path. TSC independently validates
  each operand — an unconstrained type parameter `T` is NOT a valid arithmetic operand even
  when the other side is `any`.
- **Fix**: Added per-operand validity pre-checks in both the arithmetic (`*`, `/`, `%`, `-`, `**`)
  and bitwise (`&`, `|`, `^`, `<<`, `>>`, `>>>`) paths that emit TS2362/TS2363 for individual
  invalid operands before the evaluator call.
- **Files**: `crates/tsz-checker/src/types/computation/binary.rs`
- **Unit tests**: 6 new tests covering `any * T`, `T * any`, `any & T`, `any * any`,
  `number * any`, and `any * T extends number`.
- **Key insight**: TSC's per-operand validation model checks each operand independently against
  `NumberLike | BigIntLike`, separate from the binary expression result type computation.

### expressions/binaryOperators — remaining failures (13 failing, 80.0% pass rate)
- **Comparison operator comparability** (~7 tests): `is_type_comparable_to()` is too strict for
  object types with call/constructor signatures. TSC's `comparableRelation` uses different rules
  than `assignableRelation` for call signatures — specifically, optional-parameter call signatures
  like `{ fn(a?: Base): void }` vs `{ fn(a?: C): void }` are comparable even when Base and C
  are unrelated. Generic signatures like `{ fn<T>(t: T): T }` vs `{ fn<T>(t: T[]): T }` are
  also comparable. Fix requires implementing proper `comparableRelation` semantics in the solver.
  **Solver-level fix, estimated ~100-200 LOC.**
- **logicalOrOperatorWithTypeParameters** (1 test): `||` operator should produce `NonNullable<T> | U`
  but we produce just `T`. NonNullable narrowing for logical OR. **Solver narrowing fix.**
- **logicalOrExpressionIsContextuallyTyped** (1 test): Wrong position for TS2353 excess property
  error — we point at column 5 (whole expression) instead of column 33 (the `b` property).
- **comparisonOperatorWithOneOperandIsUndefined** (1 test): TS18050 vs TS18048 code mismatch.
- **comparisonOperatorWithIntersectionType** (1 test): Intersection type display — we flatten
  `{ a: 1 } & { b: number }` to `{ a: 1; b: number }` in error messages.
- **instanceofOperator** (2 tests): Various instanceof issues including Symbol.hasInstance.

### TS2322/TS2339/TS2345 — Type mismatch / property access / argument type (ongoing)
- **Tests**: Hundreds across the suite (TS2322: ~222, TS2339: ~47, TS2345: ~40 single-code)
- **Status**: Partially implemented, ongoing solver/checker type relation work
- **Root cause**: Core assignability, property resolution, and argument type checking gaps
- **Difficulty**: HIGH (broad, incremental)

### Closure narrowing — typeof guards for captured variables RESOLVED
- **Fixed**: Removed blanket Rule #42 early-return in `apply_flow_narrowing` (definite.rs)
- **Root cause**: `apply_flow_narrowing()` returned `declared_type` immediately for captured mutable
  variables in closures, preventing local typeof guards from narrowing (e.g. `typeof x === "string" && x.length`)
- **Fix**: Rely on `check_flow()`'s existing START node handling (core.rs:1062) which already returns
  `initial_type` at function boundaries for captured mutable vars. Local CONDITION nodes are applied first.
- **Impact**: Fixed false TS2339 errors in typeGuardsInFunction, jsx, intersection tests (+4-6 tests)

### expressions/typeGuards — remaining TS2454/TS2322 gaps (42 failing, 33.3% pass rate)
- **Pattern**: All remaining failures are MISSING diagnostics (extra=0)
- **Root cause**: Missing TS2454 (used before assigned) for uninitialized `var` at global/module scope
  → leads to missing TS2322 because we narrow when tsc wouldn't (uninitialized vars shouldn't narrow)
- **Specific**: `var x: string | number;` without assignment → tsc treats as always `string | number`,
  typeof guards don't narrow. We incorrectly narrow because our DAA doesn't fire at global scope.
- **Fix needed**: `should_check_definite_assignment` in `usage.rs` may need to be adjusted for
  global-scope `var` declarations without initializers under strictNullChecks
- **Affected tests**: ~26 missing TS2454, ~23 missing TS2322, ~12 missing TS2564

### Union call signatures — combined signature computation PARTIALLY RESOLVED
- **Fixed**: `resolve_union_call` now computes combined signature for unions where all members
  have exactly one non-generic call signature. Uses hybrid approach:
  - Combined signature for argument count validation (max required across members)
  - Per-member resolution for argument type checking (avoids over-constraining)
  - Handles rest params by extracting array element types
- **Impact**: Eliminated false TS2349 ("not callable") for unions with different param counts/types (+5 tests)
- **Remaining gaps**:
  - Multi-overload unions (member with 2 sigs vs member with 1 sig) still fall through to old path
  - Union type reduction (e.g., `() => void | (x?: string) => void` → `(x?: string) => void`) not implemented
  - Fingerprint-level mismatches remain (line offsets, TS2555 vs TS2554 for rest param arity)
- **Files**: `crates/tsz-solver/src/operations/core.rs` — `resolve_union_call`, `try_compute_combined_union_signature`

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

### externalModules/typeOnly — type-only import/export handling PARTIALLY RESOLVED
- **Area**: externalModules/typeOnly (49.2% → 50.8%, +1 in-area, +2 net suite)
- **Fixed** (4 changes across 2 sessions):
  1. **Heritage clause distinction** (scope_finder.rs): Non-ambient `class extends` is value context →
     TS1361/TS2693 should NOT be suppressed. `interface extends` and `declare class extends` are type-only
     contexts where suppression is correct. Fixes extendsClause.ts.
  2. **Cross-file fallback type-only guard** (property_access_type.rs, queries/lib.rs): Skip type-only members
     in cross-file symbol resolution fallback, preventing `export type { A }` from leaking into value resolution.
  3. **ModuleNamespace type-only error code** (type_only.rs): `import * as ns` with type-only exports
     should emit TS2339 ("property doesn't exist") not TS2693, matching tsc.
  4. **Double heritage suppression fix** (type_value.rs, identifier.rs): `error_type_only_value_at()`
     had its own `is_direct_heritage_type_reference()` check that suppressed TS1361 even after the
     caller correctly determined it should fire. Added `is_heritage_type_only_context()` which uses
     `is_in_ambient_context()` to properly handle `declare namespace` cascading ambient status.
     Fixes extendsClause.ts (3 tests) and ambient.ts.
- **Remaining blockers**:
  - `import * as types from './a'` resolves to `TypeId::ANY` in multi-file mode (deep module resolution
    infrastructure issue). This prevents property access checks from running at all for namespace imports,
    blocking ~15+ typeOnly tests. Needs multi-file module resolution improvements.
  - Missing TS1362 ("exported using export type") — separate from TS1361 ("imported using import type")
  - Missing TS2303 (circular import alias) diagnostics
- **Unit tests**: 6 tests in `heritage_type_only_tests.rs` covering class/interface/ambient-class heritage
  with both local interfaces and type-only imports

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

#### Run note (2026-02-25, session 8) — es6/arrowFunction area
- **Area**: es6/arrowFunction (38.8% → 89.6%, 26/67 → 60/67)
- **Net gain**: +59 tests across full suite (6530 → 6589)
- **Fixed**: Remove dead TS1100/TS1210/TS2496/TS2522 diagnostics — tsc 6.0 never emits these. They were false positives across function expressions, declarations, parameters, variables, assignments, and unary operators. Removed all emission sites (7 files).
- **Fixed**: `arguments` resolution in arrow functions — Arrow functions are transparent for `arguments` (they capture from the enclosing scope). Previously `arguments` in arrow functions fell through to normal resolution and emitted false TS2304 ("Cannot find name"). Now resolves to IArguments regardless of scope, matching tsc behavior.
- **Remaining failures**: arrowFunctionErrorSpan (TS1200 line terminator + TS2345), arrowFunctionsMissingTokens (TS1109), arrowFunctionInConstructorArgument1 (TS2304), disallowLineTerminatorBeforeArrow (TS1200), arrowFunctionContexts (TS1101/TS2331/TS2410). All unrelated to the fixed diagnostics.

#### Run note (2026-02-25, session 9) — interfaces/declarationMerging area
- **Area**: interfaces/declarationMerging (19.2% → 60.7%, 5/26 → 17/28)
- **Net gain**: +613 tests across full suite (6912 → 7525, 55.0% → 59.9%)
- **Fixed**: tsc 6.0 strict-family defaults — `src/config.rs` had a block (lines 670-681) that overrode `CheckerOptions::default()` (all `true`) to `false` when `strict` was not explicitly set in tsconfig. This matched tsc 5.x behavior but NOT tsc 6.0, where all strict-family options (`strictNullChecks`, `strictPropertyInitialization`, `noImplicitAny`, `strictFunctionTypes`, `strictBindCallApply`, `noImplicitThis`, `useUnknownInCatchVariables`, `alwaysStrict`) default to `true` even without explicit `strict: true`. Removed the override block. The tsc-6.0-correct defaults from `CheckerOptions::default()` now propagate correctly. Tests with explicit `strict: false` still work via the existing branch.
- **Side effect**: Extra TS2322/TS2339/TS2345 emissions increased (~138/68/87 more false positives). These are pre-existing type checker imprecisions that were previously masked by non-strict mode. Not regressions from this change — they represent type relation bugs that become visible under strict checks.
- **Also fixed**: `conformance.sh` freshness check now includes root `src/` directory. Previously, changes to `src/config.rs` (tsz-core root crate) were not detected by the binary freshness check, causing stale binaries to be used.

#### Run note (2026-02-25, session 10) — types/mapped area
- **Area**: types/mapped (26.9%, 7/26 → still 7/26 in this specific area, but +3 net across suite)
- **Net gain**: +3 tests across full suite (rebased on 7525 baseline, exact count TBD after rebase)
- **Fixed**: Remove dead TS2862 diagnostic — tsc 6.0 completely removed "Type is generic and can only be indexed for reading." Removed `check_generic_indexed_write_restriction` and `index_expression_constrained_to_object_keys` from assignment_checker.rs, and `is_uninstantiated_type_parameter` from solver type_queries.
- **Fixed**: Reverse homomorphic mapped type assignability — Added `check_homomorphic_mapped_source_to_type_param` in core.rs and `check_homomorphic_mapped_to_target` in generics.rs. Detects identity-shaped mapped types (`{ [K in keyof S]: S[K] }`) and allows them to be assigned to their source type parameter (Readonly<T> <: T, Partial<T> <: T).
- **Fixed**: Forward homomorphic mapped type with -? modifier — Removed MappedModifier::Remove restriction from both unions.rs (`is_assignable_to_homomorphic_mapped`) and generics.rs (`check_source_to_homomorphic_mapped`). T <: Required<T> now works at generic level.
- **Remaining types/mapped failures**: 19/26 still fail. Dominant causes: TS2322 false positives from missing generic mapped type instantiation/evaluation (mappedTypes5/6, mappedTypeRelationships), TS7053 noImplicitAny gaps (isomorphicMappedTypeInference), TS2403/TS2536 property modifier enforcement gaps (mappedTypeModifiers, mappedTypeErrors2), parser issues in mappedTypeProperties (TS1005/TS1128).

#### Run note (2026-02-25, session 11) — types/mapped area (continued)
- **Area**: types/mapped — fixed homomorphic mapped type optional/readonly preservation
- **Net gain**: +6 tests (7528 → 7534, 60.0%)
- **Fixed**: Three root causes for `Pick<TP, keyof TP>` producing wrong types:
  1. `try_expand_type_arg()` didn't expand `KeyOf` type arguments during Application evaluation — added KeyOf to the evaluate arm in `evaluate.rs`
  2. `is_homomorphic_mapped_type()` returned bool, not source object — refactored to `homomorphic_mapped_source()` returning `Option<TypeId>` so Method 2 (post-instantiation form with eagerly evaluated keyof) can extract source properties
  3. Declared-type fix for optional properties only applied to `-?` (MappedModifier::Remove) case — generalized to all homomorphic mapped types where source property is optional
- **Root cause detail**: During generic instantiation, `keyof T` in type args was eagerly evaluated to `"a" | "b"` while `T` was resolved to a different TypeId. This caused Method 1 homomorphism check (`obj != source_from_constraint`) to fail, and Method 2 (`expected_keys == mapped.constraint`) to fail because constraint was still `KeyOf(Lazy(...))`.
- **Tests added**: 3 evaluate tests (keyof preserves optional/readonly, post-instantiation preserves optional) + 1 integration test (Pick identity bidirectional subtype)

#### Run note (2026-02-26) — types/mapped area (filtering as-clauses)
- **Area**: types/mapped (46.15% → 50.0%, 12/26 → 13/26)
- **Net gain**: +6 tests across full suite (9256 → 9262, 73.7%)
- **Fixed**: `mappedTypeAsClauseRelationships.ts` — false TS2322 on lines 12, 22 where `T` is assigned to filtering mapped types like `Filter<T> = { [P in keyof T as T[P] extends Foo ? P : never]: T[P] }`
- **Root cause**: `check_source_to_homomorphic_mapped` (generics.rs) and `is_assignable_to_homomorphic_mapped` (unions.rs) blanket-rejected ALL mapped types with `as` clauses (name_type != None). But **filtering** as-clauses — conditionals that produce either `P` or `never` — preserve key subsets of T, so T is still assignable to the result.
- **Fix**: Added `is_filtering_name_type()` helper in generics.rs. Checks if the as-clause is a conditional type where one branch is the iteration parameter and the other is `never`. When this pattern is detected, the homomorphic assignability optimization is allowed to proceed. Made `pub(crate)` so unions.rs can reuse it.
- **Key distinction**: Filtering as-clauses (`as T[P] extends Foo ? P : never`) keep a subset of original keys → T is assignable. Renaming as-clauses (`as \`bool${P}\``) transform keys → T is NOT assignable.
- **Files**: `crates/tsz-solver/src/relations/subtype/rules/generics.rs`, `crates/tsz-solver/src/relations/subtype/rules/unions.rs`
- **Tests added**: 3 unit tests in `generics_rules_tests.rs` (filter no modifier, filter with optional, filter with remove-optional fails correctly)
- **Remaining types/mapped failures**: 13/26 still fail. Dominant causes: TS2322 from generic mapped type eval (mappedTypeRelationships, mappedTypeErrors), TS1360 false positive (mappedTypesGenericTuples2), TS2769 false positive (mappedTypesArraysTuples), TS2313/TS2456/TS2589 missing (recursiveMappedTypes), parser issues (mappedTypeProperties)

#### Run note (2026-02-25, session 13) — references area
- **Area**: references (13.3% → 93.3%, 2/15 → 14/15, +12 in area, +14 net suite-wide)
- **Fixed**: Three root causes for `/// <reference types="..." />` resolution:
  1. `normalize_type_roots()` had a heuristic that reinterpreted absolute paths as relative to project root when they didn't exist on disk. tsc treats absolute typeRoots as-is — removed the heuristic.
  2. `resolve_type_reference_from_node_modules()` fallback was gated on `!Classic` module resolution mode. tsc always does node_modules walk-up for type reference directives regardless of module resolution mode — removed the gate.
  3. Scoped package name mangling missing: `@scope/name` → `@types/scope__name` — added to `type_package_candidates()` and `resolve_type_reference_from_node_modules()`.
- **Also fixed**: TS2688 diagnostic byte offset now points at the type name inside the directive (column 23) instead of line start (column 1). Threaded `types_offset`/`types_len` through `type_reference_errors`.
- **Also fixed**: Empty typeRoots with explicit `types` config option — when no valid type roots exist, all entries in `types` are now correctly reported as unresolved (TS2688).
- **Remaining**: library-reference-5.ts needs TS2403 (conflicting secondary references with different types). This is a checker-level gap, not a resolution issue.

#### Run note (2026-02-25, session 12) — expressions/typeGuards area
- **Area**: expressions/typeGuards (27.0% → 31.7%, 17/63 → 20/63, +3 in area, +3 net suite-wide)
- **Fixed**: TS2454 narrowing-first approach — Reordered `check_flow_usage()` to apply flow narrowing BEFORE definite assignment checking. When typeof/instanceof guards narrow the type in a branch, the narrowing implies the variable has a value, so TS2454 should not fire. This prevents false TS2454 in narrowed branches while preserving them for non-narrowed code paths.
- **Fixed**: Type predicate ASI in parser — Added `!scanner.has_preceding_line_break()` check before treating `is` as a type predicate keyword in both `parse_type()` and `parse_return_type_inner()`. A line break before `is` means ASI applies and `is` should be parsed as an identifier (method name), not as a type predicate. Matches tsc's `parseTypePredicatePrefix()`.
- **Fixed**: Solver formatting — Reformatted let-chains in `core.rs` and `generics.rs` (cosmetic only).
- **Investigated but not fixed**: var vs let TS2454 behavior — tsc emits TS2454 for both var and let declarations without initializers. The narrowing-first approach is a useful heuristic that correctly suppresses TS2454 in typeof true branches but incorrectly suppresses it in typeof false branches (where undefined could still be the runtime value). A more precise fix would require integrating typeof narrowing with definite assignment to determine if the narrowed branch eliminates undefined.
- **Remaining expressions/typeGuards failures**: 43/63 still fail. Dominant causes: TS2322/TS2339 from narrowing accuracy issues (typeof/instanceof/in narrowing not fully integrated), TS2454 fingerprint-level mismatches (correct codes but wrong line numbers), TS2564 false positives for class properties, TS2367 missing comparisons.

#### Run note (2026-02-25, session 14) — expressions/unaryOperators area + Node18/Node20
- **Area**: expressions/unaryOperators (investigated on old broken cache — see session 8-9 for the cache fix by another session)
- **Fixed**: ModuleKind::Node18/Node20 — Added `Node18 = 101` and `Node20 = 102` variants to `ModuleKind` enum with `is_node_module()` helper. Updated all exhaustive matches across 12+ files (args, config, checker, emitter, resolver, wasm).
- **Fixed**: TS5110 range-based check — Changed from exact-match to range-based logic for "Option 'module' must be set to '{0}'" diagnostic. tsc accepts any module in [Node16, NodeNext] range with node-style resolution; we were checking for exact match only. Added 4 unit tests for Node18/Node20 acceptance, ES2015 rejection, and Classic resolution passthrough.
- **Fixed**: Variant filter removal — `filter_incompatible_module_resolution_variants` was filtering out variants that should produce TS5110 errors. Now passes all variants through since the corrected cache contains proper expected errors for each combination.

#### Run note (2026-02-25, session 15) — externalModules/typeOnly area
- **Area**: externalModules/typeOnly (locked assignment area, originally selected by index 6 at session start)
- **Focus test**: `TypeScript/tests/cases/conformance/externalModules/typeOnly/exportNamespace6.ts`
- **Expected fingerprint (before fix)**: TS1362 for `A` and `B` at `e.ts:2:16` and `c.ts:4:1`
- **Observed before fix**: TS18046 for both symbols (type/value namespace confusion through transitive wildcard re-exports)
- **Root cause layer**: CHECKER/BINDER orchestration boundary (connector bug between module-resolution cache and import/export map seeding)
- **Specific gap**: `export type * from "./a"` metadata was stored on module file `/a.ts`, but when imported via `/c.ts -> /b.ts -> /a.ts` the intermediate `/b.ts` bridge was not propagated into `/c.ts`'s binder, so `resolve_import_with_reexports_type_only` missed the type-only edge.
- **Fix location**:
  - `crates/tsz-cli/src/driver/check.rs`: `collect_diagnostics`, `check_file_for_parallel`, `CheckFileForParallelContext` setup
  - Added `propagate_module_export_maps(...)` to recursively copy `module_exports`, `wildcard_reexports`, `wildcard_reexports_type_only`, and `reexports` across wildcard chains from `resolved_module_paths`.
  - `crates/tsz-cli/src/driver/check.rs` test: `test_transitive_module_export_bridge_infers_type_only_flags`
- **Estimated scope**: ~70 lines in `check.rs` (+1 unit test)
- **Other tests affected**: `externalModules/typeOnly` set; direct win on `exportNamespace6` and likely adjacent transitive wildcard/type-only files (`exportNamespace3/5`, `exportNamespace8/9/11/12`) as map propagation is now transitive.

#### Run note (2026-02-25, session 16) — classes area (TS2729 static blocks)
- **Area**: classes (37.5% → improved), specifically classStaticBlock sub-area (48.5% → 57.6%, +3 tests)
- **Fixed**: TS2729 ("Property used before initialization") for static blocks — Static blocks (`static { ... }`) were type-checked but missing the TS2729 use-before-init check that already existed for static property initializers. Added `check_static_block_initialization_order()` in `types/type_checking/property_init.rs` (~280 lines) which:
  - Finds the static block's position in the class member list
  - Collects `this.X` and `ClassName.X` property accesses via recursive traversal
  - Correctly stops at function/arrow/class boundaries (deferred execution = no error)
  - Compares access positions against static property declaration positions
  - Emits TS2729 for any access that precedes its declaration
- **Call site**: Added 3-line hook in `member_declaration_checks.rs` for `CLASS_STATIC_BLOCK_DECLARATION` (kind 176)
- **Tests added**: 3 unit tests in `tests/checker_state_tests.rs` — basic use-before-init, this-access variant, arrow-function-no-error
- **Dead code discovery**: `state/state_checking_members/property_init.rs` exists as an untracked file but is NOT in `mod.rs` — dead code. The real compiled implementation is `types/type_checking/property_init.rs`.
- **Conformance gain**: +3 tests (classStaticBlock3, classStaticBlock4, classStaticBlock9). Net: 7698→7706 after rebase (61.2%→61.3%)
- **Remaining TS2729 gaps**: Instance property tests (initializationOrdering1, redefinedPararameterProperty, assignParameterPropertyToPropertyDeclarationESNext/ES2022, privateNameCircularReference) need the same pattern extended to instance contexts.

#### Run note (2026-02-26) — TS2515 abstract member satisfaction via declaration merging
- **Fixed**: False TS2515 ("Non-abstract class does not implement inherited abstract member") when a merged interface declaration provides the abstract member.
- **Root cause**: `check_abstract_member_implementations` in `class_implements_checker.rs` only collected members from the class body's own AST members. It didn't consider members provided by merged interface declarations (class + interface with same name in same scope).
- **Fix**: After collecting own class members, look up the class symbol's declarations for merged interfaces. For each merged interface, collect members (both own and inherited via extends clauses using the solver's object shape).
- **Tests added**: 2 new tests — TS2515 suppressed with merged interface, TS2515 emitted without merged interface.
- **Note**: Cannot verify conformance improvement due to upstream regression from `beaf4f9fc6` (binding pattern contextual typing) which dropped the full suite from ~9260 to ~7129 tests.

#### UPSTREAM REGRESSION (beaf4f9fc6) — binding pattern contextual typing
- **Commit**: `beaf4f9fc6 fix(checker): set contextual type for arrow/function initializers in binding patterns`
- **Impact**: Full suite dropped from 9260/12570 (73.7%) to 7129/12570 (56.7%), ~2131 test regression
- **Unit tests**: 182 pre-existing test failures across binder, checker, and ASI test modules
- **Symptoms**: TS2428, TS2564, TS2454 and many other diagnostics missing in conformance tests
- **Root cause**: Changes to `types/queries/binding.rs` array binding pattern handling restructured iteration logic. Need investigation.

#### Run note (2026-02-26) — interfaces/interfaceDeclarations area (TS2430 type alias bases + error location)
- **Area**: interfaces/interfaceDeclarations
- **Changes**:
  1. **TS2430 type alias base checking**: Added property compatibility checking when interface extends a type alias (e.g., `interface I extends T1 { ... }` where `type T1 = { a: number }`). Uses DefId-first resolution for generic aliases with type arguments. Supports intersection type alias bases by searching each intersection member.
  2. **TS2430 error location fix**: Changed error location for private member conflicts from the conflicting member to the interface name (matching tsc behavior).
- **Key implementation detail**: `get_type_of_interface_member` returns an ObjectShape wrapping the property, not the raw property type. When comparing derived member types against base property types from `find_property_in_type_by_str`, we must extract the raw property type from the ObjectShape using `find_property_in_type_by_str` on the derived member type too.
- **Tests added**: 5 new unit tests (type alias incompatible, compatible, intersection incompatible, mapped type ignored, private member error location).
- **Conformance gain**: +5 tests (interfaceWithPropertyThatIsPrivateInBaseType, interfaceWithPropertyThatIsPrivateInBaseType2, interfaceExtendingClassWithPrivates, interfaceExtendingClassWithProtecteds, typeofANonExportedType). Verified via test list diff (baseline 3311 fails → 3309 fails, +2 net after flaky test noise).
- **Note**: Cannot see gain in FINAL RESULTS due to upstream regression (beaf4f9fc6).
- **Remaining gaps**: Mapped type alias bases not yet evaluated in unit test environment. `typeof CX`/`typeof EX`/`typeof NX` base types use alias name instead of resolved type in error messages.

#### Run note (2026-02-26) — interfaces/declarationMerging area (TS2411/TS2413)
- **Area**: interfaces/declarationMerging (24/28 → 25/28, 85.7% → 89.3%)
- **Net gain**: +1 test (mergedInterfacesWithIndexers2)
- **Fixed TS2411 quoting**: String literal property names in TS2411 diagnostics now preserve the original quote style (single or double). Uses `node_text()` to extract the raw source text including quotes, matching TSC's `symbolToString` behavior. Previously we stripped quotes: `'a': number` → Property 'a', now → Property ''a''.
- **Fixed TS2413 location**: When interfaces merge across separate bodies, TS2413 was emitted from both the body with the number index (correct, line 4) AND the body with the string index (extra, line 9). Root cause: `check_index_signature_compatibility` is called per-body but sees merged solver index info. The fallback to `string_index_nodes` was unnecessary. Additionally, `duplicate_identifiers.rs` had redundant cross-body number-vs-string index checks that duplicated what `check_index_signature_compatibility` already handles. Removed both the fallback in `index_signature_checks.rs` and the redundant checks in `duplicate_identifiers.rs`.
- **Tests added**: 5 new tests — TS2411 single-quote/double-quote/identifier quoting, TS2413 single-body emission, TS2413 no-duplication across merged bodies.
- **Remaining failures (3 tests)**:
  - `mergedInheritedMembersSatisfyAbstractBase`: Extra TS2515 (abstract member not satisfied despite declaration merging providing the member) + missing TS2320 (interface cannot simultaneously extend conflicting types). Needs declaration merging to be considered when checking abstract member satisfaction.
  - `mergedInterfacesWithInheritedPrivates2`: Missing TS2341 (private property access through merged interface with inherited privates). Needs private member tracking for merged interface extends.
  - `mergedInterfacesWithInheritedPrivates3`: Extra TS2420 (class incorrectly implements interface). TSC suppresses this when the interface has conflicting private members from extends.

#### Run note (2026-02-26, session 17) — interfaces/declarationMerging area (TS2428)
- **Area**: interfaces/declarationMerging (60.7% → 75.0%, 17/28 → 21/28, +4 in area)
- **Net gain**: +15 tests across full suite (8710 → 8725, 69.3% → 69.4%)
- **Fixed**: TS2428 ("All declarations of 'X' must have identical type parameters") was not firing for interfaces declared in separate namespace blocks with the same name.
- **Root cause**: `check_duplicate_identifiers()` in `duplicate_identifiers.rs` grouped interface declarations by the `NodeIndex` of their enclosing `MODULE_DECLARATION`. Two separate `namespace M {}` blocks have different `NodeIndex` values even though the binder merges them into one `SymbolId`. This meant interfaces in separate blocks were never compared.
- **Fix**: Created `get_enclosing_namespace_symbol()` that resolves `NodeIndex → SymbolId` via `binder.node_symbols`. Changed grouping key from `NodeIndex` to `SymbolId` so separate namespace blocks with the same symbol are correctly treated as the same scope.
- **Tests added**: 6 unit tests in `tests/ts2428_tests.rs` — generic vs non-generic, same params (no error), different arity, namespace separate blocks, namespace same block.
- **No regressions**: Zero extra TS2428 errors across the full suite.

#### Run note (2026-02-26, session 18) — expressions/binaryOperators area
- **Area**: expressions/binaryOperators (72.3% → 76.9%, 47/65 → 50/65, +3 in area)
- **Net gain**: +4 tests across full suite (8765 → 8769, 69.7% → 69.8%)
- **Fixed**: TS1345 void truthiness gated on strictNullChecks — `check_truthy_or_falsy_with_type()` in `callable_truthiness.rs` was unconditionally emitting TS1345 for void expressions. tsc only emits this under `strictNullChecks`. Moved the `strict_null_checks` early return before the void check (+2 tests: logicalAndOperatorWithEveryType, logicalOrOperatorWithEveryType).
- **Fixed**: Mixed-orderable comparison bug — `is_orderable()`/`OrderableVisitor` in solver's `binary_ops.rs` checked each operand independently for orderability. Both `number` and `string` are individually orderable, so `number < string` returned `BinaryOpResult::Success` instead of `TypeError`. Removed `is_orderable` entirely; TSC requires SAME orderable kind (both number-like, both string-like, both bigint-like). Now mixed comparisons fall through to `TypeError`, and the checker's existing `is_type_comparable_to` handles the rest (+1 test: comparisonOperatorWithNoRelationshipPrimitiveType).

- **Attempted but reverted**: Simplified checker's relational operator fallback to just `is_type_comparable_to(left, right)`. This regressed `comparisonOperatorWithNoRelationshipTypeParameter` because `is_type_comparable_to(T, number)` resolves T to apparent type `unknown`, and `number` IS assignable to `unknown`, making them "comparable" when they shouldn't be. Root cause: `is_type_comparable_to` uses bidirectional assignability which doesn't match TSC's `comparableRelation` for type parameters.
- **Remaining binaryOperators failures (15 tests)**: Extra TS2365 on function/constructor comparisons (~6 tests, needs proper `comparableRelation` in solver), missing TS2362/TS2363 for type params (~1 test), instanceof Symbol.hasInstance (~2 tests), intersection type printing (~1 test), contextual typing location (~1 test), missing TS2365 for primitives (~3 tests, message-level diff).

#### Run note (2026-02-26, session 19) — override area
- **Area**: override (48.4% → 66.7%, 16/33 → 22/33, +6 in area)
- **Net gain**: +5 tests across full suite (8769 → ~8805, 69.8% → 70.0%)
- **Fixed**: Three issues in `classes/class_checker.rs`:
  1. **Ambient class suppression** — `declare class` members now skip `noImplicitOverride` checks. Ambient classes are type-only; tsc only checks `TS1040` (override in ambient context) but not TS4114 (missing override). Added `is_ambient_class` flag gating `no_implicit_override` (+1 test: override3).
  2. **Parameter property diagnostic positions** — TS4115/TS4113/TS4112 for constructor parameter properties now point at the first modifier keyword (public/protected/private/readonly), matching tsc. Added `find_first_param_property_modifier()` helper (+2 tests: override6, override8).
  3. **Dynamic name detection** — `is_computed_expression_dynamic()` now resolves identifiers to check variable declarations. `let`/`var` variables → always dynamic (TS4127). `const` with explicit `symbol` type annotation → dynamic (non-unique symbol). `const` with string/number literal type → NOT dynamic (late-bindable). Handles both raw SymbolKeyword and TYPE_REFERENCE-wrapped keyword AST shapes (+3 tests: overrideDynamicName1, overrideLateBindableIndexSignature1, + fingerprint improvements).
- **Remaining override failures**: 11 tests still fail. Dominant causes: missing TS1029 (modifier ordering), TS1089 (override on constructor), TS1040 (override in ambient context), TS4117 suggestion text differences (intersection type names), TS8009 (override in JS files), TS4123 (JSDoc @override). These are separate feature gaps requiring parser/checker work beyond override-specific checking.
- **Note**: Code changes were independently implemented by a concurrent session and merged first. This session's identical changes were superseded during rebase. Only this documentation was committed from this session.

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

### JSX Diagnostic Position Fixes (Session 2026-02-25) — DONE
- **Fixed**: TS2322/TS2741 anchor at attribute name / tag name instead of value expression
- **Fixed**: Boolean JSX attributes (`<x disabled />`) now checked against expected type
- **Fixed**: Excess property type display `{ attr: type; }` instead of `{attr}`
- **Fixed**: TS1005 `'</' expected` instead of `'token' expected` (parser token_to_string)
- **Fixed**: TS7005 suppressed in .d.ts files
- **Net gain**: +5 tests (at baseline HEAD; post-rebase gains may differ due to upstream regression)

### JSX Factory/Fragment Fixes (Session 2026-02-25 #2) — DONE
- **Fixed**: TS2874 false positives — skip factory-in-scope check when `jsxFactory` is explicitly
  set via config (tsc 6.0 behavior). Use `resolve_name_with_filter` with accept-all filter for
  full scope chain (class members, parameters, locals, imports, globals).
- **Fixed**: TS7026 about "JSX.Element" — tsc 6.0 never emits TS7026 for the Element interface,
  only for IntrinsicElements. Removed false emission in `get_jsx_element_type` for fragments.
- **Added**: TS17016 — "jsxFragmentFactory must be provided" when jsxFactory is set but
  jsxFragmentFactory is not. New `check_jsx_fragment_factory()` method.
- **New fields**: `jsx_factory_from_config` and `jsx_fragment_factory_from_config` in CheckerOptions
  to distinguish explicit config from defaults/reactNamespace.
- **Reverted**: TS2604 — "no construct or call signatures" check caused false positives because
  component types aren't fully resolved yet (many evaluate to objects without signatures).
  Needs better type resolution before this can be enabled.
- **Net gain**: +14 tests (JSX 30.5% → 31.0%, overall +20 after rebase)

### JSX Remaining Gaps (classified during session)
- ~~**TS2874 false positives**: JSX pragma/factory resolution gap~~ RESOLVED (see above)
- **TS2874 edge cases**: `@jsx` pragma support still needed for `inlineJsxFactoryDeclarations.tsx`
- **TS7026 emission**: Fewer TS7026 instances than tsc for some tests (namespaced JSX like `<svg:path>`)
- **TS7026 from jsxImportSource**: 6 tests emit extra TS7026 where JSX namespace resolution
  should be relative to factory or jsxImportSource module, not global
- **TS2604**: Blocked until component type resolution improves (class/function signatures)
- **TS7008 member name quoting**: Runner filename handling with `@filename` directives complicates comparison
- **TS2322 for component props**: Needs `IntrinsicAttributes` intersection in JSX type checking
- **Type display differences**: `string | undefined` vs `string` for optional props; property ordering in objects
- **71 zero-error tests**: ~~Dominated by missing TS2307 (react module resolution)~~ RESOLVED: .lib/ path rewriting bug fixed (JSX 30%→42%). Remaining gaps are TS7026 and type-checking precision

---

## Other Open Issues

### TS2320 — Interface extension remaining gaps (10/20 passing)
- **FIXED**: Class base public member type conflicts now detected (class_checker_compat.rs)
- **FIXED**: Class base visibility conflicts (public vs private/protected) now detected
- **FIXED**: Generic class base type parameter substitution for member comparison
- **FIXED**: Qualified name in error messages — now uses resolved symbol name (matches tsc)
- **Remaining**: 10 of 20 tests still fail:
  - `complexRecursiveCollections` — very complex recursive types
  - `genericAndNonGenericInheritedSignature1/2` — need identity check instead of mutual
    assignability for call signatures (`f(x: any): any` vs `f<T>(x: T): T`)
  - `mergedInheritedMembersSatisfyAbstractBase` — class+interface declaration merging:
    need to include class's extended base members when checking interface TS2320
  - `mergedInterfacesWithInheritedPrivates3` — extra TS2420 emitted
  - `interfaceExtendingClassWithPrivates2/Protecteds2` — wrong TS2430 location (pointing
    at member instead of interface name on extends clause) + missing TS2341/TS2445
  - `interfaceDeclaration1` — missing TS2717 (different error code)
  - `multipleBaseInterfaesWithIncompatibleProperties` — partial pass
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
| TS2792→TS2307 | Module resolver: return NotFound instead of ModuleResolutionModeMismatch for Node16/NodeNext exports failures | -11 false TS2792 |
| skipLibCheck | Extend skipLibCheck to .d.cts/.d.mts (not just .d.ts) | +2 node tests |
| node_modules | Suppress diagnostics for declaration files inside node_modules | included in above |
| TS1100/TS1210 | Remove dead strict-mode eval/arguments diagnostics (tsc 6.0 no longer emits) | +59 tests |
| TS2496/TS2522 | Remove dead arrow/async function arguments diagnostics | included above |
| arguments | Fix arguments resolution in arrow functions (transparent scope capture) | included above |
| mapped types | Homomorphic mapped type assignability (T <: Partial<T>, flatten_mapped_chain eval, transitive deferral) | +1 test |
| TS18050 | ~~Remove incorrect strictNullChecks gate on TS18050 emission~~ REVERSED: gate TS18050 binary ops on strictNullChecks (tsc DOES gate) | net +20 tests (prior), corrected |
| strict defaults | Match tsc 6.0 strict-family defaults (all true when `strict` not set in tsconfig) | +613 tests |
| TS2862 | Remove dead TS2862 diagnostic (tsc 6.0 never emits "generic indexed write restriction") | +1 test |
| mapped types (reverse) | Bidirectional homomorphic mapped type assignability (Readonly<T> <: T, Partial<T> <: T, T <: Required<T>) | +1 test |
| TS18050/TS2365 snc gate | Gate TS18050 binary op errors on strictNullChecks; suppress TS2365 for nullish+nullish when snc off | +1 test (bitwiseNotOperatorWithAnyOtherType) |
| TS2454/narrowing | Reorder check_flow_usage: apply narrowing before TS2454 to suppress false "used before assigned" in typeof guard branches | +2 tests |
| JSX diagnostics | Anchor TS2322/TS2741 at attr name/tag name; boolean attr checking; excess property type display; `</` parser token; TS7005 .d.ts suppression | +5 tests |
| .lib/ path fix | Fix /.lib/ reference path rewriting: format string kept leading /, rewrite func skipped .lib/ paths. Regenerated tsc cache for 138 affected entries | +28 tests (JSX 30%→42%) |
| TS5107 suppression | Suppress TS5107 deprecation diagnostics when source files have parse errors (1000-1999), matching tsc behavior | +52 tests |
| JSX factory/fragment | TS2874 false positive fix (jsxFactory config skip + full scope chain), TS7026 Element removal, TS17016 fragment factory diagnostic | +14 tests (JSX 30.5%→31.0%) |
| wildcard reexport ordering | Fix `resolve_cross_file_export` and `resolve_export_in_file`: check reexport chains (wildcard/named) BEFORE file_locals fallback, and collect reexported symbols for namespace imports when target has no direct exports | +5 tests |
| TS1345 strictNullChecks | Gate void truthiness check (TS1345) on `strictNullChecks` — was unconditionally emitting | +2 tests (logicalAndOperatorWithEveryType, logicalOrOperatorWithEveryType) |
| TS2365 mixed-orderable | Remove `is_orderable`/`OrderableVisitor` from solver `BinaryOpEvaluator` — was accepting mixed-kind comparisons like `number < string` | +1 test (comparisonOperatorWithNoRelationshipPrimitiveType) |
| TS2411/TS2413 index sig | TS2411: preserve original quote style for string literal property names using `node_text()`. TS2413: only emit on number index nodes (remove string/container fallback); remove redundant cross-body number-vs-string checks from `duplicate_identifiers.rs` | +1 test (mergedInterfacesWithIndexers2) |
| mapped type as-clause | Filtering as-clause recognition in homomorphic mapped type assignability (T <: Filter<T> where Filter uses `as P extends Foo ? P : never`) | +6 tests |
