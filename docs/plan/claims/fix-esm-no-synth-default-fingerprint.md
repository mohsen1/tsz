# fix(checker): align ESM no synthesized default diagnostics

- **Date**: 2026-04-28
- **Branch**: `fix/esm-no-synth-default-fingerprint`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance and fingerprints)

## Intent

Fix the fingerprint-only conformance mismatch for `esmNoSynthesizedDefault.ts`, where tsz emits the right `TS1192` and `TS2339` codes but does not match tsc's diagnostic tuple details. The fix should keep module export semantics in the checker boundary and avoid display-only patching unless the root cause is diagnostic rendering.

## Files Touched

- `crates/tsz-checker/src/state/type_analysis/computed_helpers_binding.rs`
- `crates/tsz-checker/tests/conformance_issues/errors/accessors.rs`

## Verification

- `cargo check --package tsz-checker`
- `cargo nextest run -p tsz-checker --test conformance_issues test_bare_esm_package_without_default_uses_resolved_node_modules_display`
- `CARGO_INCREMENTAL=0 cargo build --profile dist-fast -p tsz-cli -p tsz-conformance`
- `.target/dist-fast/tsz-conformance --filter esmNoSynthesizedDefault --verbose --print-fingerprints --test-dir /Users/mohsen/code/tsz/.claude/worktrees/emit-dts-worktree/TypeScript/tests/cases --cache-file scripts/conformance/tsc-cache-full.json --tsz-binary .target/dist-fast/tsz --workers 1 --no-batch`
- `.target/dist-fast/tsz-conformance --sample-size 200 --verbose --print-fingerprints --test-dir /Users/mohsen/code/tsz/.claude/worktrees/emit-dts-worktree/TypeScript/tests/cases --cache-file scripts/conformance/tsc-cache-full.json --tsz-binary .target/dist-fast/tsz --workers 1 --no-batch`
  - Result: 199/200 passed; the only failure was `aliasOnMergedModuleInterface.ts`, a pre-existing missing `TS2708` case in the checked-in conformance detail.
- `.target/dist-fast/tsz-conformance --verbose --print-fingerprints --test-dir /Users/mohsen/code/tsz/.claude/worktrees/emit-dts-worktree/TypeScript/tests/cases --cache-file scripts/conformance/tsc-cache-full.json --tsz-binary .target/dist-fast/tsz --workers 1 --no-batch`
  - Result: 12096/12582 passed (96.1%), 3 skipped, 23 known failures, 225 fingerprint-only. This external TypeScript worktree is not directly comparable to the checked-in snapshot because the local `TypeScript` submodule checkout was unavailable after `scripts/session/quick-pick.sh` failed to initialize it.

## Notes

- `./scripts/conformance/conformance.sh --filter esmNoSynthesizedDefault --verbose --print-fingerprints` could not complete because its dependency install step failed DNS resolution for `registry.npmjs.org`.
