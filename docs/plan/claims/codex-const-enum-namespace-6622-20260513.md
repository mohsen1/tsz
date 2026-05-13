# Claim: rejected const enum namespace merge reports member access TS2339

Issue: #6622
Branch: codex/const-enum-namespace-6622-20260513
Status: ready

## Summary

When a const enum merge with a namespace is rejected by TS2567, namespace-exported values should not remain accessible through the enum object. The enum/namespace member fast path now reports TS2339 for non-enum-member namespace exports on rejected const-enum namespace merges while preserving enum member access.

## Files changed

- `crates/tsz-checker/src/types/property_access_type/resolve.rs`
- `crates/tsz-checker/tests/ts2567_augmentation_enum_cross_arena_decl_tests.rs`
- `docs/plan/claims/codex-const-enum-namespace-6622-20260513.md`

## Validation

- `cargo test -p tsz-checker --test ts2567_augmentation_enum_cross_arena_decl_tests const_enum_namespace_rejected_merge_hides_namespace_member -- --nocapture`
- `cargo test -p tsz-checker --test ts2567_augmentation_enum_cross_arena_decl_tests -- --nocapture`
- `cargo fmt --all --check`
