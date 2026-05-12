# chore(lsp): consolidate reference node collection

- **Date**: 2026-05-12
- **Branch**: `dry-lsp-reference-node-collection-20260512`
- **PR**: TBD
- **Status**: claim
- **Workstream**: DRY audit P1 §3 (LSP Provider Context And Reference Occurrence Model)

## Intent

`FindReferences` repeats the same reference-node assembly path in several entry
points: collect reference nodes, append declarations, sort by node id,
deduplicate, and map to LSP locations. This PR extracts that flow into a single
provider-local helper so reference, rename-location, and reference-info paths
share one ordering and deduplication contract.

## Files Touched

- `crates/tsz-lsp/src/navigation/references.rs` (~40 LOC change)
- `docs/plan/claims/dry-lsp-reference-node-collection-20260512.md`

## Verification

- Pending
