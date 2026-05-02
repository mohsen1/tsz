---
branch: fix/checker-string-literal-lib-suggest-at
status: ready
created: 2026-05-02 20:14:10
---

**2026-05-02 20:14:10** · branch `fix/checker-string-literal-lib-suggest-at` ·
**Conformance fix: `indexAt.ts` (TS2550 for `"foo".at(0)` on pre-es2022 lib)** ·
The bootstrap apparent type for `string` (used as a fallback when boxed
String interface lookup returns NotFound) included `at` in
`STRING_METHODS_RETURN_STRING`. With a real lib loaded for an older
target (e.g. es2021), `String.at` is genuinely absent, but the fallback
papered over the not-found result, so tsz silently resolved
`"foo".at(0)` and never emitted TS2550. Removing `at` from the
fallback list lets the property-not-found path run, which then routes
through the existing `get_lib_for_type_property("String", "at") =>
"es2022"` lookup and emits the matching TS2550 suggestion. `at` was
also incorrect to keep in this list because it returns `string |
undefined`, not `string`. Net conformance: 12346 → 12347 (+1, only
`indexAt.ts`); no regressions.
