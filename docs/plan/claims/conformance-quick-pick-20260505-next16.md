# fix(checker): align JSX children diagnostic surfaces

- **Date**: 2026-05-05
- **Branch**: `conformance/quick-pick-20260505-next16`
- **PR**: #3147
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the quick-picked fingerprint-only mismatch in
`jsxChildrenIndividualErrorElaborations.tsx`. The diagnostic code set already
matches tsc, but several JSX children diagnostics preserve `Cb` alias displays
where tsc reports the expanded function type or the declared union surface.

## Files Touched

- `crates/tsz-checker/src/checkers/jsx/children.rs`
- `crates/tsz-checker/src/checkers/jsx/diagnostics.rs`
- `crates/tsz-checker/tests/jsx_component_attribute_tests.rs`

## Verification

- `scripts/session/quick-pick.sh --run` selected and reproduced
  `TypeScript/tests/cases/compiler/jsxChildrenIndividualErrorElaborations.tsx`.
- `cargo fmt --check`
- `CARGO_TARGET_DIR=/tmp/tsz-next16-target cargo nextest run -p tsz-checker --test jsx_component_attribute_tests`
- `./scripts/conformance/conformance.sh run --filter "jsxChildrenIndividualErrorElaborations" --verbose --profile dev`
  (with local `.target/dev -> debug` symlink because Cargo's dev profile outputs
  under `.target/debug`)
