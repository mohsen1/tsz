# fix(server): report TypeScript version, not crate version, for `status`

- **Date**: 2026-05-06
- **Branch**: `claude/nice-darwin-Aqqfj`
- **PR**: TBD
- **Status**: ready
- **Workstream**: tsz-server protocol parity (issue #3745)

## Intent

`tsz-server`'s `handle_tsserver_request` returned `env!("CARGO_PKG_VERSION")`
(the `tsz-cli` crate version, currently `0.1.9`) for the `status` command.
TypeScript's `tsserver` reports the TypeScript version in this protocol field,
and clients use it as a server/version probe — so tsz looked like a TypeScript
`0.1.9` server. Switch the `status` body to `env!("TSZ_TSC_VERSION")`, the
already-existing build-time TS version constant used by `tsz_cli::help::TSC_VERSION`
for `tsz --version`.

## Files Touched

- `crates/tsz-cli/src/bin/tsz_server/main.rs` (~1 LOC: swap `CARGO_PKG_VERSION` → `TSZ_TSC_VERSION`)
- `crates/tsz-cli/src/bin/tsz_server/tests.rs` (+~22 LOC: regression test that asserts
  the `status` body's `version` matches `TSZ_TSC_VERSION` and is *not* the crate version)

## Verification

- `cargo test -p tsz-cli --bin tsz-server test_status_reports_typescript_version_not_crate_version` — passes
- `cargo test -p tsz-cli --bin tsz-server` — 260 passed, 1 pre-existing unrelated failure
  (`test_format_document_does_not_invalidate_fourslash_markers`, also fails on `main` without this change)
