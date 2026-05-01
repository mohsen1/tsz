Task: Workstream 5 large-repo residency - share per-file declaration secondary index
Status: ready
Branch: `perf/bound-file-sym-decl-index-arc`
PR: #2135

Plan:
- Add a per-file `sym_to_decl_indices` secondary index to `BoundFile`, built once during merge/remap.
- Reuse that index when reconstructing tsz-core per-file binders instead of rebuilding it from `declaration_arenas` per binder.
- Verify with focused core checks/tests, perf hotspot benchmarks, and a guarded large-repo RSS sample.
