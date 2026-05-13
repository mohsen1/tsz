# Claim: async Promise target/lib diagnostic (#6305)

Status: claim
Branch: fix-async-promise-target-lib-6305-20260513
PR: TBD
Owner: Codex
Created: 2026-05-13

## Scope

Handle or document TS2705 for async functions when target/lib settings do not provide the Promise constructor.

## Verification plan

- Direct CLI repro with `--target es5 --lib es5 --ignoreDeprecations 6.0`.
- Focused CLI compatibility regression if behavior already exists or after implementation.
- `cargo fmt --all -- --check`.
