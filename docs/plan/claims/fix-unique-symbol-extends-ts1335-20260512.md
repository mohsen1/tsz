# fix-unique-symbol-extends-ts1335-20260512

Status: claim
Owner: Codex
Branch: fix-unique-symbol-extends-ts1335-20260512
Issue: #5833

## Scope

Emit TS1335 for `unique symbol` used directly as the extends type in conditional types, matching tsc and removing the downstream false TS2322 described in #5833.

## Plan

- Add focused checker regression coverage for the issue repro.
- Detect the invalid `unique symbol` type node in conditional-type extends position.
- Verify the targeted test and the relevant conformance filter if available.
