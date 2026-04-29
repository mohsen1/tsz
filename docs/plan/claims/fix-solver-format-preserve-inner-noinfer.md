# fix(solver): preserve `NoInfer<...>` in display when nested inside a union/function (only strip at outermost)

- **Date**: 2026-04-29
- **Branch**: `fix/solver-format-preserve-inner-noinfer`
- **PR**: #1733
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance — fingerprint parity)

## Intent

Fix fingerprint mismatch in `compiler/noInferUnionExcessPropertyCheck1.ts`
(fingerprint-only, codes match). tsc preserves `NoInfer<...>` in error
messages when it's nested inside a union or function, but strips it only at
the outermost layer of the displayed type. tsz currently strips at every
level, producing `'{ x: string; } | (() => { x: string; })'` where tsc
shows `'NoInfer<{ x: string; }> | (() => NoInfer<{ x: string; }>)'`.

## Root Cause

`crates/tsz-solver/src/diagnostics/format/mod.rs:1453-1454`:

```rust
// NoInfer<T> is transparent in error messages - tsc displays just T
TypeData::NoInfer(inner) => self.format(*inner),
```

This is unconditionally transparent. tsc actually keeps `NoInfer<>` for
inner positions (union members, function returns, etc.) and strips it only
when the displayed type's *outermost* form is a `NoInfer<>` wrapper.

## Fix

Detect "outermost" via `current_depth == 1` (the formatter increments
`current_depth` at the entry of `format()` from 0 → 1 before delegating to
`format_key`, so `format_key` runs at depth 1 for the top-level call and
≥ 2 for inner recursions). When at depth 1, strip; otherwise, render
`NoInfer<...>`.

## Files Touched

- `crates/tsz-solver/src/diagnostics/format/mod.rs` (~5 LOC)
- `crates/tsz-solver/src/diagnostics/format/tests.rs` (or a new colocated
  test file) — lock the inner-vs-outer behavior with three matrix cases
  matching the conformance test.

## Verification

- `cargo nextest run --package tsz-solver --test format_tests` (or full
  solver suite)
- `./scripts/conformance/conformance.sh run --filter "noInferUnionExcessPropertyCheck1" --verbose`
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run` (full
  conformance, expect +1 net delta)
