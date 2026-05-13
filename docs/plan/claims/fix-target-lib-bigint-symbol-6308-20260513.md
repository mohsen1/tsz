# Claim: target/lib diagnostics for BigInt and Symbol (#6308)

Status: ready
Branch: fix-target-lib-bigint-symbol-6308-20260513
PR: #6313
Owner: Codex
Created: 2026-05-13

## Scope

Add regression coverage for TypeScript-compatible feature availability diagnostics:

- TS2737 for BigInt literals when target is lower than ES2020.
- TS2585 for `Symbol` value usage when the active lib set does not provide a value-side `Symbol`.

## Assumption

The issue's bare `tsz test.ts` reproduction assumes the old tsc default target. This repository currently uses the TS6-compatible default target `es2025`; under that default, `123n` and `Symbol()` are valid. The diagnostics are still required when users select the lower target/lib combination, and that path already works.

## Verification

- `cargo run -p tsz-cli --bin tsz -- --noEmit --strict --pretty false --target ES5 --ignoreDeprecations 6.0 --lib es5 /tmp/issue6308.ts` emitted TS2737 and TS2585.
- `cargo test -p tsz-cli --test tsc_compat_tests bigint_and_symbol_availability_follow_target_and_lib -- --nocapture` passed.
- `cargo fmt --all -- --check` passed.
