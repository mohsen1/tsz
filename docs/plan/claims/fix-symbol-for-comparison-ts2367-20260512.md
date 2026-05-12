# fix-symbol-for-comparison-ts2367-20260512

Status: claim
Owner: Codex
Branch: fix-symbol-for-comparison-ts2367-20260512
Issue: #5834

## Scope

Match tsc by emitting TS2367 when comparing distinct `Symbol.for(...)` results that tsz currently treats as overlapping.

## Plan

- Add a focused TS2367 regression for the issue repro.
- Inspect unique-symbol inference/comparison overlap for `Symbol.for(...)` calls.
- Prefer fixing unique-symbol identity/comparability at the type level over a call-site special case.
