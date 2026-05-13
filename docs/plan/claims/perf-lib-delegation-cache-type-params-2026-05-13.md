# Claim: Preserve lib delegation cache type parameters

Date: 2026-05-13

## Claim

`lib_delegation_cache` must cache the resolved `TypeId` together with the
definition type parameters. Generic lib direct paths such as `ArrayIterator<T>`
return both pieces of data on a cache miss, so the cache needs to retain both
pieces for the next typed actual-lib query slice.

This is a preparatory performance-safety slice. It does not re-enable the failed
actual-lib utility-alias shortcut, and it leaves the existing lib cache-hit
return contract unchanged. Type aliases still stay on the child-checker fallback
path until a typed alias-body query or canonical `DefinitionStore` entry
preserves the alias shape under full conformance.

## Evidence

- `crates/tsz-checker/src/context/mod.rs`
  - changes `lib_delegation_cache` to store `(TypeId, Vec<TypeParamInfo>)`.
- `crates/tsz-checker/src/state/type_analysis/cross_file.rs`
  - propagates cached type parameters through direct writes and child-checker
    cache merge-back while preserving the current cache-hit return behavior.
- `crates/tsz-checker/src/state/type_analysis/cross_file_direct.rs`
  - records direct-path type parameters in the cache.
  - adds `direct_actual_lib_delegation_cache_preserves_type_params`, proving a
    generic actual-lib direct path caches the `ArrayIterator<T>` parameter list.

## Validation

- `cargo test -p tsz-checker --lib direct_actual_lib_delegation_cache_preserves_type_params -- --nocapture`
- `cargo check -p tsz-checker`
- `cargo fmt --all --check`
- `git diff --check`
