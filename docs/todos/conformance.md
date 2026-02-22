# Conformance Issues — Investigated but Deferred

## TS5024 — Compiler option requires a value of type (Implemented)

**Status**: Implemented basic validation. 10 new tests passing.

### Remaining issues

- **Fingerprint line number mismatch**: ~19 additional tests match at error-code level
  but fail fingerprint comparison because JSON key ordering in the generated tsconfig
  differs from the cache generator. The cache generator (JS) preserves insertion order
  via JavaScript Object property ordering. Our runner uses `HashMap` which loses order.
  Partial fix: added `option_order` tracking from test parser to `convert_options_to_tsconfig`,
  but strict-family defaults are appended after directive options, shifting line numbers.
  - Fix: Either stop adding strict defaults to tsconfig (let tsz handle internally) or
    match the cache generator's exact tsconfig format including strict defaults placement.

## TS5025: Canonical option name mapping (Fixed)

**Status**: Fixed. Added 53 missing entries to `canonical_option_name()` across
all 3 Rust copies (tsz_wrapper.rs, generate-tsc-cache.rs, generate-tsc-cache-tsserver.rs)
and regenerated the TSC cache. This eliminated 262 false TS5025 diagnostic entries
that were caused by lowercase option names in tsconfig.json not being mapped to
canonical camelCase. Result: +23 tests passing.

### Remaining fingerprint-level TS5025 mismatches
Some tests still fail at the fingerprint level due to tsconfig property ordering
differences affecting TS5025 diagnostic line/column positions. These are not
error-code-level failures.

## TS7006: Contextual typing gaps causing spurious implicit-any errors (16 tests)

**Error code:** TS7006 ("Parameter 'x' implicitly has an 'any' type")
**Test files:** e.g., `contextualParameterAndSelfReferentialConstraint1.ts`,
`contextuallyTypedParametersWithInitializers2.ts`, `inferringAnyFunctionType3.ts`
**Reason:** tsz fails to contextually type parameters in some generic/mapped-type
scenarios. Even with `noImplicitAny: true`, tsc resolves these parameter types from
context and does not emit TS7006. Requires solver-level contextual typing improvements.

## TS2305: Module name quoting difference (7 tests)

**Error code:** TS2305 ("Module '…' has no exported member '…'")
**Test files:** Tests with `Module './b'` vs tsc's `Module '"./b"'`
**Reason:** Our diagnostic message formats module specifiers without extra quotes
around the module name. tsc includes the source-level quotes in the message.
Simple string formatting fix in checker diagnostic message construction.

## TS1191: Import modifier diagnostic position (8 tests)

**Error code:** TS1191 ("An import declaration cannot have modifiers")
**Test files:** Tests with `export import …` patterns
**Reason:** Our parser emits TS1191 at the `import` keyword position (column 8)
instead of the `export` keyword position (column 1). The diagnostic span should
start at the beginning of the statement.

## TS5057 — Cannot find a tsconfig.json file at the specified directory
- **Tests**: 52 failing tests, 22 would pass
- **Reason**: Requires tsconfig project-reference and composite build support which is
  not yet implemented. These tests expect `tsc --build` behavior.

## TS5095 — Option 'bundler' can only be used when 'module' is set to...
- **Tests**: 26 failing tests, 16 would pass
- **Reason**: Requires moduleResolution validation against module kind constraints.
  Needs `resolve_compiler_options` to validate moduleResolution/module compatibility
  and emit TS5095 diagnostics.

## TS2304 — Cannot find name (extra emissions)
- **Tests**: 204 tests have extra TS2304, 25 tests only have extra TS2304
- **Reason**: tsz emits TS2304 for identifiers that should be resolved from lib types
  or that tsc resolves through more advanced module resolution. Reducing false positives
  requires broader improvements to lib file resolution and module resolution accuracy.

## TS2322/TS2339/TS2345 — Type mismatch/property access (partial)
- **Reason**: These are the core type-checking error codes. Improvements are ongoing
  in solver/checker. Each individual fix is complex and requires careful tsc parity analysis.

## TS2300 — Duplicate identifier false positives (parameter+var, fixed)
- **Tests**: 24 `arguments`-related false positives eliminated; 3 conformance tests now pass
- **Root cause**: `resolve_duplicate_decl_node` did not recognize PARAMETER nodes, so
  they resolved to the parent FunctionDeclaration and got FUNCTION flags. This made
  parameter+var pairs appear as FUNCTION vs FUNCTION_SCOPED_VARIABLE conflicts.
