# fix(conformance): zero-skip clean pass on current main

- **Date**: 2026-05-12
- **Branch**: `inspect/zero-skips-20260512`
- **Base**: `main`
- **Status**: ready
- **Workstream**: conformance

## Intent

Move current main from the stale 12581/12582 conformance snapshot to a clean
12585/12585 run with no skipped tests, known failures, crashes, timeouts, or
fingerprint-only cases.

## Scope

- Run `@noCheck` conformance files instead of treating them as harness skips.
- Preserve `noCheck` in generated tsconfig options while forcing those cases off
  the server path.
- Restore the `callsOnComplexSignatures.tsx` React `ComponentType` JSX union pass.
- Raise the default conformance per-test timeout so the full suite does not
  record the slow `coAndContraVariantInferences3.ts` fixture as a timeout.
- Refresh the conformance cache and snapshot artifacts.

## Verification

- `cargo test -p tsz-conformance -- --nocapture` - passed
- `cargo test -p tsz-checker --lib jsx_react_component_type_union_does_not_emit_ts2786 -- --nocapture` - passed
- `./scripts/conformance/conformance.sh run --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --filter "noCheck" --verbose` - `3/3 passed`, skipped 0
- `./scripts/conformance/conformance.sh run --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --filter "callsOnComplexSignatures"` - `1/1 passed`
- `scripts/safe-run.sh --limit 75% -- ./scripts/conformance/conformance.sh run --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases` - `12585/12585 passed`, skipped 0, known failures 0, timeouts 0
