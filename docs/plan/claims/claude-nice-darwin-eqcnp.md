# fix(cli): showConfig handles explicit files without tsconfig (#3580)

- **Date**: 2026-05-07 19:25:00
- **Branch**: `claude/nice-darwin-eqcnp`
- **PR**: TBD
- **Status**: ready
- **Workstream**: CLI parity (`--showConfig`)

## Intent

Closes #3580. `tsz --showConfig` previously failed to match `tsc` for two
adjacent tsconfig-discovery scenarios:

1. Explicit files passed and a `tsconfig.json` discovered by walking up the
   filesystem: `tsz` silently inherited the walked-up config; `tsc` rejects
   the implicit pickup with TS5112.
2. No files and no tsconfig found anywhere: `tsz` printed `{}` and exited 0
   even when other CLI options were supplied; `tsc` emits TS5081.

The fix tracks whether the resolved tsconfig path was discovered via
walk-up (vs explicitly via `--project`) and gates TS5112 on that flag,
generalises the TS5081 emission to every "no tsconfig + no files" case,
and leaves the existing TS5057/TS5058 path for explicit `--project`
mistakes intact.

## Files Touched

- `crates/tsz-cli/src/bin/tsz.rs` — `handle_show_config` resolves
  `(tsconfig_path, discovered_via_walkup)`, emits TS5112 when the walk-up
  pickup is shadowed by explicit files, and emits TS5081 whenever no
  tsconfig is resolved and no explicit files are present.
- `crates/tsz-cli/tests/tsc_compat_tests.rs` — six new regressions: four
  direct (`show_config_*`) and two `tsc_parity_*` cross-checks against
  `tsc`.

## Verification

- `cargo test -p tsz-cli --test tsc_compat_tests show_config_explicit_files_without_any_tsconfig_synthesizes_config show_config_explicit_files_with_walkup_tsconfig_emits_ts5112 show_config_explicit_files_with_walkup_tsconfig_ignore_config_synthesizes show_config_no_files_no_tsconfig_with_cli_options_emits_ts5081 tsc_parity_show_config_explicit_files_no_tsconfig tsc_parity_show_config_explicit_files_walkup_tsconfig_ts5112` — 6 new tests pass.
- `scripts/safe-run.sh cargo test -p tsz-cli --tests` — 957 passed, 80
  failed; the 80 failures match the baseline on `claude/nice-darwin-eqcnp`
  before this change (verified by stashing the diff and re-running).
- `cargo clippy -p tsz-cli --all-targets -- -D warnings` — clean.
- `cargo fmt --check` — clean.
- Manual `tsc` parity check across six scenarios (no-tsconfig + explicit
  file, walk-up + explicit file, `--ignoreConfig` shadow, no anchor with
  CLI opts, no anchor without opts, walk-up + no files) — all match `tsc`
  exit code and output character-for-character.
