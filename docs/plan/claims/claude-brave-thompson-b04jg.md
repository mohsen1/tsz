# checker: emit TS2374 on lib bodies of merged interface duplicate index signatures

- **Date**: 2026-04-30
- **Branch**: `claude/brave-thompson-b04jg`
- **PR**: TBD
- **Status**: ready
- **Workstream**: conformance (fingerprint-only TS2374 cross-arena emission)

## Intent

When a user-file interface (`String`, `Array<T>`, ...) merges with a default-lib
interface that also declares a same-kind index signature, tsc reports TS2374
on **every** participating signature — including the lib-side one. The existing
checker paths (`check_index_signature_compatibility` and the local-merge
branch in `check_merged_interface_declaration_diagnostics`) only emit on
user-arena nodes, so the lib's index signature was silently missed and the
test stayed `fingerprint-only`. This change adds a small cross-arena pass
that walks every merged interface symbol with both local and remote bodies,
counts same-kind index signatures (`number`/`string`/`symbol`), and emits
TS2374 at each remote body's index signature when the merged count is `>= 2`.
Local-body emissions remain owned by the existing paths to avoid double-firing.

## Files Touched

- `crates/tsz-checker/src/types/type_checking/duplicate_index_signatures.rs`
  (new module: `check_lib_merged_interface_duplicate_index_signatures` +
  per-symbol scanner; ~270 LOC)
- `crates/tsz-checker/src/types/type_checking/mod.rs` (+1 LOC: register the
  new module)
- `crates/tsz-checker/src/state/state_checking/source_file.rs` (+1 LOC:
  call the new pass after `check_duplicate_identifiers`)
- `crates/tsz-checker/Cargo.toml` (+4 LOC: register the new test target)
- `crates/tsz-checker/tests/ts2374_lib_merged_index_signature_tests.rs`
  (new test: 3 cases — String/Array merge, Number negative case)
- `scripts/session/spin.sh` (new minimal random-failure picker)

## Verification

- `cargo test -p tsz-checker --test ts2374_lib_merged_index_signature_tests`
  → 3 passed
- `./scripts/conformance/conformance.sh run --filter "duplicateNumericIndexers"`
  → 1/1 passed (was fingerprint-only fail)
- `./scripts/conformance/conformance.sh run --max 200` → 200/200 passed,
  no regressions vs. baseline
- `cargo test -p tsz-checker --lib` → 3058 passed, 0 failed (incl.
  `architecture_contract_tests::test_checker_file_size_ceiling`)
- `cargo test -p tsz-solver --lib` → 5558 passed, 0 failed
- `cargo fmt --all --check` → clean
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  → clean (run twice across the refactor)
