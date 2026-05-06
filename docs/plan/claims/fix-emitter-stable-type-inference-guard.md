# [WIP] fix(emitter): avoid unstable type inference match guard

- **Date**: 2026-05-06
- **Branch**: `fix/emitter-stable-type-inference-guard`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 8.4 (DRY emitter helpers)

## Intent

Current `origin/main` does not build on stable Rust because `type_inference.rs` uses an `if let` match guard, which is still unstable. This slice rewrites that guard to equivalent stable control flow without changing declaration emit behavior.

## Files Touched

- TBD after implementation.

## Verification

- Baseline: `./scripts/conformance/conformance.sh run --filter "propTypeValidatorInference" --verbose` fails to build with `E0658` in `crates/tsz-emitter/src/declaration_emitter/helpers/type_inference.rs`.
