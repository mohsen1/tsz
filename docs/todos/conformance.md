# Conformance Issues — Investigated but Deferred

## TS5025: Remaining tsconfig property ordering mismatch (32 tests)

**Error code:** TS5025 ("Unknown compiler option … Did you mean …?")
**Test files:** 32 remaining tests with TS5025-only fingerprint mismatches
**Reason:** The tsc cache was generated with non-deterministic `HashMap` iteration
order for tsconfig properties. The conformance runner now sorts properties
alphabetically, but the existing cache has different orderings for these 32 tests.
**Fix:** Regenerate the tsc cache (`./scripts/conformance.sh generate`) after the
cache generator's `sort_keys()` change lands. This will align line numbers for all
TS5025 diagnostics. Expected to fix ~32 additional tests.

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
