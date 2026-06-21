# Emitter

`crates/tsz-emitter` is the output layer. It consumes syntax plus checked facts
and produces JavaScript, declaration output, and source maps. It must not perform
semantic validation or patch already emitted output to encode type policy.

## Two-Phase JS Emit

The emitter architecture is described in `docs/architecture/EMIT_ARCHITECTURE.md`.
The core model is:

```text
AST + options + checked summaries
  -> lowering pass
  -> EmitPlan / transform directives / typed plan fields
  -> printer
  -> JavaScript + source map
```

Key paths:

- `crates/tsz-emitter/src/lowering/`
- `crates/tsz-emitter/src/passes/`
- `crates/tsz-emitter/src/emitter/`
- `crates/tsz-emitter/src/transforms/`
- `crates/tsz-emitter/src/output/`
- `crates/tsz-emitter/src/context/`

The lowering pass decides what must change for the configured target and module
mode. The printer writes output. Transform code may use IR where direct string
assembly would be fragile.

## Declaration Emit

Declaration emit is separate from JS emit. It uses AST plus checked type and
symbol summaries to produce `.d.ts` text.

Key paths:

- `crates/tsz-emitter/src/declaration_emitter/`
- `crates/tsz-emitter/src/declaration_emitter/core/`
- `crates/tsz-emitter/src/declaration_emitter/exports/`
- `crates/tsz-emitter/src/declaration_emitter/helpers/`
- `crates/tsz-emitter/src/declaration_emitter/usage_analyzer/`
- `crates/tsz-emitter/src/type_cache_view.rs`

If declaration emit needs a semantic fact, the target architecture is to receive
that fact through a precomputed summary or cache view. Late semantic discovery in
emit is a boundary smell.

## Source Maps And Writers

Output writers and source-map helpers live under `crates/tsz-emitter/src/output/`
and are re-exported through `crates/tsz-core/src/lib.rs`. Source-map correctness
is tested in both emitter-specific tests and core source-map tests.

## Emit Verification

Broad emit parity is CI-owned. Local work should use focused checks and narrow
filters. The harness lives in `scripts/emit/`:

- `scripts/emit/run.sh`
- `scripts/emit/query-emit.py`
- `scripts/emit/audit-output-surgery.py`
- `scripts/emit/output-surgery-allowlist.txt`
- `scripts/emit/src/`

The output-surgery audit should remain clean: no unallowlisted late output
patches and no growing allowlist.
