# Retired campaign flag gauge

This directory preserves the fixture from the pre-rewrite identity and
materialization campaign as historical evidence. The campaign depended on
deleted checker/solver packages and thirteen ambient feature channels; none of
those controls belong to the clean-slate compiler.

`run.sh` is intentionally an unavailable-status shim. It never exports the old
flags or reports a replacement result for an experiment that no longer exists.
Use `scripts/reset/seed-oracle.sh` for the R0 compatibility floor and
`scripts/perf/forced-parallel-project-determinism.sh` for real worker-count
determinism checks.

Git history at checkpoint `2770da88d4` contains the original gauge and its
interpretation.
