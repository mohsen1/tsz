# fix(cli): preserve files in --showConfig instead of validating root files

- **Date**: 2026-05-06
- **Branch**: `claude/nice-darwin-c4Hii`
- **PR**: TBD
- **Status**: claim
- **Workstream**: CLI parity with `tsc --showConfig`

## Intent

Closes #3919. `tsz --showConfig` currently runs source discovery via
`discover_ts_files` and converts an empty result into TS18003 before printing
the config. This means an explicit `files` entry with an unsupported
extension (e.g. `style.css`) or a missing path is rejected with TS18003 /
TS6053 instead of being preserved verbatim like `tsc --showConfig` does.

Fix: in the `--showConfig` code path, when `files` is explicitly set (CLI
or tsconfig), use that list directly and normalize each entry with a
`./` prefix without checking existence or extension. When `files` is not
set, still fall back to discovery but treat its empty result as "no files
to print" rather than as TS18003. TS18003 stays in the normal compile
path; it is no longer emitted from `handle_show_config`.

## Files Touched

- `crates/tsz-cli/src/bin/tsz.rs` (~30 LOC: rewrite the file-list build inside `handle_show_config`)
- `crates/tsz-cli/tests/tsc_compat_tests.rs` (new regression tests)

## Verification

- `cargo nextest run -p tsz-cli` (showConfig + driver tests)
- Manual repros from the issue produce parity with `tsc --showConfig`.
