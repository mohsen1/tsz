# Claim: recursive array infer conditional flatten (#6307)

Status: claim
Branch: fix-recursive-array-infer-flatten-6307-20260513
PR: TBD
Owner: Codex
Created: 2026-05-13

## Scope

Fix recursive conditional evaluation for `T extends Array<infer U> ? Flatten<U> : T`, where `Flatten<number[]>` currently remains `number[]` instead of evaluating to `number`.

## Verification plan

- Direct CLI repro from #6307.
- Focused checker/solver regression for recursive array infer conditional evaluation.
- `cargo fmt --all -- --check`.
