# Claim: Typed actual-lib alias body query

Date: 2026-05-13
Status: ready

## Claim

The next safe actual-lib alias slice should introduce a typed alias-body proof
surface instead of expanding a name-only utility-alias allowlist.

This PR adds a narrow query/helper that can return an actual bundled-lib
type alias body together with its `TypeParamInfo` list only when the symbol's
declarations prove builtin-lib ownership and the alias body can be lowered
through the existing lib resolver/definition-store path. This initial
behavior-changing slice admits only the decorator metadata aliases; unsupported
aliases stay on the existing child-checker fallback path.

## Scope

- Build on the merged `lib_delegation_cache` type-parameter metadata from
  #6389.
- Avoid the previously rejected broad shortcut for `FlatArray`,
  `IteratorResult`, and `Record` until the alias body shape is proven by the
  typed query.
- Keep behavior fallback-first: if the proof cannot establish ownership or
  alias shape, return `None`.
- Keep `PropertyKey` on fallback after the broader non-generic alias attempt
  failed conformance in assignability-sensitive lib signatures.

## Evidence

- `crates/tsz-checker/src/state/type_analysis/cross_file_direct.rs`
  - adds `direct_actual_lib_type_alias_body`, which requires proven actual-lib
    type-alias declarations, a `Lazy(DefId)` result, `DefKind::TypeAlias`, and a
    registered `DefinitionStore` body.
  - returns and caches the registered alias body, not the opaque `Lazy(DefId)`
    alias wrapper, so assignability and constraint checks see the structural
    body.
  - admits only `DecoratorMetadata` and `DecoratorMetadataObject` in this PR;
    aliases with type parameters and `PropertyKey` return `None`.
  - adds unit coverage proving the decorator metadata alias body lowers
    directly while `PropertyKey` and `Record` stay on fallback.
- `docs/plan/perf-runs/2026-05-13-delegate-actual-lib-alias-body-query.md`
  records monorepo-006 attribution evidence: diagnostics stay `10,198`,
  `DelegateCrossArenaSymbol` child-checkers drop `28 -> 26`,
  `delegate.misses` drops `30 -> 28`, and
  `checker.with_parent_cache_constructed` drops `31 -> 29`.

## Validation

- `cargo test -p tsz-checker --lib direct_actual_lib_symbol_type -- --nocapture`
- `cargo test -p tsz-checker --lib cross_file_direct -- --nocapture`
- `cargo test -p tsz-checker --test ts2322_tests test_ts2322_flatarray_assignment_keeps_rhs_declared_alias_display -- --nocapture`
- `cargo test -p tsz-checker --lib zod_issue_5030_defaults_path_with_lib_utility_aliases -- --nocapture`
- `cargo test -p tsz-checker --test generic_alias_assignability_pollution_tests -- --nocapture`
- `cargo test -p tsz-checker --test conformance_issues test_module_exports_define_property_does_not_fall_back_to_lib_signature -- --nocapture`
- `cargo test -p tsz-checker --test conformance_issues test_type_literal_bare_uint8array_does_not_poison_later_defaulted_refs -- --nocapture`
- `cargo test -p tsz-checker --test conformance_issues test_typed_array_to_locale_string_uses_options_parameter_type -- --nocapture`
- `cargo build -p tsz-cli --release --features perf-tools`
- `TSZ_PERF_COUNTERS=1 ./.target/release/tsz --project scripts/bench/scale-cliff/fixtures/monorepo-006/tsconfig.json --pretty false --noEmit --diagnostics-json .ci-logs/perf-actual-lib-alias-body-query/after-diag.json --perf-counters-json .ci-logs/perf-actual-lib-alias-body-query/after-pc.json` (expected exit `2`)
- `cargo fmt --all --check`
- `cargo check -p tsz-checker`
