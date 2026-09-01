//! Contiguous test shard split out of the parent module to satisfy the
//! source-file line cap.

use super::*;

/// Perf invariant: the per-evaluation *intermediate* seed/persist memo is
/// gated by a structural cache-size soft cap, not by any fixture/file/name.
///
/// Below the cap the memo runs; once the persistent `env_eval_cache` grows
/// past `ENV_EVAL_SEED_PERSIST_SOFT_CAP`, the O(cache_size)-per-call marshalling
/// is skipped to avoid O(N^2) blowup across alias-sharing type positions. The
/// gate is keyed only on cache length, so it triggers regardless of which
/// `DefId`s populate the cache.
#[test]
fn test_env_eval_seed_persist_gate_is_cache_size_keyed() {
    use crate::context::env_eval_cache::ENV_EVAL_SEED_PERSIST_SOFT_CAP;

    let arena = NodeArena::new();
    let binder = BinderState::new();
    let types = TypeInterner::new();
    let ctx = CheckerContext::new(
        &arena,
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );

    // Empty cache: memo enabled.
    assert!(
        ctx.env_eval_seed_persist_enabled(),
        "intermediate seed/persist must run while the cache is small"
    );

    // Fill exactly up to the cap with distinct lazy keys. DefId integers stand
    // in for arbitrary alias references — the gate must not depend on the
    // specific ids or any user-chosen name.
    for i in 0..ENV_EVAL_SEED_PERSIST_SOFT_CAP as u32 {
        let key = types.lazy(DefId(900_000 + i));
        ctx.cache_env_eval_result(key, TypeId::STRING, false);
    }
    assert!(
        ctx.env_eval_seed_persist_enabled(),
        "memo stays enabled at exactly the cap ({ENV_EVAL_SEED_PERSIST_SOFT_CAP})"
    );

    // One past the cap: the marshalling memo is skipped.
    let over = types.lazy(DefId(900_000 + ENV_EVAL_SEED_PERSIST_SOFT_CAP as u32));
    ctx.cache_env_eval_result(over, TypeId::NUMBER, false);
    assert!(
        !ctx.env_eval_seed_persist_enabled(),
        "memo must be skipped once the cache exceeds the soft cap"
    );
    assert!(
        ctx.env_eval_cache_seed_entries().is_empty(),
        "over-cap caches must not materialize speed-only seed batches"
    );
}

/// Residency invariant: the seed/persist cap bounds the active persistence
/// loop too, not only the next call's `env_eval_seed_persist_enabled` check.
/// Starting exactly at the cap may admit one more speed-only intermediate, but
/// it must not scan and insert an arbitrarily large drained evaluator batch.
#[test]
fn test_env_eval_persist_stops_after_soft_cap() {
    use crate::context::env_eval_cache::ENV_EVAL_SEED_PERSIST_SOFT_CAP;

    let arena = NodeArena::new();
    let binder = BinderState::new();
    let types = TypeInterner::new();
    let ctx = CheckerContext::new(
        &arena,
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );

    for i in 0..ENV_EVAL_SEED_PERSIST_SOFT_CAP as u32 {
        let key = types.lazy(DefId(910_000 + i));
        ctx.cache_env_eval_result(key, TypeId::STRING, false);
    }

    let first_bulk_key = types.lazy(DefId(920_001));
    let second_bulk_key = types.lazy(DefId(920_002));
    ctx.persist_env_eval_cache_entries(vec![
        (first_bulk_key, TypeId::NUMBER),
        (second_bulk_key, TypeId::BOOLEAN),
    ]);

    assert_eq!(
        ctx.lookup_env_eval_cache(first_bulk_key).map(|e| e.result),
        Some(TypeId::NUMBER),
        "the first speed-only intermediate may fill the cache one past the cap"
    );
    assert!(
        ctx.lookup_env_eval_cache(second_bulk_key).is_none(),
        "bulk persistence must stop once the structural cap is exceeded"
    );
}

