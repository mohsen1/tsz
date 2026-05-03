# fix(emitter): avoid imports for referenced declaration type queries

- **Date**: 2026-05-03
- **Branch**: `fix/dts-reference-directive-typeof-05031312`
- **PR**: TBD
- **Status**: claim
- **Workstream**: §2 (Emit pass rate)

## Intent

Investigate and fix declaration emit cases where a triple-slash reference
directive already makes a declaration-file symbol visible, but tsz still adds a
synthetic import for a `typeof` query in the generated `.d.ts`. The target
cluster is the `typeReferenceDirectives` pair with a single extra import line.

## Files Touched

- TBD

## Verification

- `./scripts/safe-run.sh ./scripts/emit/run.sh --dts-only --filter=typeReferenceDirectives --verbose --timeout=20000 --json-out=/tmp/tsz-emit-typeReferenceDirectives-before.json`
  (reproduces 9/11 pass; failures are `typeReferenceDirectives5` and
  `typeReferenceDirectives13` with one extra import line).
