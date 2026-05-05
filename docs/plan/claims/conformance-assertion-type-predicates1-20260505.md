# [WIP] fix(conformance): align assertion type predicate diagnostics

- **Date**: 2026-05-05
- **Claimed**: 2026-05-05 17:00:18 UTC
- **Branch**: `conformance/assertion-type-predicates1-20260505`
- **PR**: #3127
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the picked `assertionTypePredicates1.ts` conformance mismatch. The current
fingerprint expects TS1228, TS2775, TS2776, and TS7027, but tsz only emits
TS1228, so the work will identify why assertion-call diagnostics and unreachable
code are missing.

## Pick

```text
path:     TypeScript/tests/cases/conformance/controlFlow/assertionTypePredicates1.ts
category: only-missing
expected: TS1228,TS2775,TS2776,TS7027
actual:   TS1228
missing:  TS2775,TS2776,TS7027
extra:    -
pool:     131
```

## Files Touched

- TBD after investigation.

## Verification

- `./scripts/conformance/conformance.sh run --filter "assertionTypePredicates1" --verbose`
- focused Rust unit tests in the owning crate
- `cargo check --package tsz-checker`
- `cargo build --profile dist-fast --bin tsz`
- `cargo nextest run --package tsz-checker --lib`
- `./scripts/conformance/conformance.sh run --max 200`
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run 2>&1 | grep FINAL`
