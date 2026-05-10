# perf(checker): wire overlay entries_total/entries_max via Arc-snapshot total_entries

- **Date**: 2026-05-10
- **Branch**: `perf/t0-overlay-entries-audit-2026-05-10`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 5 (Performance) — Tier 0.3 follow-up

## Intent

Address the 2026-05-10 scale-cliff summary "Follow-up gaps" #4: the
attribution run reports `overlay.copy_calls = 820` on monorepo-006 but
`entries_total = 0` and `entries_max = 0`. The counter was hardcoded
to `record_overlay_copy(reason, 0)` because the architecture moved to
an Arc-snapshot model where nothing is *physically* copied at handoff
time.

The plan §4.T0.3 explicitly asks for "total inherited entries, max
entries, and size buckets ... keep the current Arc snapshot model;
this is to prove copy cost is gone." The right value to record is the
*visible* entry count the child can see (own delta + transitive
parent chain), which `SymbolFileTargetsNode` already tracks as
`total_entries`. This PR exposes that count and threads it through.

## Approach

1. Add `pub(super) const fn total_entries(&self) -> usize` accessor to
   `SymbolFileTargetsNode` in `crates/tsz-checker/src/context/symbol_file_targets.rs`.
2. In `crates/tsz-checker/src/context/core.rs`,
   `copy_symbol_file_targets_to_attributed`: replace
   `record_overlay_copy(reason, 0)` with
   `record_overlay_copy(reason, parent_snapshot.total_entries() as u64)`.

That's it. The Arc-snapshot model is preserved; the counter just reads
a `usize` field on the cached node instead of pretending the count is
zero.

## Files Touched

- `crates/tsz-checker/src/context/symbol_file_targets.rs` — add
  `total_entries()` accessor.
- `crates/tsz-checker/src/context/core.rs` —
  `copy_symbol_file_targets_to_attributed` now records the real
  visible-entry count.

## Verification

End-to-end attribution on a 3-file fixture with 2 cross-file imports:

| Counter | Before | After |
| --- | --- | --- |
| `copy_calls` | 33 | 33 |
| `entries_total` | **0** | **79** |
| `entries_max` | **0** | **8** |

`copy_calls` is unchanged — handoff frequency is the same. The
non-zero `entries_total` and `entries_max` now reflect the real
state-visibility through the chain. On a larger fixture this signal
distinguishes "many medium overlays" from "a few catastrophic large
overlays" (the size buckets `len_ge_{1k,10k,100k,1m}` continue to
work as before).

- `cargo nextest run -p tsz-checker --lib -E 'test(symbol_file_targets)'`
  — 3/3 pass.
- `cargo clippy -p tsz-checker --all-targets -- -D warnings` clean.

## No conformance / behavior impact

Pure instrumentation accuracy. No checker/solver behavior change.
