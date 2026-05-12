# fix(checker): emit TS2309 for export assignment conflicts

Status: claim

Owner: Codex
Branch: fix/export-assignment-ts2309-20260512
Issue: #5841

## Scope

Restore the missing TS2309 diagnostic when a source file combines `export =`
with other exported elements. The first target is the minimal issue
reproduction where `tsc` reports TS1203 and TS2309 but `tsz` currently only
reports TS1203.

## Plan

- Find the current export-assignment diagnostic path and where source-file
  export declarations are summarized.
- Add checker-owned TS2309 emission without weakening existing TS1203 behavior.
- Add focused regression coverage and run the targeted conformance repro.

## Verification

- Pending.
