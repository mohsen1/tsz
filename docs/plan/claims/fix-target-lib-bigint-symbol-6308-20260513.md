# Claim: target/lib diagnostics for BigInt and Symbol (#6308)

Status: claim
Branch: fix-target-lib-bigint-symbol-6308-20260513
PR: TBD
Owner: Codex
Created: 2026-05-13

## Scope

Add missing TypeScript-compatible feature availability diagnostics:

- TS2737 for BigInt literals when target is lower than ES2020.
- TS2585 for `Symbol` value usage when the active lib set does not provide a value-side `Symbol`.

## Verification plan

- Direct CLI repro from issue #6308.
- Focused CLI compatibility regression.
- `cargo fmt --all -- --check`.