/// Speed-only invariant: evaluator seed entries omit mappings that cannot be
/// represented faithfully or cannot save work. The top-level memo may record
/// `T -> T`, and may record `depth_exceeded` metadata, but fresh evaluator
/// seeds only carry `(TypeId, TypeId)` pairs.
#[test]
fn test_env_eval_seed_entries_skip_identity_and_depth_entries() {
    let arena = NodeArena::new();
    let binder = BinderState::new();
    let types = TypeInterner::new();
    let ctx = CheckerContext::new(
        &arena,
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );

    let identity = types.lazy(DefId(930_001));
    let useful_key = types.lazy(DefId(930_002));
    let useful_value = TypeId::STRING;
    let depth_key = types.lazy(DefId(930_003));
    let depth_value = TypeId::NUMBER;
    ctx.cache_env_eval_result(identity, identity, false);
    ctx.cache_env_eval_result(useful_key, useful_value, false);
    ctx.cache_env_eval_result(depth_key, depth_value, true);
    ctx.cache_env_eval_result(TypeId::STRING, TypeId::NUMBER, false);

    let seeds = ctx.env_eval_cache_seed_entries();
    assert!(
        !seeds.iter().any(|&(k, v)| k == identity && v == identity),
        "identity env-eval entries must not be marshalled into fresh evaluator seeds"
    );
    assert!(
        seeds
            .iter()
            .any(|&(k, v)| k == useful_key && v == useful_value),
        "non-identity env-eval entries remain available as speed-only seeds"
    );
    assert!(
        !seeds
            .iter()
            .any(|&(k, v)| k == depth_key && v == depth_value),
        "depth-exceeded entries need metadata propagation and must stay out of seed batches"
    );
    assert!(
        !seeds.iter().any(|&(k, _)| k == TypeId::STRING),
        "intrinsic keys cannot save evaluator work and must stay out of seed batches"
    );
}

/// Boundary invariant: `evaluate_type_with_cache` can skip draining evaluator
/// cache entries when the caller will not persist them. The env-eval path must
/// thread its `seed_persist` decision into that flag so over-cap caches avoid
/// materializing a discarded intermediate batch.
#[test]
fn test_env_eval_threads_seed_persist_to_cache_entry_collection() {
    let boundary = fs::read_to_string("src/query_boundaries/state/type_environment.rs")
        .expect("failed to read type_environment query boundary");
    assert!(
        boundary.contains("enum CacheEntryCollection")
            && boundary.contains("CacheEntryCollection::Collect")
            && boundary.contains("#[must_use]")
            && boundary.contains("matches!(cache_entry_collection, CacheEntryCollection::Collect)"),
        "evaluate_type_with_cache must expose a cache-entry collection gate"
    );

    let lazy = fs::read_to_string("src/state/type_environment/lazy.rs")
        .expect("failed to read lazy type environment");
    let collection_gate_count = lazy.matches("CacheEntryCollection::when_enabled").count();
    assert!(
        collection_gate_count >= 2
            && lazy.contains("seed_persist")
            && lazy.contains("let second_pass_seed_persist")
            && lazy.contains("second_pass_seed_persist"),
        "env-eval first pass must pass seed_persist and second pass must \
         recompute the gate before cache-entry collection"
    );
}

/// File-session invariant: type-position identifier resolution is cached by
/// file-local `NodeIndex`, so it must be reset before checking the next file.
#[test]
fn test_type_position_resolution_cache_clears_at_file_boundary() {
    let reset_source = fs::read_to_string("src/context/file_session_reset.rs")
        .expect("failed to read file_session_reset.rs");
    assert!(
        reset_source.contains("type_position_resolution_cache")
            && reset_source.contains("type_position_resolution_cache.borrow_mut().clear()"),
        "type-position node-index cache must clear at the file-session boundary"
    );
}

/// Boundary invariant: every `evaluate_type_with_cache` call must pass the
/// explicit cache-entry collection flag. This keeps the speed-only drain gate
/// visible at call sites instead of relying on a hidden default.
#[test]
fn test_evaluate_type_with_cache_call_sites_pass_collection_flag() {
    let files = [
        "src/state/type_environment/lazy.rs",
        "src/state/type_resolution/constructors/callable_type_arguments.rs",
        "src/types/type_node_advanced.rs",
        "src/types/type_node_query_members.rs",
        "src/types/type_node_advanced/indexed_access_type.rs",
    ];

    for file in files {
        let src =
            fs::read_to_string(file).unwrap_or_else(|err| panic!("failed to read {file}: {err}"));
        for (line_no, line) in src.lines().enumerate() {
            if line.contains("evaluate_type_with_cache(") {
                let tail = src
                    .lines()
                    .skip(line_no)
                    .take(24)
                    .collect::<Vec<_>>()
                    .join("\n");
                assert!(
                    tail.contains("CacheEntryCollection::"),
                    "{file}:{} evaluate_type_with_cache call must pass explicit \
                     collect_cache_entries flag",
                    line_no + 1
                );
            }
        }
    }

    for file in files
        .into_iter()
        .filter(|file| *file != "src/state/type_environment/lazy.rs")
    {
        let src =
            fs::read_to_string(file).unwrap_or_else(|err| panic!("failed to read {file}: {err}"));
        for (line_no, line) in src.lines().enumerate() {
            if line.contains("evaluate_type_with_cache(") {
                let tail = src
                    .lines()
                    .skip(line_no)
                    .take(24)
                    .collect::<Vec<_>>()
                    .join("\n");
                assert!(
                    tail.contains("CacheEntryCollection::Skip"),
                    "{file}:{} result-only evaluate_type_with_cache call must not \
                     materialize discarded cache entries",
                    line_no + 1
                );
            }
        }
    }
}

