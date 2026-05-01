---
name: Fix checker TS2740 unfold generic alias wrapper
description: Extend `ts2739_alias_of_application_source_display` to also unfold generic wrapper aliases (`type Wrapper<U> = Inner<U>`) — when source is `Wrapper<X>` and the body is itself an `Application` of a different alias, display `Inner<X>` instead.
type: project
branch: fix-checker-ts2740-unfold-generic-alias-wrapper
status: ready
scope: checker (TS2739 / TS2740 / alias display)

## Summary

When a generic alias wraps another alias's application
(`type IndirectArrayish<U extends unknown[]> = Objectish<U>;`), tsc
unfolds the wrapper one level and displays the body alias's application
form (`Objectish<X>`) in TS2739/TS2740 source positions. tsz kept the
wrapper name (`IndirectArrayish<X>`).

This was the same flavor of unfold that PR #1963 introduced for
non-generic wrapper aliases (`type B = A<X>`). The extension here
covers the generic case.

## Root Cause

`ts2739_alias_of_application_source_display` returned `None` when the
alias had non-empty `type_params`, falling back to the standard
formatter that shows the wrapper name. Two paths went unhandled:

1. Source as an `Application(Lazy(wrapper), [args])` where the def
   lookup via `find_def_for_type` returns `None` — needed to peek at
   the application's base.
2. Once the wrapper's def is found, the body needs to be detected as an
   `Application` of a *different* alias and the wrapper's type-params
   substituted into the body's args.

## Fix

Both unhandled paths added:

- def-id lookup falls through to `lazy_def_id(application.base)` when
  `lazy_def_id(source)` and `find_def_for_type(source)` both miss.
- For aliases with non-empty `type_params`, check if `def.body` is an
  `Application` of a different alias, and if so substitute the
  wrapper's `type_params` with the source application's `args`,
  returning a fresh `Application(body_base, body_args_substituted)`
  for the formatter.

## Files Changed

- `crates/tsz-checker/src/error_reporter/render_failure.rs`

## Verification

- Conformance: net **+13** (12304 → 12317). 13 improvements, **0 regressions**.
  - `mappedTypeWithAny.ts` (target).
  - `reactTransitiveImportHasValidDeclaration.ts`,
    `pushTypeGetTypeOfAlias.ts`, `optionalParameterInDestructuringWithInitializer.ts`,
    `varianceReferences.ts`, `intersectionThisTypes.ts`,
    `tsxAttributeResolution5.tsx`,
    `checkJsxGenericTagHasCorrectInferences.tsx`,
    + 4 declarationEmit tests, + 1 transpile test.
- Unit tests: tsz-checker (3104) + tsz-solver (5576) all green.
