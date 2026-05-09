# perf(checker): precompile ambient module globs

- **Date**: 2026-05-07
- **Branch**: `perf/vite-vanilla-ts-bench`
- **PR**: TBD
- **Status**: ready
- **Workstream**: bench / perf-hotspots

## Intent

The `vite-vanilla-ts-app` project benchmark added in PR #4354 type-checks
a fresh `vanilla-ts` Vite app. The app pulls in `vite/client.d.ts`, which
declares ~30 wildcard ambient modules (`*.svg`, `*.css`, `*.module.css`,
`*.png`, etc.). On every wildcard match, four hot call sites in the
checker were rebuilding a `globset::Glob` and compiling a fresh matcher,
producing O(imports × patterns) glob compiles for any project that imports
ambient asset modules.

This PR pre-compiles all wildcard patterns into a single `globset::GlobSet`
when `GlobalDeclaredModules` is built, and switches the four hot sites
(`any_ambient_module_declared`, `wildcard_ambient_module_declared`,
`matches_ambient_module_pattern`, and the wildcard fallback in
`symbol_resolver_utils`) to a single `set.is_match(name)` call.

## Files Touched

- `crates/tsz-checker/src/context/global_declared_modules.rs` (new file,
  86 LOC) — extracted `GlobalDeclaredModules` struct + `finalize` /
  `matches_wildcard` helpers.
- `crates/tsz-checker/src/context/mod.rs` — removed inline definition,
  added `mod global_declared_modules; pub use ...;`.
- `crates/tsz-checker/src/context/core.rs` — call `dm.finalize()` after
  `patterns.sort()/dedup()` at the two builder sites.
- `crates/tsz-checker/src/declarations/import/core/ambient_modules.rs` —
  call `dm.matches_wildcard(name)` instead of per-pattern compile loop.
- `crates/tsz-checker/src/declarations/declarations_module_helpers.rs` —
  same.
- `crates/tsz-checker/src/symbols/symbol_resolver_utils.rs` — same.
- `crates/tsz-checker/tests/project_env_tests.rs` — locks the new
  behavior with a vite-style asset-pattern test.

## Verification

- `./scripts/bench/bench-vs-tsgo.sh --quick --filter 'vite-vanilla-ts-app' --json`
  - **Before**: tsz **188.04 ms**, factor 2.31× vs tsgo (81.39 ms).
  - **After**:  tsz **146.88 ms**, factor 2.07× vs tsgo (70.89 ms).
  - tsz wall-time saving ≈ 41 ms (~22% faster on this fixture).
- `cargo nextest run -p tsz-checker --lib` — `architecture_contract_tests::test_checker_file_size_ceiling`
  passes after splitting `mod.rs` (was 2045 LOC, now 1964 LOC).
- New unit test: `project_env_tests::global_declared_modules_matches_vite_style_asset_patterns`
  covers `*.css`, `*.svg`, `*.module.css` patterns.
- Three pre-existing test failures (`ts2300_tests::duplicate_identifier_with_default_lib_symbol_reports_lib_locations`,
  `ts2353_tests::recursive_array_union_excess_property_uses_outer_alias_display`,
  `js_constructor_property_tests::checked_js_prototype_plain_parent_method_call_reports_ts2531`)
  are environment-specific to this worktree; verified they fail with
  *and* without my changes here, and pass on the canonical checkout.
