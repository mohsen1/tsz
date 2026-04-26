# fix(checker): only strip optional-property undefined when stripped type is callable

- **Date**: 2026-04-26
- **Branch**: `fix/contextual-strip-undefined-only-for-callable`
- **PR**: TBD
- **Status**: ready
- **Workstream**: Workstream 1 (conformance fixes — fingerprint parity)

## Intent

Fix `contextuallyTypedOptionalProperty.ts` (TS18048 missing on `foo({ y: match(y => y > 0) })`).

The previous strip in `object_literal/computation.rs` removed `undefined`
from the contextual type of every optional property assignment so that
`set: deprecate(arrow, ...)` would infer `T = handlerFn` instead of
`handlerFn | undefined`. That over-eager strip also dropped `undefined`
from non-callable optional properties (e.g. `y?: number`), suppressing
TS18048 inside generic callbacks like `match<T>(cb: (value: T) => boolean): T`.

The fix gates the strip on whether the stripped (post-`remove_undefined`)
type is callable (Function, Callable with call signatures, or a union /
intersection thereof). Non-callable types like `number | undefined` keep
`undefined` so `match<T>` infers `T = number | undefined` and TS18048
fires inside the callback as tsc expects.

## Files Touched

- `crates/tsz-checker/src/types/computation/object_literal/computation.rs`
  — gate the `remove_undefined` call on `stripped_property_context_is_callable(stripped)`.
- `crates/tsz-checker/src/types/computation/object_literal/mod.rs`
  — add `stripped_property_context_is_callable` helper that recurses
  through unions/intersections.
- `crates/tsz-checker/tests/generic_call_inference_tests.rs`
  — flip `generic_return_context_preserves_undefined_through_optional_wrappers`
  back to expecting both TS18048 errors (original tsc baseline behavior).

## Verification

- Targeted conformance: `contextuallyTypedOptionalProperty.ts` flips
  FAIL → PASS.
- Full conformance: 12183 → 12189 (+6, no regressions). Other improvements:
  `jsExportMemberMergedWithModuleAugmentation2.ts`,
  `literalFreshnessPropagationOnNarrowing.ts`,
  `iteratorSpreadInArray5.ts`,
  `catchClauseWithTypeAnnotation.ts`,
  `templateLiteralTypes5.ts`.
- `cargo nextest run -p tsz-checker` (5397 pass).
- `cargo nextest run -p tsz-cli` (1057 pass).
- LOC contract: helper relocated to `mod.rs` to keep `computation.rs`
  under the 2000 LOC ceiling.
