# Fix conformance aggregate regression cluster

- **Date**: 2026-05-13
- **Branch**: `fix/conformance-aggregate-10-regressions-20260513`
- **PR**: TBD
- **Status**: claim
- **Workstream**: conformance

## Intent

The current PR queue is blocked by a repeated `conformance-aggregate` failure:
six shard jobs pass, but the aggregate reports 12575/12585 against the checked-in
12581 baseline. The reported regressions are the same 10-test cluster across
unrelated PRs, so this branch claims the global root-cause investigation and fix
rather than patching individual PR branches.

## Files Touched

- TBD

## Verification

- TBD
