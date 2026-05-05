# fix(emitter): keep namespace export local type dependencies

Status: ready
Owner: Codex
Branch: `fix-dts-namespace-export-local-types`
Created: 2026-05-05 09:25:44 UTC

## Intent

Close the `exportNamespaceDeclarationRetainsVisibility` declaration emit gap by
retaining local namespace type declarations referenced by exported namespace
members under an `export =` surface.

## Planned Scope

- `crates/tsz-emitter/src/declaration_emitter/helpers/visibility.rs`
- `crates/tsz-emitter/src/declaration_emitter/tests/simple_declarations.rs`
- `docs/plan/claims/fix-dts-namespace-export-local-types.md`

## Verification Plan

- `cargo fmt --package tsz-emitter -- --check`
- `cargo check --package tsz-emitter`
- `cargo test --package tsz-emitter test_export_equals_namespace_keeps_local_type_dependencies --lib`
- `./scripts/emit/run.sh --dts-only --filter=exportNamespaceDeclarationRetainsVisibility --verbose --concurrency=1 --timeout=30000`
