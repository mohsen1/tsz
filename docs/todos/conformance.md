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

## Deferred issues from this run (not fixed)

- **TS5057**: `TypeScript/tests/cases/compiler/commonSourceDir1.ts` — requires project/tsconfig discovery and compiler option plumbing that is not yet wired into the current checker flow.
- **TS5095**: `TypeScript/tests/cases/compiler/declarationEmitBundleWithAmbientReferences.ts` — requires moduleResolution validation against module-kind constraints, which is still outside current scope.
