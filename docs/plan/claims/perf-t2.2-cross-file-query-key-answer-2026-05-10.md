# perf(checker): T2.2 prep — typed CrossFileQueryKey + Answer

- **Date**: 2026-05-10
- **Branch**: `perf/t2.2-cross-file-query-key-answer-2026-05-10`
- **PR**: TBD
- **Status**: claim
- **Workstream**: T2.2 PR 6B+ preparation (PERFORMANCE_PLAN.md §7)

## Intent

Second foundational step toward the typed cross-file query API the
plan §7 prescribes. #5043 introduced `CrossFileQueryKind`; this PR
adds the remaining two API types from §7's contract:

```rust
#[derive(Clone, Hash, PartialEq, Eq)]
pub struct CrossFileQueryKey {
    pub kind: CrossFileQueryKind,
    pub target_file_idx: u32,
    pub symbol_id: SymbolId,
    pub request_key: Option<RequestCacheKey>,
    pub options_fingerprint: u64,
}

pub enum CrossFileQueryAnswer {
    Type(TypeId),
    TypeWithParams(TypeId, Vec<TypeParamInfo>),
    MemberType { member: Atom, ty: TypeId },
    Unknown,
    Error,
}
```

Both types are `pub(crate)` and intentionally unused in this initial
pass. They exist so subsequent PR 6B+ migrations can reference them
from day one without introducing the type alongside the migration.

## Files Touched

- `crates/tsz-checker/src/state/type_analysis/cross_file_query_types.rs`
  (new sibling module to `cross_file.rs`):
  - Defines `CrossFileQueryKey` struct with `Clone + Debug + Hash + PartialEq + Eq` derives.
  - Defines `CrossFileQueryAnswer` enum with `Clone + Debug` derives.
    Five variants (`Type`, `TypeWithParams`, `MemberType`, `Unknown`,
    `Error`) matching the plan's API verbatim.
  - Doc comments cite §7's cache-key requirements and explain how each
    field projects onto today's `(u8, file_idx, primary, secondary,
    args_hash)` storage layer.

  Lives in its own file rather than next to `CrossFileQueryKind` because
  `cross_file.rs` is already at the 2000-LOC arch-guard limit. The
  module-level docstring captures this rationale so future agents
  understand the split.

- `crates/tsz-checker/src/state/type_analysis/mod.rs`: declare the new
  `pub(crate) mod cross_file_query_types`.

Three unit tests:
    - `key_implements_required_traits` — compile-time check that the
      derives we need (Clone + Debug + Hash + Eq) are intact.
    - `key_hash_and_eq_round_trip` — `HashMap<CrossFileQueryKey, _>`
      round-trip works.
    - `answer_variants_constructible` — smoke test for all 5 variants.

## Verification

- `cargo check -p tsz-checker` clean
- `cargo clippy -p tsz-checker --all-targets -- -D warnings` clean
- `cargo nextest run -p tsz-checker --lib -E 'test(cross_file_query_key_answer)'`
  — 3/3 tests pass
- Pre-commit hook (fmt, clippy, arch guard, full nextest suite) — to
  be confirmed before push

## Conformance

No semantic change. New types are not yet referenced from any callers.
Conformance unaffected.
