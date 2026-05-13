# Claim: fix this-predicate property narrowing (#6299)

Status: ready
Branch: fix-this-predicate-property-narrowing-6299-20260513
PR: #6303
Owner: Codex
Created: 2026-05-13

## Scope

Fix the false positive where a method returning `this is Container<T> & { value: T }` narrows the receiver but a subsequent `container.value` read still reports `T | null`.

## Files touched

- `crates/tsz-checker/src/flow/control_flow/call_condition_narrowing.rs`
- `crates/tsz-cli/tests/tsc_compat_tests.rs`
- `docs/plan/claims/fix-this-predicate-property-narrowing-6299-20260513.md`

## Verification

- `cargo run -p tsz-cli --bin tsz -- --noEmit --strict --pretty false /tmp/issue6299.ts` passed
- `cargo test -p tsz-cli --test tsc_compat_tests this_type_predicate_narrows_receiver_property -- --nocapture` passed
- `cargo fmt --all -- --check` passed
