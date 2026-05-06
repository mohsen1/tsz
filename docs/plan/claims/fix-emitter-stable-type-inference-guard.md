# [WIP] fix(emitter): avoid unstable type inference match guard

- **Date**: 2026-05-06
- **Branch**: `fix/emitter-stable-type-inference-guard`
- **PR**: #3932
- **Status**: ready
- **Workstream**: 8.4 (DRY emitter helpers)

## Intent

Current `origin/main` does not build on stable Rust because `type_inference.rs` uses an `if let` match guard, which is still unstable. This slice rewrites that guard to equivalent stable control flow without changing declaration emit behavior.

## Files Touched

- `crates/tsz-emitter/src/declaration_emitter/helpers/type_inference.rs`

## Verification

- Baseline: `./scripts/conformance/conformance.sh run --filter "propTypeValidatorInference" --verbose` fails to build with `E0658` in `crates/tsz-emitter/src/declaration_emitter/helpers/type_inference.rs`.
- `cargo check -p tsz-emitter`
- `./scripts/conformance/conformance.sh run --filter "propTypeValidatorInference" --verbose` builds successfully and reaches the existing extra `TS2322` conformance failure.
- `git diff --check`

Note: `CARGO_TARGET_DIR=.target/nextest-local cargo clippy --no-deps -p tsz-emitter --lib -- -D warnings` is blocked by pre-existing `origin/main` clippy warnings in emitter files outside this change.