- **Fix**: Added PARAMETER to recognized node kinds and returned FUNCTION_SCOPED_VARIABLE
  from `declaration_symbol_flags`.
- **Remaining TS2300 issues**: `let`/`const` redeclarations conflicting with parameters
  in the same block scope are not yet detected (pre-existing gap, separate from this fix).

## TS1206 — ES decorators on class expressions (Fixed)

**Status**: Fixed. Removed unconditional TS1206 from parser for class expression
decorators and added `@dec` handling in `export default` path. ES decorators
(TC39 Stage 3) are valid on class expressions in TypeScript 5.0+. Result: +19
tests passing (offset 6000 slice: 3665→3684).

### Remaining TS1206 issues
- `decoratorOnUsing.ts` — `@dec using` still emits TS1206 from parser
  `parse_decorated_declaration` (UsingKeyword branch). TSC produces TS1134
  instead. Needs parser to unify decorator-on-invalid-declaration error codes.
- With `--experimentalDecorators`, class expression decorators should emit
  TS1206 from the checker (not parser). No tests currently exercise this path.

## Deferred issues from this run (not fixed)

- **TS2300**: `TypeScript/tests/cases/compiler/collisionArgumentsArrowFunctions.ts` — remaining failure is TS5025 (compiler option casing), not TS2300.
- **TS2300**: `TypeScript/tests/cases/compiler/collisionArgumentsInterfaceMembers.ts` — remaining failure is TS5025.
- **TS5057**: `TypeScript/tests/cases/compiler/commonSourceDir1.ts` — requires project/tsconfig discovery and compiler option plumbing that is not yet wired into the current checker flow.
- **TS5095**: `TypeScript/tests/cases/compiler/declarationEmitBundleWithAmbientReferences.ts` — requires moduleResolution validation against module-kind constraints, which is still outside current scope.
- **TS2322 (62 missing)**: Many tests still miss TS2322 assignability errors — ongoing solver/checker type relation work.
- **TS2339 (52 missing)**: Property access errors not yet emitted for union-typed or intersection-typed values in some cases.
- **TS2322/TS2339 (broad regression slice)**: `TypeScript/tests/cases/compiler/abstractClassUnionInstantiation.ts` still needs solver/checker assignability and narrowing alignment before this cycle; fixing in this pass would be a broad refactor.
- **TS2304 (57 extra)**: Over-emission of "cannot find name" — requires broader lib resolution and module resolution improvements.
- **TS1202 (fixed)**: False TS1202/TS1203 when module was a computed default (not explicitly set). Fixed by adding `module_explicitly_set` flag. +29 tests passing.
- **TS2322 (focused, unchecked)**: `TypeScript/tests/cases/compiler/checkJsObjectLiteralHasCheckedKeyof.ts` — currently reports `Type 'string'` instead of literal union mismatch for checked JS `@ts-check` with `keyof typeof obj`. Needs deeper JSDoc/`keyof` context handling in checker/solver assignability flow.
- **TS5102 (fixed)**: Implemented "Option has been removed" diagnostic for TS 5.0-deprecated/5.5-removed options (noImplicitUseStrict, keyofStringsOnly, suppressExcessPropertyErrors, suppressImplicitAnyIndexErrors, noStrictGenericChecks, charset, out, importsNotUsedAsValues, preserveValueImports). +4 tests passing in offset 6000 slice (3737→3741). Remaining TS5102 failures are in tests that have additional unimplemented error codes (verbatimModuleSyntax compat checks, multi-file module resolution).

## TS5102 — Remaining failures (investigated, deferred)

- **verbatimModuleSyntaxCompat*.ts** (4 tests): Need verbatimModuleSyntax validation logic (TS1286, TS1484) beyond just the removed-option diagnostic.
- **preserveValueImports.ts**, **importsNotUsedAsValues_error.ts**: Have additional TS1484/TS2305 codes that we don't yet emit.
- **nonPrimitiveIndexingWithForInSupressError.ts**: Has additional TS2304 (lib type resolution gap).
- **keyofDoesntContainSymbols.ts**: Expects TS5102 + TS2345. TS5102 now emitted but TS2345 requires `keyofStringsOnly` semantic behavior changes.

## TS5095 — Option 'bundler' requires compatible module kind (Implemented, updated)

