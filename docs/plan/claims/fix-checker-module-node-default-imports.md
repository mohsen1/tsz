Status: ready
Branch: fix/checker-module-node-default-imports
Created: 2026-05-06 00:45:32 UTC
Scope: Conformance workstream 1

## Intent

Fix `moduleNodeDefaultImports.ts`, where Node16/18/20/Next default imports
from a CommonJS `.cjs` module produce extra TS2339 and TS2367 diagnostics.

## Planned Scope

- Diagnose the default-import shape for `.cjs` under Node module modes.
- Fix the owning checker/solver/boundary behavior without local diagnostic
  suppression.
- Add focused Rust regression coverage for the invariant.

## Verification Plan

- `cargo nextest` for touched crates.
- `./scripts/conformance/conformance.sh run --filter "moduleNodeDefaultImports" --verbose`
- `./scripts/conformance/conformance.sh run --filter "moduleNodeDefaultImports" --exact`
- `./scripts/conformance/conformance.sh run --max 200`
