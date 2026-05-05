# [WIP] fix(checker): align instantiation expression no-crash fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/checker-instantiation-expression-nocrash-fingerprint`
- **PR**: #3245
- **Status**: claim
- **Workstream**: 1 (Conformance / diagnostic fingerprints)

## Intent

The canonical conformance picker selected
`TypeScript/tests/cases/compiler/instantiationExpressionErrorNoCrash.ts`,
a fingerprint-only failure where tsz and tsc agree on diagnostic codes
`TS2344` and `TS2635` but differ in diagnostic fingerprint details.

This PR will root-cause the remaining instantiation-expression no-crash
fingerprint mismatch, add owning Rust regression coverage, and rerun the
targeted conformance test.

## Files Touched

- `crates/tsz-checker/src/state/type_environment/formatting.rs`
- `crates/tsz-checker/src/tests/dispatch_tests.rs`
- `docs/plan/claims/fix-checker-instantiation-expression-nocrash-fingerprint.md`

## Verification

- Baseline target command:
  `./scripts/conformance/conformance.sh run --filter "instantiationExpressionErrorNoCrash" --verbose`
- `CARGO_BUILD_JOBS=1 cargo nextest run -p tsz-checker --lib ts2635`
  - 2 tests passed.
- Planned: targeted conformance rerun for `instantiationExpressionErrorNoCrash`.
  - Still blocked locally before the filtered case could run.
  - Earlier attempts were interrupted by repeated workspace disk exhaustion and
    one external SIGTERM.
  - After freeing inactive build artifacts, a clean retry failed while writing
    `.target/dist-fast/.../tsz_core...rcgu.o` because the target directory
    disappeared mid-build.
  - A standalone `CARGO_BUILD_JOBS=1 cargo build --target-dir .target --profile
    dist-fast -p tsz-cli -p tsz-conformance` reached `tsz-lsp`, then failed in
    Cargo fingerprint/output handling; `.target` was again gone immediately
    after the failure.
  - Retried with an external target dir, building only required binaries:
    `CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 cargo build --profile dist-fast
    --target-dir /tmp/tsz-3245-target -p tsz-cli --bin tsz --bin tsz-server
    -p tsz-conformance --bin tsz-conformance`; the build was first terminated
    without diagnostics, then failed late in `tsz-cli` after its object output
    disappeared.
  - Retried a lower-footprint dev build to `/tmp/tsz-3245-dev-target`; it was
    also externally terminated before reaching the project crates.
