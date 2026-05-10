# perf(cli): T0.2 perf-build-only diagnostics JSON output

- **Date**: 2026-05-10
- **Branch**: `perf/t0.2-diagnostics-json-2026-05-10`
- **PR**: TBD (will draft as WIP)
- **Status**: claim
- **Workstream**: PERFORMANCE_PLAN.md Tier 0 — T0.2 / PR 1

## Intent

Implement perf-build-only `--diagnostics-json <path>` per PERFORMANCE_PLAN.md
§4.T0.2 / §11 PR 1. Default end-user `tsz` release builds must not expose the
flag and must not pay any extra runtime cost for emitting it.

## Approach

1. Add a `perf-tools` cargo feature to `tsz-cli` (default = off).
2. Behind `cfg(feature = "perf-tools")`, wire a `--diagnostics-json <path>`
   CLI flag on the existing `tsz` binary. The plan accepts either a separate
   `tsz-perf` binary or a `cfg`-gated flag — feature-gated is lower-friction.
3. Add a `tsz_cli::perf_json` module (also `cfg`-gated) that builds the
   `PerfDiagnosticsReport` struct and writes serde_json output.
4. Schema matches PERFORMANCE_PLAN.md §3:
   - `schema_version: 1`
   - `mode: "timing"` (counters off in this PR; T0.3 wires `attribution`)
   - `tsz`: version + commit + profile
   - `fixture`: name + repo + ref + actual_commit + path + local_override
   - `command_line`: argv
   - `phases_ms`: existing `PhaseTimings` plus `config_discovery`,
     `source_discovery`, `module_resolution` placeholders
   - `counts`: files / root_files / lib_files / source_bytes / diagnostics
   - `rss_peak_bytes`: RSS at end of run when available

## Fixture provenance

Read from environment variables set by the bench harness:
- `TSZ_BENCH_FIXTURE_NAME`, `TSZ_BENCH_FIXTURE_REPO`, `TSZ_BENCH_FIXTURE_REF`,
  `TSZ_BENCH_FIXTURE_PATH`, `TSZ_BENCH_FIXTURE_ACTUAL_COMMIT`,
  `TSZ_BENCH_ALLOW_LOCAL_FIXTURE` (already used by `bench-vs-tsgo.sh`).
- All optional. Missing values serialize as `null`.

## Files Touched (planned)

- `crates/tsz-cli/Cargo.toml` (`[features] perf-tools = []`)
- `crates/tsz-cli/src/commands/args.rs` (gated CLI flag)
- `crates/tsz-cli/src/perf_json.rs` (new, gated module)
- `crates/tsz-cli/src/lib.rs` (gated `pub mod perf_json`)
- `crates/tsz-cli/src/bin/tsz.rs` (gated call site after compilation)
- new test asserting JSON shape via `cargo test -p tsz-cli --features perf-tools`

## Verification

- `cargo build -p tsz-cli` (no feature) — default release build: no new flag,
  no new symbols in `--help` output.
- `cargo build -p tsz-cli --features perf-tools` — perf build: flag visible.
- `cargo nextest run -p tsz-cli --features perf-tools` — schema test passes.
- `target/release/tsz --diagnostics-json /tmp/diag.json fixture.ts` (perf
  build) emits valid schema-versioned JSON.
- `jq -r '.schema_version, .phases_ms.total' /tmp/diag.json` extracts cleanly.

## Out of scope (future PRs)

- T0.3 Perf-Counter JSON (`attribution` mode, `wired` map).
- T0.4 Phase split decision record.
- Wiring `bench-vs-tsgo.sh` to consume the JSON via `jq` (separate small PR).
