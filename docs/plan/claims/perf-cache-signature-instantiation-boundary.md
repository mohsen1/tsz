Task: Workstream 5 instantiate_type cache - route signature instantiation through query boundary
Status: claim
Owner: Codex
Branch: perf/cache-signature-instantiation-boundary
Created: 2026-05-01T22:05:21Z

Scope:
- Replace the compact `signature_builder` direct `tsz_solver::instantiate_type`
  call sites with the checker query-boundary wrapper so repeated generic
  signature substitutions can use the shared `QueryCache` instantiation cache.
- Keep the slice narrow and avoid overlapping the open solver intrinsic
  fast-path PR.

Verification plan:
- `cargo check -p tsz-checker`
- Targeted checker tests around signature instantiation/call behavior if a
  focused test name is available.
- `scripts/bench/perf-hotspots.sh --quick`
- Guarded `large-ts-repo` RSS sample because this is Workstream 5 cache work.
