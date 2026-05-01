Task: Workstream 5 large-repo fixture - record current guarded RSS sample
Status: claim
Branch: `docs/large-repo-rss-main-sample`

Scope:
- Update the Workstream 5 status snapshot with the current local large-ts-repo run now that the fixture exists at `~/code/large-ts-repo`.
- Record that the current main run no longer hits the non-total sort panic and plateaued below the 75% memory guard, but did not finish during the manual sample window.

Verification:
- Reuse the local guarded run already performed with `scripts/safe-run.sh --limit 75% --interval 2 --verbose -- .target-bench/dist/tsz --extendedDiagnostics --noEmit -p ~/code/large-ts-repo/tsconfig.flat.bench.json`.
