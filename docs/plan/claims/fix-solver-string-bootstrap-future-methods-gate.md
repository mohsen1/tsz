---
branch: fix/solver-string-bootstrap-future-methods-gate
status: ready
created: 2026-05-02 21:28:57
---

**2026-05-02 21:28:57** · branch `fix/solver-string-bootstrap-future-methods-gate` ·
**Conformance fix: gate post-es5 primitive members in `resolve_primitive_property` bootstrap fallback** ·
Generalises the per-method gate shipped in
`fix(solver): gate Function.name bootstrap fallback on no-lib` (PR
#2398) to all primitive types. `resolve_primitive_property` consulted
the boxed primitive interface (`String`/`Number`/`Symbol`/`Boolean`/
`Bigint`) first and then unconditionally fell back to a hardcoded
apparent-member list when the boxed lookup returned NotFound. That
list claimed methods like `String.includes`, `String.padStart`,
`String.at`, `Symbol.description`, etc. that were only added to the
boxed interface in lib.es2015.* or later. With a real lib loaded that
predates the property's introduction, the fallback papered over the
not-found result so the checker never emitted TS2550 / TS2339. The
fix introduces `is_post_es5_primitive_member` as the structural
filter: when the boxed interface IS loaded but lacks a post-es5
member, the not-found result propagates; the no-lib bootstrap path is
unaffected. Net conformance: 12346 → 12350 (+4): `indexAt.ts`,
`genericIndexedAccessVarianceComparisonResultCorrect.ts`,
`externalModules/typeOnlyMerge3.ts`, `externalModules/umd2.ts`. No
regressions. Unit test
`string_post_es5_member_resolves_via_bootstrap_when_no_string_interface_registered`
locks the negative case.