**Status**: Implemented. +15 tests initially (3843→3858), then +4 more from node module fix.
**Error code:** TS5095 ("Option 'bundler' can only be used when 'module' is set to 'preserve' or to 'es2015' or later.")
**Fix**: Added validation in `parse_tsconfig_with_diagnostics` (src/config.rs) that emits TS5095
when `moduleResolution: "bundler"` is combined with an incompatible module kind (commonjs, amd,
umd, system, none). Also handles implicit module default from target.

**Update**: Added `node16`, `node18`, `node20`, `nodenext` as valid module kinds for bundler
resolution (they are ES2015+ compatible). Also added bundler compatibility filtering to
`filter_incompatible_module_resolution_variants` in the conformance runner to prevent false
TS5095 from multi-module variant expansion (e.g., `@module: esnext, commonjs, amd` where the
cache only tests the first value). +4 tests (3935→3939).

### Remaining TS5095 failures
- **requireOfJsonFileWithModuleNodeResolutionEmit{None,System,Umd}.ts** (3 tests): Expect both TS5095 AND TS5071 (`--resolveJsonModule` incompatible with none/system/umd). TS5071 not yet implemented.
- **syntheticDefaultExportsWithDynamicImports.ts**, **noBundledEmitFromNodeModules.ts**: Also need TS5071.
- **bundlerOptionsCompat.ts**: Needs TS5095 + TS5109.
- **pathMappingBasedModuleResolution3_node.ts**: Needs TS5095 + TS18003.

### Message text note
The diagnostic message in `diagnosticMessages.json` (data.rs template) includes "commonjs" in the
allowed list, but actual tsc 6.0 output says "preserve' or to 'es2015' or later" without "commonjs".
We use the exact tsc output string for fingerprint-level conformance.

## TS5103 — Invalid value for '--ignoreDeprecations' (Implemented)

**Status**: Implemented. +16 tests passing in first 6000 slice (3857→3873), +48 total.
**Error code:** TS5103 ("Invalid value for '--ignoreDeprecations'.")
**Fix**: Added validation in `parse_tsconfig_with_diagnostics` (src/config.rs) that emits TS5103
when `ignoreDeprecations` is set to any value other than `"5.0"`. Also added early return in
`compile_inner` (driver.rs) when TS5103 is present, matching TSC's behavior of halting
compilation on invalid `ignoreDeprecations` values.

### Key finding
TSC 6.0-dev only accepts `"5.0"` as a valid `ignoreDeprecations` value. Even though TS5107
messages suggest `"6.0"` to suppress newer deprecations, `"6.0"` is not yet a valid value.
All 48 conformance tests used `// @ignoreDeprecations: 6.0` which TSC rejects with TS5103.

## TS18003 — No inputs found in config file (Fixed, partial)