/// Boundary behavior: disabling cache-entry collection must not change the
/// evaluated result. It only suppresses the speed-only drained intermediate
/// batch returned to env-eval persistence.
#[test]
fn test_evaluate_type_with_cache_can_skip_cache_entry_collection() {
    let types = TypeInterner::new();
    let indexed = types.index_access(types.array(TypeId::STRING), TypeId::NUMBER);
    let seed = std::iter::empty::<(TypeId, TypeId)>();
    let has_seed = false;
    let evaluated = crate::query_boundaries::state::type_environment::evaluate_type_with_cache(
        &types,
        &tsz_solver::def::resolver::NoopResolver,
        indexed,
        seed,
        has_seed,
        crate::query_boundaries::state::type_environment::EvaluateTypeWithCacheOptions {
            expand_application_display_alias_args: false,
            query_db: None,
            authoritative: false,
            cache_entry_collection:
                crate::query_boundaries::state::type_environment::CacheEntryCollection::Skip,
        },
    );

    assert_eq!(evaluated.result, TypeId::STRING);
    assert!(
        evaluated.cache_entries.is_empty(),
        "collect_cache_entries=false must suppress drained intermediate cache entries"
    );
}

/// Fast-path invariant: when evaluator cache-entry collection is disabled, the
/// persistence helper receives an empty batch. It must return before touching
/// declaration-file or cache-cap logic so disabled collection has near-zero
/// follow-up cost.
#[test]
fn test_env_eval_persist_empty_entries_fast_path_is_first() {
    let src =
        fs::read_to_string("src/context/env_eval_cache.rs").expect("failed to read env_eval_cache");
    let body = src
        .split("pub(crate) fn persist_env_eval_cache_entries")
        .nth(1)
        .expect("persist_env_eval_cache_entries exists");
    let empty_check = body
        .find("if entries.is_empty()")
        .expect("empty-entry fast path exists");
    let declaration_check = body
        .find("self.is_declaration_file()")
        .expect("declaration-file guard exists");
    let cap_check = body
        .find("ENV_EVAL_SEED_PERSIST_SOFT_CAP")
        .expect("cache cap guard exists");

    assert!(
        empty_check < declaration_check && empty_check < cap_check,
        "empty cache-entry batches must return before declaration-file or cap checks"
    );
}

/// Filter invariant: intrinsic env-eval results cannot contain `this`,
/// `infer`, type queries, unions, or applications. The persistence filter
/// should therefore avoid the recursive marker scans and union->application
/// poisoning guard for intrinsic values.
#[test]
fn test_env_eval_persist_intrinsic_results_skip_shape_queries() {
    let src =
        fs::read_to_string("src/context/env_eval_cache.rs").expect("failed to read env_eval_cache");
    let key_skip = src
        .find("if k == v || k.is_intrinsic()")
        .expect("identity and intrinsic keys are skipped before recursive key checks");
    assert!(
        src.contains("let key_contains_this = contains_this_type(self.types, k);")
            && src.contains("if key_contains_this"),
        "non-intrinsic key-side entries must use the cached key_contains_this fact"
    );
    let result_local = src
        .find("let result_is_intrinsic = v.is_intrinsic();")
        .expect("result intrinsic local exists");
    assert!(
        src.contains("let result_is_intrinsic = v.is_intrinsic();")
            && src.contains("result_is_intrinsic")
            && src.contains("!result_is_intrinsic")
            && src.contains("is_application_type(self.types, v)"),
        "env-eval persistence must fast-path intrinsic results before \
         structured result-shape queries"
    );
    assert!(
        key_skip < result_local,
        "identity/intrinsic/this-bearing keys must skip before result-side shape checks"
    );
}

