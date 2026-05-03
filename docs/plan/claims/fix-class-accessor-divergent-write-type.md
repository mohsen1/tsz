# fix(checker): narrow Application<unknown,..> -> Application<X,..> assignability shortcut to require `never` in target

- **Date**: 2026-05-03
- **Branch**: `fix/class-accessor-divergent-write-type`
- **PR**: TBD
- **Status**: ready
- **Workstream**: conformance — generic Application assignability + variance

## Intent

`is_assignable_to` had a fast path that returned `true` whenever the source
was `Foo<unknown, ...>` (or aliased to one) and the target was `Foo<X, ...>`
with at least one non-unknown, non-error arg. That bypassed variance entirely
and incorrectly accepted `A<unknown>` -> `A<string>` even when `A<T>` is
covariant or invariant in T (where `unknown` is NOT a subtype of `string`).
The conformance test `getAndSetNotIdenticalType2.ts` expected a TS2322 on
`x.x = r` where the setter takes `A<string>` and the getter returned
`A<unknown>`; we silently let it through.

The fix narrows the shortcut to require at least one `never` in the target's
args -- the typical signature of inference fallback for Thenable / Promise
constructors (`EPromise<never, A>`). User-written `A<unknown>` -> `A<string>`
no longer slips through, but the `await_incorrectThisType.ts` flow that
the original shortcut was added for keeps working.

## Files Touched

- `crates/tsz-checker/src/assignability/assignability_checker.rs` (~25 LOC)
- `crates/tsz-checker/src/tests/application_unknown_args_assignability_tests.rs` (new, ~110 LOC)
- `crates/tsz-checker/src/lib.rs` (+3 LOC: register the new test module)

## Verification

- `cargo nextest run -p tsz-checker --lib` -- 3209 / 3209 passing
- `./scripts/conformance/conformance.sh run --filter "getAndSetNotIdenticalType2"` -- 1 / 1 passing
- `./scripts/conformance/conformance.sh run --filter "await_incorrectThisType"` -- 1 / 1 passing
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run` -- net +4 (`getAndSetNotIdenticalType2`, `strictOptionalProperties3`, `tsxAttributeResolution6`, `typeFromParamTagForFunction` flip FAIL -> PASS), no regressions
