# perf(cli): gate-once the remaining inline resolver counters

**2026-05-10 13:55:00**

## Scope

PR #4966 introduced gate-once-cached counter wrappers for the three
filesystem probes (`count_is_file`, `count_is_dir`, `count_read_dir`)
and swept 21 call sites onto them. A Copilot review on that PR flagged
that *three other* inline `inc(&counters().X)` sites remained:

- `resolution.rs:1807` — virtual-root candidate path emission
  (path-mapping branch)
- `resolution.rs:1964` — suffix-extension candidate emission
- `resolution.rs:3111` — `read_package_json_uncached` per-package read

Each pays an unconditional `OnceLock<PerfCounters>` deref to compute
the field reference, even though `inc()` short-circuits on
`enabled_fast()` internally. The wrappers from #4966 fixed the same
shape of issue for the fs-probe trio; this PR extends the pattern to
the remaining three.

## Approach

Two new wrappers in the same module:

- `count_candidate_path()` — bumps `resolver_candidate_paths_total`,
  consumed by both candidate-emission sites.
- `count_read_package_json()` — bumps
  `resolver_read_package_json_calls`, consumed by the
  uncached-read site.

Both share the same gate-then-deref shape as the existing fs-probe
wrappers: `if enabled_fast() { inc(&counters().X) }`. Disabled mode
collapses to one cached-bool load; enabled mode pays one
`counters()` deref per increment.

## Behavior

No semantic change. Counter values for these fields under
`TSZ_PERF_COUNTERS=1` are unchanged. The disabled-mode cost drops by
one `OnceLock<PerfCounters>::get()` per call site invocation.

## Verification

- `cargo check -p tsz-cli` clean
- Pre-commit (fmt, clippy `-D warnings`, arch guard, full nextest
  suite) — to be confirmed by hook before push

## Conformance

No code-path semantics change; compile-out behavior of disabled
counters is the only delta. Snapshots unaffected.