**Status**: Fixed fingerprint alignment. +36 tests passing (7602→7638).
**Error code:** TS18003 ("No inputs were found in config file 'tsconfig.json'...")
**Fix**: Two changes:
1. Driver: emit TS18003 with empty file and position 0,0 (matching tsc's format)
2. Conformance runner: unified include patterns to always use
   `["*.ts","*.tsx","*.js","*.jsx","**/*.ts","**/*.tsx","**/*.js","**/*.jsx"]`
   matching the cache generator exactly. File discovery still respects `allowJs`
   via extension filtering in `discover_ts_files`.

### Remaining TS18003 failures (34 tests)
- Tests with `@Filename: A:/foo/bar.ts` (Windows-style absolute paths) — our
  temp directory writes these as subdirectories, which the include patterns match.
  tsc's virtual filesystem treats them as a separate drive root where include
  patterns don't match, so tsc emits TS18003 but we find and compile the files.
- Tests with `node_modules/@types` structures — our compiler discovers @types
  files as source files instead of treating them as type-only references.

## TS5110 — Module must match moduleResolution (Implemented, net-zero)

**Status**: Implemented. Net-zero conformance impact (correct behavior but fingerprint
positions don't match for affected tests).
**Error code:** TS5110 ("Option 'module' must be set to '{0}' when option 'moduleResolution' is set to '{1}'.")
**Fix**: Added validation in `parse_tsconfig_with_diagnostics` (src/config.rs) that emits TS5110
when `moduleResolution` is node16/nodenext but `module` is not. Skips check when module
is not explicitly set (tsc defaults it automatically).

### Why net-zero
Tests that trigger TS5110 also have the diagnostic at a different line/column position
than the cache expects. `find_value_offset_in_source` returns 0 for the "module" key in
the generated tsconfig because the pretty-printed JSON has different offsets.

## TS2454 — Variable used before assignment (Fixed)

**Status**: Fixed. +14 tests passing (3882→3896 in first 6000 slice, 65.0%).
**Error code:** TS2454 ("Variable 'x' is used before being assigned.")
**Root cause**: `is_definitely_assigned_at()` in `flow_analysis_usage.rs` returned `true`
(assumes assigned) when `get_node_flow(idx)` found no flow node for the identifier reference.
The binder only records flow nodes for statements and declarations, NOT for individual
identifier references within expressions. So `var a: Bar; a()` — the `a` identifier node
had no flow node, and the function assumed it was definitely assigned.
**Fix**: Added parent-walk fallback (same pattern used by `apply_flow_narrowing()`) to find
the nearest ancestor node with a flow node. Falls through to `true` only when no ancestor
has flow info either (rare edge case for ambient/external contexts).
**Tests affected**: 128 tests in first-6000 slice have ONLY TS2454 as expected error.
286 total tests across full suite expect only TS2454. Net +14 in first 6000 (some tests
also have other missing/extra error codes that prevent them from fully passing).

## TS7005 — False implicit-any for block-scoped declarations (Fixed)

**Status**: Fixed. +8 tests passing (3874→3882 in first 6000 slice, 64.7%).
**Error code:** TS7005 ("Variable 'x' implicitly has an 'any' type.")
**Root cause**: `state_variable_checking.rs` added ALL non-ambient declarations without
initializers to `pending_implicit_any_vars`, including `let`/`const`/`using`. tsc only emits
TS7005/TS7034 for function-scoped (`var`) declarations — block-scoped declarations get their
implicit `any` silently without a diagnostic.
**Fix**: Check parent `VariableDeclarationList` flags for `LET`/`CONST`/`USING` before
inserting into `pending_implicit_any_vars`. Only `var` declarations are now tracked.
**Tests affected**: 9 `nestedBlockScopedBindings*` tests that all had `let x;` patterns.

### Config.rs strict defaults (also fixed, same commit)
When `strict` is not explicitly set in tsconfig, TypeScript defaults all strict-family options
to `false`. Our `resolve_checker_options` was only defaulting `noImplicitAny = true` (inheriting
from `CheckerOptions::default()`). Fixed to explicitly set all strict-family options to `false`
when `strict` is not specified. Harmless for conformance (runner injects `strict: true`).

## Deferred issues from TS7005 investigation session

- **TS18003 (runner-level)**: Conformance runner fingerprint mismatch for `include` patterns
  and config file path. Not a compiler bug — runner writes tsconfig with different include
  patterns than what the cache expects. Deferred.
- **TS2300 (fingerprint only)**: 13 failing tests already emit at least one TS2300 — fixes
  would only improve fingerprint accuracy (diagnostic count/position), not flip pass/fail at
  error-code level. 6 categories: accessor duplicates, interface string-literal duplicates,
  class+namespace merge, cross-file declare global, numeric literal names. Deferred.
- **TS1038 (diminishing returns)**: 5/6 pure tests already pass. Only 1 new flip possible
  (`importDeclWithDeclareModifierInAmbientContext.ts`). Deferred.
- **TS1206 (complex parser fix)**: Only 7 actual false-positive tests (not 38 as analysis
  suggested). 5 different parser root causes. Deferred.
- **TS5102 (already implemented)**: All remaining failures are due to OTHER unimplemented
  error codes in the same tests, not TS5102 itself. Deferred.
- **TS2882 (FIXED)**: See "TS2882 — noUncheckedSideEffectImports default" section below.

## TS6133 — Write-only parameters incorrectly suppressed (Fixed)

**Status**: Fixed. +4 tests passing (3896→3900 in first 6000 slice, 65.0%).
**Error code:** TS6133 ("'X' is declared but its value is never read.")
**Root cause**: `get_const_variable_name()` in `assignment_checker.rs` used the tracking
`resolve_identifier_symbol()` to check if an assignment target was const. This added
the target to `referenced_symbols`, which suppressed TS6133 for write-only parameters
(e.g., `person2 = "dummy value"` — `person2` was marked as "read" when it was only written).
**Fix**: Changed to use `self.ctx.binder.resolve_identifier()` (no tracking side-effect),
matching the pattern used by `check_function_assignment`.

### Remaining TS6133 fingerprint-level failures (29 tests)
These tests match at error-code level but fail fingerprint comparison:
- **15 over-reporting**: underscore-prefixed variables (`_`, `_a`) falsely flagged,
  object spread/rest destructuring, private class members, type guard variables.
- **13 under-reporting**: 12 tests have a last unused parameter not flagged (separate
  issue from the write-only fix — may be about destructuring or method-specific contexts),
  1 test has unflagged type parameter.
- **1 mixed**: write-only variable detection for locals (TS6198 vs TS6133 boundary).

### Missing TS6133 entirely (9 tests, deferred)
Tests where tsz produces `[]` but tsc expects TS6133:
- CommonJS `.js` files, ES private fields (`#unused`), destructured parameters,
  `infer` positions, JSDoc `@template` tags, self-references, dynamic property names,
  type parameter merging. Each has a distinct root cause.

## TS2305/TS2459/TS2460/TS2614 — Module name quoting in diagnostics (Fixed)

**Status**: Fixed. +11 tests passing in first 6000 slice (3900→3911, 65.2%).
**Error codes:** TS2305 ("Module '...' has no exported member '...'"), TS2459, TS2460, TS2614.
**Root cause**: TSC includes source-level double quotes in the module specifier parameter:
`Module '"./foo"' has no exported member 'X'`. Our diagnostics omitted the inner quotes,
producing `Module './foo' has no exported member 'X'`.
**Fix**: Added `format!("\"{module_name}\"")` wrapping in all `format_message` calls for
MODULE_HAS_NO_EXPORTED_MEMBER and related diagnostics across:
- `import_checker.rs` (8 call sites: TS2305, TS2459, TS2460, TS2614)
- `module_checker.rs` (2 call sites: TS2305, TS2614)
- `state_type_resolution_module.rs` (2 call sites: TS2305, TS2614)
Note: TS2307 ("Cannot find module") does NOT use double quotes — only single quotes
from the message template. No change needed there.

## TS6133 — Underscore suppression for destructuring binding elements (Fixed)

**Status**: Fixed. +1 test passing in full suite (7710→7711).
**Error code:** TS6133 ("'X' is declared but its value is never read.")
**Root cause**: TSC suppresses TS6133 for underscore-prefixed names (`_a`, `_b`) when
they appear in destructuring patterns (`const [_a, b] = arr` or `const { x: _x } = obj`).
Regular declarations like `let _a = 1` are NOT suppressed. Our checker lacked this check
in the local variables section of `type_checking_unused.rs`.
**Fix**: Added condition `is_variable && name.starts_with('_') && find_parent_binding_pattern(decl_idx).is_some()`
to skip TS6133 emission for underscore-prefixed destructuring binding elements.
**Key distinction**: Parameters already had underscore suppression (line ~445). This fix
covers local variables in destructuring patterns only, matching TSC's nuanced behavior.

### Remaining TS6133 underscore issues (not fixed)
- TSC also suppresses `import * as _` and for-of/for-in loop `const _` (not destructuring).
  These require additional checks for import symbols and for-of/for-in variable contexts.
- `unusedLocalsStartingWithUnderscore.ts` still fails due to extra TS2307 and missing
  import/for-of/for-in underscore suppression.

## Deferred from this session (not fixed)

- **TS2440 (19 tests)**: Import conflicts with local declaration. Code exists in
  `import_declaration_checker.rs` but never reached. Root cause is likely symbol merging
  in the binder — when import + local declaration create a single merged symbol, the
  conflict detection logic's filtering skips the relevant declarations. MEDIUM difficulty.
- **TS2875 (14 tests)**: JSX runtime module not found. Requires JSX pragma parsing
  (`@jsxImportSource`), module resolution validation, and error emission in JSX checking
  paths. MEDIUM difficulty.
- **TS2497 (13 tests)**: Module can only be referenced with ECMAScript imports. Requires
  detecting `export =` modules imported via ESM syntax and checking `esModuleInterop`/
  `allowSyntheticDefaultImports` flags. MEDIUM difficulty.
- **TS2589 (9 tests)**: Excessive instantiation depth. Infrastructure 80% complete (solver
  has depth tracking + guards). Missing: wiring `depth_exceeded` flag from evaluator/
  instantiator to checker diagnostic emission for type nodes/aliases. MEDIUM difficulty.
- **TS2580 (9 tests)**: Cannot find name (Node.js types). Code emits TS2591 instead of
  TS2580 because tsz always runs with tsconfig. Cache may expect TS2580 for non-tsconfig
  contexts. MEDIUM difficulty.
- **TS2454 (16 quick-win tests)**: 9 "pure" tests (tsz emits zero errors) and 7 multi-file
  tests. Root causes: try/catch destructuring, ES5 Symbol var, for-of pre-loop usage,
  computed property names, JSDoc type annotations. Each requires targeted flow analysis work.
- **TS18046 (10 tests, not implemented)**: "'x' is of type 'unknown'". Needs checks at
  property access, function calls, and binary operations on `unknown` type. Medium complexity.

## TS2882 — noUncheckedSideEffectImports default (Fixed)

**Status**: Fixed. +10 tests passing (part of 3915→3933 batch).
**Error code:** TS2882 ("Cannot find module or type declarations for side-effect import of '...'.")
**Root cause**: `CheckerOptions::default()` had `no_unchecked_side_effect_imports: true`, but
tsc 6.0 defaults to `false`. This caused all tests with side-effect imports (`import "module"`)
to be checked for module resolution even when the option wasn't explicitly set.
**Fix**: Changed default in `crates/tsz-common/src/checker_options.rs` from `true` to `false`.
Updated 3 test files that relied on the old default to explicitly set the option when needed.
**Previous diagnosis was wrong**: Earlier session noted this as "stale cache" issue — it was
actually a wrong default in `CheckerOptions`.

## TS2506 — False circular reference in heritage checking (Fixed)

**Status**: Fixed. +8 tests passing (part of 3915→3933 batch).
**Error code:** TS2506 ("'X' is referenced directly or indirectly in its own base expression.")
**Root cause**: `state_heritage_checking.rs` emitted TS2506 whenever a cross-file symbol was
found in `class_instance_resolution_set` during heritage clause checking. But this set is a
recursion guard (tracks symbols currently being type-resolved), NOT a cycle detector. A symbol
being in this set just means its type is being computed up the call stack — it does not prove
a circular base expression. This caused false positives for legitimate forward-reference class
relationships like `class Derived extends Base` where `Base` is declared later.
**Fix**: Removed the diagnostic emission block at lines 227-243 in `state_heritage_checking.rs`.
The recursion guard (`TypeId::ERROR` fallback) is preserved to prevent stack overflow. True
TS2506 cycle detection is handled by dedicated inheritance checks elsewhere.

## Deferred from this session (not fixed)

- **TS2693 (9 tests, false positive)**: "X only refers to a type, but is being used as a value
  here." False TS2693 emitted for expressions like `number[]`, `string[]`, `boolean[]` in value
  positions (e.g., `var na = new number[]`). tsc emits only TS1011 for the missing bracket
  argument. Root cause: `type_computation_access.rs` (lines 27-73) emits TS2693 for primitive
  keywords in element access parse-recovery, and `type_computation_identifier.rs` (lines 867-883)
  also emits TS2693 for unresolved primitive keywords. Fix: suppress TS2693 when parent is
  element access with missing argument (TS1011 already covers it). EASY difficulty.
- **TS18004 (5 tests, false positive)**: "No value exists in scope for the shorthand property."
  Emitted for parser error-recovery shorthand properties in `{ a; b; c }` (semicolons instead
  of commas). tsc suppresses this near parse errors. Attempted fix with `node_has_nearby_parse_error`
  didn't work — the check returned false despite nearby TS1005 errors. Needs debugging of why
  parse error positions don't align with shorthand property node spans. MEDIUM difficulty.
- **TS2322 (63 extra)**: Largest single-code false positive source. Complex type mismatch false
  positives across many test patterns. Requires ongoing solver/checker assignability work.
- **TS2339 (54 extra)**: Property access false positives. Ongoing.
- **TS2345 (52 extra)**: Argument type mismatch false positives. Ongoing.

## Current score: 7836/12574 (62.3%) — full suite

### Session progress (7687 → 7836, +149 tests):
- **TS5069/TS5053**: Config checks for emitDeclarationOnly/declarationMap/isolatedDeclarations without declaration, conflicting option pairs (+7)
- **TS5070/TS5071/TS5098**: resolveJsonModule with classic/none/system/umd, resolvePackageJson* without modern moduleResolution (+9)
- **TS5102 suppression**: Suppress TS5102 when ignoreDeprecations: "5.0" is valid (+2)
- **skipLibCheck**: Skip .d.ts type checking when enabled (+6)
- **TS2713**: Skip false positives for ALIAS symbols and parse error contexts (+32)
- **TS2580 vs TS2591**: Use TS2580 (no tsconfig suggestion) when no types field (+varies)
- **checkJs**: Removed redundant checker.check_js propagation that broke JSDoc (+11)
- **TS2524→TS1109**: Emit TS1109 instead of TS2524 for bare await in parameter defaults (+38)
- **TS2304 suppression**: File-level real syntax error detection replaces dead node flags (+66)
