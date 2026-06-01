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
