Task: Workstream 5 large-repo fixture - record current main guarded RSS sample
Status: claim
Branch: `docs/large-repo-rss-main-20260502`

Scope:
- Update the Workstream 5 status snapshot with a current guarded
  `large-ts-repo` run on `main` after the latest merged perf and emit queue.
- Record the observed exit status and peak sampled physical footprint so the
  next performance slice can compare against a concrete current baseline.

Verification:
- Reuse the local guarded run already performed:
  `RUST_BACKTRACE=1 scripts/safe-run.sh --limit 75% --interval 2 --verbose -- .target-bench/dist/tsz --extendedDiagnostics --noEmit -p ~/code/large-ts-repo/tsconfig.flat.bench.json`.
