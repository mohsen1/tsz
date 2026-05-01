Task: Workstream 5 large-repo residency - share per-file declaration secondary index
Status: claim
Branch: `perf/bound-file-sym-decl-index-arc`
PR: pending

Plan:
- Add a per-file `sym_to_decl_indices` secondary index to `BoundFile`, built once during merge/remap.
- Reuse that index when reconstructing tsz-core per-file binders instead of rebuilding it from `declaration_arenas` per binder.
- Verify with focused core checks/tests and perf hotspot benchmarks; large-repo RSS will be noted as unavailable locally if `~/code/large-ts-repo` is still absent.
