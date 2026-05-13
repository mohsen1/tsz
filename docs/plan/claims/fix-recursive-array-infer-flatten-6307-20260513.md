# Claim: recursive array infer conditional flatten (#6307)

Status: ready
Branch: fix-recursive-array-infer-flatten-6307-20260513
PR: #6320
Owner: Codex
Created: 2026-05-13

## Scope

Fix recursive conditional evaluation for `T extends Array<infer U> ? Flatten<U> : T`, where `Flatten<number[]>` currently remains `number[]` instead of evaluating to `number`.

## Implementation

- Match `Array<infer U>` / `ReadonlyArray<infer U>` application patterns against native array-like check types in conditional evaluation.
- Defer unresolved application infer patterns instead of caching the false branch before lib-backed bases are available.
- Add a focused checker regression for recursive `Flatten` over shorthand and generic array forms.

## Verification

- Passed: direct CLI repro from #6307 with `CARGO_TARGET_DIR=target-pr6320 cargo run -q -p tsz-cli --bin tsz -- --noEmit --strict <tmp repro>`.
- Passed: `CARGO_TARGET_DIR=target-pr6320 cargo test -p tsz-checker --test conditional_infer_tests -- --nocapture`.
- Passed: `CARGO_TARGET_DIR=target-pr6320 cargo check -p tsz-checker`.
- Passed: `cargo fmt --all -- --check`.
