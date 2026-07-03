use std::fs;

#[test]
fn flow_analyzer_shared_caches_are_wired_as_one_bundle() {
    // The shared-cache bundle is all-or-nothing (#13060): `FlowAnalyzer`
    // exposes one `with_shared_caches` entry point backed by the
    // context-owned `FlowSharedCaches`, and production sites construct
    // through `from_ctx`, so a launch site cannot wire a partial cache
    // subset.
    let core = fs::read_to_string("src/flow/control_flow/core.rs")
        .expect("failed to read src/flow/control_flow/core.rs for architecture guard");
    assert!(
        core.contains("pub fn from_ctx("),
        "FlowAnalyzer must expose the single `from_ctx` production construction path"
    );
    assert!(
        core.contains("fn with_shared_caches("),
        "FlowAnalyzer must wire shared caches through the one-bundle builder"
    );
    for forbidden in [
        "fn with_flow_cache(",
        "fn with_narrowing_cache(",
        "fn with_flow_buffers(",
        "fn with_reference_match_cache(",
        "fn with_switch_reference_cache(",
        "fn with_numeric_atom_cache(",
        "fn with_call_type_predicates(",
        "fn with_flow_reference_keys(",
        "fn with_alias_base_assignment_cache(",
        "fn with_symbol_last_assignment_pos(",
        "fn with_symbol_nested_closure_assignment(",
        "fn with_symbol_first_identifier_ref(",
    ] {
        assert!(
            !core.contains(forbidden),
            "per-cache builder `{forbidden}` reintroduces partial shared-cache wiring; \
             wire caches through with_shared_caches/from_ctx instead"
        );
    }

    // Every production construction site receives the full context bundle.
    for site in [
        "src/types/computation/identifier_flow.rs",
        "src/types/property_access_helpers/access_semantics.rs",
        "src/flow/flow_analysis/usage.rs",
        "src/flow/flow_analysis/definite.rs",
        "src/types/type_node_advanced.rs",
    ] {
        let src = fs::read_to_string(site)
            .unwrap_or_else(|_| panic!("failed to read {site} for architecture guard"));
        assert!(
            src.contains("FlowAnalyzer::from_ctx("),
            "{site} must construct FlowAnalyzer through from_ctx so the \
             shared-cache bundle is wired whole"
        );
        assert!(
            !src.contains("FlowAnalyzer::with_node_types(") && !src.contains("FlowAnalyzer::new("),
            "{site} must not hand-assemble FlowAnalyzer construction outside from_ctx"
        );
    }
}

#[test]
fn control_flow_contains_type_parameter_checks_use_flow_query_boundary() {
    let src = fs::read_to_string("src/flow/control_flow/core.rs")
        .expect("failed to read src/flow/control_flow/core.rs for architecture guard");

    assert!(
        src.contains("query::contains_type_parameters("),
        "control_flow type-parameter cacheability checks should route through query_boundaries::flow_analysis"
    );
    assert!(
        !src.contains("tsz_solver::type_queries::contains_type_parameters_db("),
        "control_flow should not call solver type_queries::contains_type_parameters_db directly"
    );
}

#[test]
fn control_flow_assignability_helpers_use_flow_query_boundary() {
    // The original positive probes here (`query::is_assignable_with_env(` /
    // `query::is_assignable_strict_null(` in core.rs) went stale while this
    // file was not registered as a `[[test]]` target: those call sites were
    // refactored out of core.rs entirely. The durable invariant is the
    // negative one — no control_flow source may call solver assignability
    // directly instead of going through `query_boundaries::flow_analysis`.
    fn scan(dir: &std::path::Path, hits: &mut Vec<String>) {
        for entry in fs::read_dir(dir).expect("failed to read control_flow dir") {
            let path = entry.expect("failed to read dir entry").path();
            if path.is_dir() {
                scan(&path, hits);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                let src = fs::read_to_string(&path).expect("failed to read control_flow source");
                if src.contains("tsz_solver::type_queries::is_assignable")
                    || src.contains("tsz_solver::is_assignable")
                {
                    hits.push(path.display().to_string());
                }
            }
        }
    }

    let mut hits = Vec::new();
    scan(std::path::Path::new("src/flow/control_flow"), &mut hits);
    assert!(
        hits.is_empty(),
        "control_flow must route assignability through query_boundaries::flow_analysis; \
         direct solver assignability calls found in: {hits:?}"
    );
}

#[test]
fn assignment_reduction_uses_flow_query_boundary() {
    let src = fs::read_to_string("src/flow/control_flow/assignment.rs")
        .expect("failed to read src/flow/control_flow/assignment.rs for architecture guard");

    assert!(
        src.contains("query_boundaries::flow_analysis::narrow_assignment("),
        "FlowAnalyzer assignment reduction should delegate type algebra to query_boundaries::flow_analysis"
    );
    assert!(
        !src.contains("fn resolve_assignment_reduction_type("),
        "assignment reduction type resolution belongs in query_boundaries::flow_analysis"
    );
    assert!(
        !src.contains("fn assignment_source_assignable_to_member("),
        "assignment member filtering belongs in query_boundaries::flow_analysis"
    );
}

#[test]
fn assignment_fallback_shape_construction_uses_flow_query_boundary() {
    let src = fs::read_to_string("src/flow/control_flow/assignment_fallback.rs")
        .expect("failed to read src/flow/control_flow/assignment_fallback.rs");

    for forbidden in [
        ".factory().object(",
        ".factory().callable(",
        ".factory.object(",
        ".factory.callable(",
        "CallableShape {",
        "CallableShape::default()",
    ] {
        assert!(
            !src.contains(forbidden),
            "assignment fallback must route solver shape construction through \
             query_boundaries::flow_analysis, found `{forbidden}`"
        );
    }

    assert!(
        src.contains("flow_analysis::object_type_from_properties(")
            && src.contains("flow_analysis::call_only_callable_type("),
        "assignment fallback object/callable construction should route through flow_analysis"
    );

    let boundary = fs::read_to_string("src/query_boundaries/flow_analysis.rs")
        .expect("failed to read src/query_boundaries/flow_analysis.rs");
    assert!(
        boundary.contains("fn object_type_from_properties(")
            && boundary.contains("fn call_only_callable_type("),
        "flow_analysis must own assignment-fallback object/callable construction helpers"
    );
}
