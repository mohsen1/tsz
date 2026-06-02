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

/// Guard: when the checker downgrades a solver-related outcome via
/// `checker_only_assignability_failure_reason` (iterator-result mismatches and
/// peers), the structured failure reason MUST be surfaced into the
/// `RelationOutcome.failure` slot. Otherwise TS2322/TS2345/TS2416 emit paths
/// observe a degenerate `(related=false, failure=None)` pair and render the
/// generic "Type 'X' is not assignable to type 'Y'." with no inner elaboration,
/// breaking the structural rule that the `query_boundaries/assignability`
/// gateway always returns a coherent decision-plus-reason pair.
#[test]
fn test_checker_only_downgrade_preserves_failure_reason_through_gateway() {
    let source = fs::read_to_string("src/assignability/assignability_relation.rs")
        .expect("failed to read src/assignability/assignability_relation.rs");

    // The shared downgrade helper must exist so every consumer agrees on the
    // "downgrade + reason recovery" semantics rather than open-coding the
    // pattern at each callsite.
    assert!(
        source.contains("fn apply_checker_side_downgrade("),
        "assignability_relation.rs must define a single apply_checker_side_downgrade \
         helper so all checker-side downgrades agree on preserving the failure reason"
    );

    let downgrade_body = extract_method_body(&source, "fn apply_checker_side_downgrade(");
    assert!(
        downgrade_body.contains("outcome.related = false;"),
        "apply_checker_side_downgrade must force outcome.related to false"
    );
    // The reason surfacing may be inlined OR delegated to the shared
    // `set_failure_from_reason_if_empty` helper. Accept either shape.
    let surfaces_reason_inline = downgrade_body.contains("outcome.failure = Some(")
        && downgrade_body.contains("RelationFailure::from_solver_reason(");
    let surfaces_reason_via_helper =
        downgrade_body.contains("set_failure_from_reason_if_empty(");
    assert!(
        surfaces_reason_inline || surfaces_reason_via_helper,
        "apply_checker_side_downgrade must surface the structured failure reason \
         so callers don't observe (related=false, failure=None)"
    );

    // The shared helper itself must always wrap the solver reason into a
    // `RelationFailure` and assign it iff `outcome.failure` is currently None.
    let helper_body =
        extract_method_body(&source, "fn set_failure_from_reason_if_empty(");
    assert!(
        helper_body.contains("outcome.failure.is_none()")
            && helper_body.contains("outcome.failure = Some(")
            && helper_body.contains("RelationFailure::from_solver_reason("),
        "set_failure_from_reason_if_empty must guard on outcome.failure.is_none() \
         and wrap the solver reason into a checker-facing RelationFailure"
    );

    let exec_body = extract_method_body(&source, "fn execute_relation_request(");
    assert!(
        exec_body.contains("apply_checker_side_downgrade("),
        "execute_relation_request must route checker-side downgrades through \
         apply_checker_side_downgrade so the failure reason is preserved"
    );

    // assign_relation_outcome's failed branch must compute the raw-input
    // failure reason BEFORE the boundary's evaluated-shape pass and OVERRIDE
    // any boundary reason it produced. This matches the early-return ordering
    // of `analyze_assignability_failure`: when a raw-input detector fires the
    // raw reason wins, because the boundary's evaluated pipeline cannot
    // reconstruct those shapes (e.g. same-generic `C<A..>` vs `C<B..>` evaluates
    // to incompatible object property values, producing a wrapper reason that
    // masks tsc's direct type-argument elaboration).
    let assign_body = extract_method_body(&source, "fn assign_relation_outcome(");
    assert!(
        assign_body.contains("raw_input_failure_reason("),
        "assign_relation_outcome must compute the raw-input failure reason \
         via raw_input_failure_reason"
    );
    let raw_reason_idx = assign_body
        .find("raw_input_failure_reason(")
        .expect("raw_input_failure_reason call site not found");
    let execute_idx = assign_body
        .find("execute_relation_request(")
        .expect("execute_relation_request call site not found");
    assert!(
        raw_reason_idx < execute_idx,
        "raw_input_failure_reason must be computed BEFORE execute_relation_request \
         so the raw reason can override any wrapper reason the boundary's \
         evaluated-shape pass produces"
    );
    let override_assigns_failure_unconditionally = assign_body
        .contains("outcome.failure = Some(")
        && assign_body.contains("RelationFailure::from_solver_reason(");
    let override_uses_if_let_some =
        assign_body.contains("if let Some(reason) = raw_reason");
    assert!(
        override_assigns_failure_unconditionally && override_uses_if_let_some,
        "assign_relation_outcome must overwrite outcome.failure when the raw \
         reason fires, not gate on outcome.failure.is_none() — otherwise the \
         boundary's wrapper reason would shadow the raw-input elaboration"
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
