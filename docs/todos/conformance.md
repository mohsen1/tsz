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
