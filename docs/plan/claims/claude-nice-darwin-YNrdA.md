# fix(config): surface TS5024 from base configs and silent loader sites

- **Date**: 2026-05-07
- **Branch**: `claude/nice-darwin-YNrdA`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance — config-load TS5024 parity)

## Intent

Closes #3589. Two coupled regressions silently coerce string-typed booleans
into bools instead of reporting `tsc`'s `TS5024` diagnostic:

1. `load_tsconfig_inner_with_diagnostics` validates the *child* config but
   recurses into `extends` bases through the silent `load_tsconfig_inner`
   path. Invalid options in inherited configs survive into the merged
   `CompilerOptions`.
2. The CLI helpers `handle_show_config` (`--showConfig`) and
   `handle_list_files_only` (`--listFilesOnly`) call the silent
   `load_tsconfig` even for the *root* config, so root-level coercions
   never surface either.

Fix routes both paths through the diagnostic loader, aggregates base-config
diagnostics into the parent's diagnostic list (anchored at the base file's
own location, matching tsc's `base.json(L,C):` output), and ORs the
removed-but-honored suppress flags across the chain.

## Files Touched

- `crates/tsz-core/src/config/mod.rs` — recurse `load_tsconfig_inner_with_diagnostics`
  for base configs; OR `suppress_excess_property_errors` /
  `suppress_implicit_any_index_errors` / `no_implicit_use_strict` from bases.
- `crates/tsz-cli/src/bin/tsz.rs` — switch `handle_show_config` and
  `handle_list_files_only` to the diagnostic loader; print TS5024 / TS5025
  / TS5102 diagnostics and exit 1 when any error fires.
- `crates/tsz-core/src/config/tests/extends_diagnostic_tests.rs` (new) —
  lock TS5024 anchoring to the base config when the invalid option is
  inherited via `extends`.

## Verification

- `cargo test -p tsz-core --lib config::`
- `cargo test -p tsz-cli --tests tsc_compat_tests`
- Root-config repro and extends repro from #3589
