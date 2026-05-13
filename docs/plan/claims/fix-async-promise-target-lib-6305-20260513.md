# Claim: async Promise target/lib diagnostic (#6305)

Status: ready
Branch: fix-async-promise-target-lib-6305-20260513
PR: #6317
Owner: Codex
Created: 2026-05-13

## Scope

Add regression coverage for TS2705 on async functions when target/lib settings do not provide the Promise constructor.

## Assumption

The issue's bare `tsz test.ts` reproduction assumes the old tsc default target. This repository currently uses the TS6-compatible default target `es2025`; under that default, async functions are valid. TS2705 is still required when users select a lower target/lib combination without Promise, and that path already works.

## Verification

- `cargo run -p tsz-cli --bin tsz -- --noEmit --strict --pretty false --target ES5 --ignoreDeprecations 6.0 --lib es5 /tmp/issue6305.ts` emitted TS2705.
- `cargo test -p tsz-cli --test tsc_compat_tests async_function_without_promise_constructor_reports_ts2705 -- --nocapture` passed.
- `cargo fmt --all -- --check` passed.
