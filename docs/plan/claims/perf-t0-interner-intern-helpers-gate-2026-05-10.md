# perf(solver): gate-once the intern_* helper counter sites

**2026-05-10 14:50:00**

## Scope

The main `TypeInterner::intern()` function has used the gate-once-cached
counter pattern since #4960 — `enabled_fast()` is checked first, then
`counters()` is dereffed once and the resulting `Option<&PerfCounters>`
is reused across all increments inside the function.

Eight sibling `intern_*` helper functions still use the inline
`inc(&counters().X)` shape, paying an unconditional
`OnceLock<PerfCounters>::get()` every call:

- `intern_string` — fires per identifier/string interning (millions/run)
- `intern_type_list` and `intern_type_list_from_slice` — share
  `interner_type_list_intern_calls`
- `intern_object_shape` — interner_object_shape_intern_calls
- `intern_function_shape` — interner_function_shape_intern_calls
- `intern_conditional_type` — interner_conditional_intern_calls
- `intern_mapped_type` — interner_mapped_intern_calls
- `intern_application` — interner_application_intern_calls

All eight are hot. `intern_string` is the highest-frequency one
(`large-ts-repo` profiles measured ~7M string interns).

## Approach

Each call site now follows the same `if enabled_fast()` gate as the
checker hot-path PR (#5004) and the resolver wrappers (#4966, #5000):

```rust
if tsz_common::perf_counters::enabled_fast() {
    tsz_common::perf_counters::inc(
        &tsz_common::perf_counters::counters().X,
    );
}
```

Single-increment functions like these are the simplest case for the
gate — there's no way to amortize the `counters()` deref across
multiple increments because there's only one. The win is purely "skip
the deref entirely in disabled mode."

## Behavior

- **Enabled mode** (`TSZ_PERF_COUNTERS=1`): counter values unchanged.
- **Disabled mode** (default): each call site drops one
  `OnceLock<PerfCounters>::get()` per invocation. Compounds heavily
  across `intern_string` and `intern_type_list` which fire at the
  millions-per-compile scale.

No semantic change.

## Verification

- `cargo check -p tsz-solver` clean
- Pre-commit hook (fmt, clippy, arch guard, full nextest suite) — to
  be confirmed before push

## Conformance

Counter values, diagnostics, and conformance snapshots are unaffected;
this is pure perf-counter wiring.