/// Correctness invariant: skipping the intermediate seed/persist memo must not
/// affect the authoritative top-level result memo. Even when the cache is far
/// over the cap (gate skipping intermediate persistence), `cache_env_eval_result`
/// / `lookup_env_eval_cache` still record and return results exactly.
#[test]
fn test_top_level_env_eval_memo_unaffected_by_seed_cap() {
    use crate::context::env_eval_cache::ENV_EVAL_SEED_PERSIST_SOFT_CAP;

    let arena = NodeArena::new();
    let binder = BinderState::new();
    let types = TypeInterner::new();
    let ctx = CheckerContext::new(
        &arena,
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );

    // Push the cache well past the cap so the intermediate memo is disabled.
    for i in 0..(ENV_EVAL_SEED_PERSIST_SOFT_CAP as u32 + 16) {
        let key = types.lazy(DefId(800_000 + i));
        ctx.cache_env_eval_result(key, TypeId::STRING, false);
    }
    assert!(
        !ctx.env_eval_seed_persist_enabled(),
        "precondition: cache is over the cap"
    );

    // The top-level memo must still store and return an exact result.
    let probe_key = types.lazy(DefId(700_001));
    let probe_result = types.lazy(DefId(700_002));
    ctx.cache_env_eval_result(probe_key, probe_result, false);
    assert_eq!(
        ctx.lookup_env_eval_cache(probe_key).map(|e| e.result),
        Some(probe_result),
        "top-level env-eval memo must be unaffected by the seed/persist cap"
    );
}

/// Kill-switch: `TSZ_DISABLE_ENV_EVAL_SEED_CAP` forces the legacy always-on
/// behavior. This guard documents the A/B contract — with the switch set the
/// gate never skips, so cap-on vs cap-off output must be byte-identical.
#[test]
fn test_env_eval_seed_cap_killswitch_contract() {
    // The killswitch reader is process-wide and cached; assert only that the
    // default (unset) value is `false` so the cap is active by default. Setting
    // the env var here would poison the OnceLock for sibling tests, so the
    // forced-on path is covered by the conformance A/B harness instead.
    assert!(
        !crate::context::env_eval_cache::env_eval_seed_cap_disabled()
            || std::env::var_os("TSZ_DISABLE_ENV_EVAL_SEED_CAP").is_some(),
        "seed cap must be active unless TSZ_DISABLE_ENV_EVAL_SEED_CAP is set"
    );
}

/// Guard: `ensure_def_ready_for_lowering` delegates to
/// `extract_declared_type_params_for_reference_symbol` (not inline loops).
#[test]
fn test_ensure_def_ready_delegates_to_extract_declared_params() {
    let src = fs::read_to_string("src/state/type_resolution/reference_helpers.rs")
        .expect("failed to read src/state/type_resolution/reference_helpers.rs");

    // Find the ensure_def_ready_for_lowering body and check it calls
    // extract_declared_type_params_for_reference_symbol
    let in_helper = src
        .lines()
        .skip_while(|line| !line.contains("fn ensure_def_ready_for_lowering"))
        .take(30)
        .any(|line| line.contains("extract_declared_type_params_for_reference_symbol"));

    assert!(
        in_helper,
        "ensure_def_ready_for_lowering must delegate to \
         extract_declared_type_params_for_reference_symbol for type-param \
         extraction — no inline declaration iteration."
    );
}

/// Guard: `namespace_checker.rs` must NOT directly construct `TypeData::Lazy`
/// outside of documented exceptions for pure-namespace member handling.
///
/// Namespace types should use structural object types (via `build_namespace_object_type`)
/// or stable-identity helpers — except for pure-namespace sub-members which require
/// Lazy(DefId) to avoid infinite recursion during subtype checks.
#[test]
fn test_namespace_checker_no_raw_lazy_construction() {
    let src = fs::read_to_string("src/declarations/namespace_checker.rs")
        .expect("failed to read src/declarations/namespace_checker.rs");

    // Count occurrences of .lazy( outside comments
    let lazy_count = src
        .lines()
        .filter(|line| !line.trim().starts_with("//"))
        .filter(|line| line.contains(".lazy("))
        .count();

    // Currently 2 allowed usages for pure-namespace members:
    // 1. get_type_of_class_namespace_member (line ~264)
    // 2. build_namespace_object_type for is_pure_namespace (line ~774)
    const ALLOWED_LAZY_COUNT: usize = 2;

    assert!(
        lazy_count <= ALLOWED_LAZY_COUNT,
        "namespace_checker.rs has {lazy_count} .lazy() calls (allowed: {ALLOWED_LAZY_COUNT}). \
         Namespace types should use structural object types \
         (build_namespace_object_type) or stable-identity helpers. \
        Only pure-namespace sub-members may use Lazy(DefId) to avoid recursion."
    );
}

