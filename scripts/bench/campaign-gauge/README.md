# Campaign flag gauge (#14344 / #14345)

Committed, reproducible gauge for the identity + materialize-once campaign's
default-OFF flag stack. Replaces the hand-run numbers the campaign used to quote
and the dormant, unread `canonical-defid-harness` (removed in #15317).

## What it does

`run.sh [all|flag-tests|determinism|census]`

All modes export the composed substrate stack once (see `CAMPAIGN_FLAGS` in
`run.sh`, kept in sync with `CAMPAIGN_STORE_CHANNELS` in
`crates/tsz-solver/src/def/core/campaign_channels.rs`).

- **flag-tests** (GATING): runs the flag-ON tests designed to pass under the
  stack — the HKT body-publication tests in
  `crates/tsz-checker/tests/hkt_cross_file_augmentation_13653_repro.rs` plus the
  channel-registry / election-ordering unit tests. A regression here means a
  landed flag's own parity contract broke.
- **determinism** (GATING): builds (or reuses `TSZ_BIN`) the `tsz` CLI and
  compiles the committed `fixture/` cross-file HKT-augmentation project
  `TSZ_GAUGE_REPEATS` times (default 3), asserting the sorted diagnostic output
  is byte-identical across runs — the guard against the historical run-to-run
  election flap. When `TSZ_FPTS_DIR` points at an fp-ts fixture, the fp-ts row is
  also run and its diagnostic count asserted stable.
- **census** (NON-GATING): runs the full `tsz-solver` suite under the stack with
  `--no-fail-fast` and prints the pass/fail envelope into the job log. The 2^13
  flag composition space is **not** green by design — many unit tests encode
  flag-OFF expectations — so this is a crash/hang smoke run plus a printed
  snapshot (no committed baseline is diffed yet). A hard crash or hang is
  surfaced (process abort / job timeout); assertion deltas are informational.
- **all** (default): flag-tests, then determinism, then census.

## Why the stack forces `TSZ_DETERMINISTIC_STORE_ELECTION`

Two flap-driving channels (`TSZ_TYPEPARAM_DECL_IDENTITY`,
`TSZ_XARENA_HERITAGE_TYPEARG`) are read in `tsz-checker`, which the solver's
`deterministic_store_election_enabled()` cannot call into. The env derivation
added in #15317 covers them, and the gauge additionally forces the explicit
override so the lane is belt-and-suspenders deterministic.

## CI

`.github/workflows/campaign-flag-lane.yml` runs this nightly and on
`workflow_dispatch`.

## Local run

```
scripts/bench/campaign-gauge/run.sh              # full gauge
scripts/bench/campaign-gauge/run.sh determinism  # just the repeat-and-compare
TSZ_BIN=.target/dist-fast/tsz scripts/bench/campaign-gauge/run.sh determinism
```
