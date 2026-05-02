---
branch: fix/solver-bootstrap-fallback-only-no-lib
status: ready
created: 2026-05-02 21:08:46
---

**2026-05-02 21:08:46** · branch `fix/solver-bootstrap-fallback-only-no-lib` ·
**Conformance fix: gate `Function.name` bootstrap fallback on no-lib** ·
`resolve_function_property` consulted the boxed `Function` interface
first and then *unconditionally* fell back to a hardcoded list when the
boxed lookup returned NotFound. The hardcoded list included
`name => string`, but `Function.name` was only added to the lib
`Function` interface in `lib.es2015.core.d.ts`. With a real lib loaded
that predates es2015 (or the boxed interface otherwise lacking the
property), the bootstrap fallback papered over the not-found result and
silently resolved future-version members. The fix gates only the
version-specific `name` entry behind `!boxed_function_loaded`; all
other es5-baseline entries (`apply`/`call`/`bind`/`prototype`/etc.) keep
their fallback behavior so synthesized callable shapes that don't
navigate to the boxed `Function` interface still resolve them. Net
conformance: 12346 → 12348 (+2: `genericIndexedAccessVarianceComparisonResultCorrect.ts`,
`externalModules/umd2.ts`); no regressions. Unit test
`function_name_resolves_via_bootstrap_when_no_function_interface_registered`
locks the negative case (the no-lib path that internal Solver tests
depend on).
