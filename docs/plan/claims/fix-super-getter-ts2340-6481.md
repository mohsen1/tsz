# fix(checker): allow public super getter access

- **Date**: 2026-05-13
- **Branch**: `fix-super-getter-ts2340-6481`
- **PR**: TBD
- **Status**: claim
- **Workstream**: conformance/checker false positives

## Intent

Fix #6481, where `super.publicGetter` is incorrectly rejected with TS2340.
The implementation should preserve TypeScript compatibility by allowing
public and protected accessors through `super` while still rejecting invalid
private access.

## Files Touched

- `docs/plan/claims/fix-super-getter-ts2340-6481.md`

## Verification

- Pending.
