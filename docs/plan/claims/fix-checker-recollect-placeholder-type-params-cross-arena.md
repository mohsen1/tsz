---
branch: fix/checker-recollect-placeholder-type-params-cross-arena
status: ready
created: 2026-05-03 03:55:00
---

**2026-05-03 03:55:00** · branch
`fix/checker-recollect-placeholder-type-params-cross-arena` ·
**Conformance fix: recollect placeholder type-param caches when the
symbol originates outside the current arena** ·
`get_type_params_for_symbol`'s placeholder-cache detection gated
re-collection on `symbol_is_from_lib(sym_id)`. For lib types whose
declarations were merged into the user binder (e.g. `IteratorResult`,
`IteratorYieldResult`, `IteratorReturnResult`), `symbol_arenas` no longer
points at the lib arena, `symbol_is_from_lib` returns `false`, and the
cached all-`None` placeholder is returned untouched. Downstream
`fillMissingTypeArguments` (in
`crates/tsz-lowering/src/lower/advanced.rs::lower_type_reference`)
short-circuits because every remaining param's `default` is `None`, the
resulting `Application` has fewer args than the def's type-parameter
count, and the variance fast-path in
`crates/tsz-solver/src/relations/subtype/rules/generics.rs:438`
(`if variances.len() == s_app.args.len()`) is skipped — falling through
to a stricter structural check that produces false TS2416/TS2322. The
fix broadens the placeholder gate from `symbol_is_from_lib` to "from a
different arena" — `from_lib` OR `symbol_arenas[sym_id] != ctx.arena` OR
any of the symbol's declarations lives in a different arena. Pure
user-arena symbols keep the original single-shot collection semantics
(this preserves the JS imported-generic TS8026 path, which depends on
the cached params not being refreshed mid-traversal). Net conformance:
12346 → 12351 (+5): `genericIndexedAccessVarianceComparisonResultCorrect.ts`,
`indexAt.ts`, `jsxEmptyExpressionNotCountedAsChild2.tsx`,
`typeOnlyMerge3.ts`, `umd2.ts`,
`assertionsAndNonReturningFunctions.ts`, `typeFromJSInitializer.ts`,
`invalidUndefinedValues.ts`. Three fingerprint-only display regressions
remain (`tsxInvokeComponentType`, `destructuringParameterDeclaration1ES5`,
`generatorTypeCheck25`) — all error-code-correct, with cosmetic
alias-vs-expansion or default-vs-inferred argument display drift that
follow-up work can address. Unit test
`alias_with_default_type_arg_implements_check_does_not_emit_ts2416`
locks the fix using a self-contained user-arena reproduction of the
same shape (`type IteratorResult<T, TReturn = any> = …`) that the lib
case exercises.