/// Guard: diagnostic-bearing assignability paths should use named
/// `RelationOutcome` helpers instead of locally constructing relation requests.
#[test]
fn test_assignability_diagnostics_route_through_relation_outcome_helpers() {
    let relation_src = fs::read_to_string("src/assignability/assignability_relation.rs")
        .expect("failed to read src/assignability/assignability_relation.rs");
    for helper in [
        "fn assign_relation_outcome",
        "fn call_arg_relation_outcome",
        "fn bivariant_callbacks_relation_outcome",
    ] {
        assert!(
            relation_src.contains(helper),
            "assignability_relation.rs must expose {helper} for diagnostic relation decisions"
        );
    }

    let diagnostic_files = [
        "src/assignability/assignability_diagnostics.rs",
        "src/assignability/assignment_checker/destructuring.rs",
    ];
    let forbidden = [
        "RelationRequest::assign(",
        "RelationRequest::call_arg(",
        "RelationRequest::bivariant_callbacks(",
    ];

    let mut violations = Vec::new();
    for path in diagnostic_files {
        let source = fs::read_to_string(path)
            .unwrap_or_else(|_| panic!("failed to read {path} for architecture guard"));
        for pattern in forbidden {
            if source.contains(pattern) {
                violations.push(format!("{path} contains {pattern}"));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "diagnostic assignability paths should call named RelationOutcome helpers; violations:\n{}",
        violations.join("\n")
    );
}

/// Guard: the checker-side downgrade pattern is centralized in a single
/// `apply_checker_side_downgrade` helper so every consumer agrees on the
/// "solver said related, checker overrides to not-related" semantics rather
/// than open-coding the conditional at each callsite. The reason that backs
/// the downgrade is consumed by the TS2322/TS2345 emit path through
/// `analyze_assignability_failure` (in `error_reporter/assignability.rs:602`),
/// which runs `raw_input_failure_reason` as a pre-pass — so the elaborated
/// diagnostic chain stays intact without the helper itself populating
/// `outcome.failure`. Populating `outcome.failure` from the downgrade has
/// unrelated semantic side-effects on `outcome.failure`-reading predicates in
/// `core_statement_checks.rs` (see #12239 conformance regression on
/// `coAndContraVariantInferences2.ts` and `correlatedUnions.ts`).
#[test]
fn test_checker_only_downgrade_preserves_failure_reason_through_gateway() {
    let source = fs::read_to_string("src/assignability/assignability_relation.rs")
        .expect("failed to read src/assignability/assignability_relation.rs");

    assert!(
        source.contains("fn apply_checker_side_downgrade("),
        "assignability_relation.rs must define a single apply_checker_side_downgrade \
         helper so all checker-side downgrades agree on the downgrade semantics"
    );

    let downgrade_body = extract_method_body(&source, "fn apply_checker_side_downgrade(");
    assert!(
        downgrade_body.contains("outcome.related = false;"),
        "apply_checker_side_downgrade must force outcome.related to false when the \
         checker-only reason fires"
    );
    assert!(
        downgrade_body.contains("checker_only_assignability_failure_reason("),
        "apply_checker_side_downgrade must consult \
         checker_only_assignability_failure_reason to decide whether to downgrade"
    );

    let exec_body = extract_method_body(&source, "fn execute_relation_request(");
    assert!(
        exec_body.contains("apply_checker_side_downgrade("),
        "execute_relation_request must route checker-side downgrades through \
         apply_checker_side_downgrade so the gateway's downgrade semantics live \
         in one place"
    );
}

/// Slice the body of a Rust method from `source` starting at `fn_signature`
/// and ending at the matching `}` (assumes 4-space indented method body).
fn extract_method_body<'a>(source: &'a str, fn_signature: &str) -> &'a str {
    let start = source
        .find(fn_signature)
        .unwrap_or_else(|| panic!("{fn_signature} not found"));
    let after = &source[start..];
    let end = after
        .find("\n    }\n")
        .unwrap_or_else(|| panic!("{fn_signature} must close with a method-end '}}'"));
    &after[..end]
}
