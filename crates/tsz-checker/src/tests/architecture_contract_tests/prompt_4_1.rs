use super::*;
fn test_pack_relation_flags_tracks_checker_strict_options() {
    let arena = NodeArena::new();
    let binder = BinderState::new();
    let types = TypeInterner::new();

    let options = CheckerOptions {
        strict: false,
        strict_null_checks: true,
        strict_function_types: false,
        exact_optional_property_types: true,
        no_unchecked_indexed_access: true,
        ..Default::default()
    };

    let ctx = CheckerContext::new(&arena, &binder, &types, "test.ts".to_string(), options);

    let expected = RelationCacheKey::FLAG_STRICT_NULL_CHECKS
        | RelationCacheKey::FLAG_EXACT_OPTIONAL_PROPERTY_TYPES
        | RelationCacheKey::FLAG_NO_UNCHECKED_INDEXED_ACCESS;

    assert_eq!(ctx.pack_relation_flags(), expected);
}

#[test]
fn test_configure_compat_checker_honors_strict_option_semantics() {
    let arena = NodeArena::new();
    let binder = BinderState::new();
    let types = TypeInterner::new();
    let (animal, dog) = make_animal_and_dog(&types);

    let dog_to_animal_fn = types.function(FunctionShape {
        params: vec![ParamInfo::unnamed(dog)],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let animal_fn = types.function(FunctionShape {
        params: vec![ParamInfo::unnamed(animal)],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let non_strict_options = CheckerOptions {
        strict: false,
        strict_null_checks: false,
        strict_function_types: false,
        ..Default::default()
    };
    let ctx = CheckerContext::new(
        &arena,
        &binder,
        &types,
        "test.ts".to_string(),
        non_strict_options,
    );
    let mut compat = CompatChecker::new(&types);
    ctx.configure_compat_checker(&mut compat);

    assert!(compat.is_assignable(TypeId::NULL, TypeId::NUMBER));
    assert!(compat.is_assignable(dog_to_animal_fn, animal_fn));

    let strict_options = CheckerOptions {
        strict: false,
        strict_null_checks: true,
        strict_function_types: true,
        ..Default::default()
    };
    let ctx = CheckerContext::new(
        &arena,
        &binder,
        &types,
        "test.ts".to_string(),
        strict_options,
    );
    let mut compat = CompatChecker::new(&types);
    ctx.configure_compat_checker(&mut compat);

    assert!(!compat.is_assignable(TypeId::NULL, TypeId::NUMBER));
    assert!(!compat.is_assignable(dog_to_animal_fn, animal_fn));
}

#[test]
fn test_no_implicit_any_scope_inference_for_js_files() {
    let arena = NodeArena::new();
    let binder = BinderState::new();
    let types = TypeInterner::new();

    let js_with_no_check_js = CheckerContext::new(
        &arena,
        &binder,
        &types,
        "test.js".to_string(),
        CheckerOptions {
            no_implicit_any: true,
            check_js: false,
            ..Default::default()
        },
    );
    assert!(!js_with_no_check_js.no_implicit_any());

    let js_with_check_js = CheckerContext::new(
        &arena,
        &binder,
        &types,
        "test.js".to_string(),
        CheckerOptions {
            no_implicit_any: true,
            check_js: true,
            ..Default::default()
        },
    );
    assert!(js_with_check_js.no_implicit_any());

    let ts_file = CheckerContext::new(
        &arena,
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions {
            no_implicit_any: true,
            check_js: false,
            ..Default::default()
        },
    );
    assert!(ts_file.no_implicit_any());
}

#[test]
fn def_id_fallback_prefers_symbol_stable_declaration_span() {
    let source = "interface Foo { value: number }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let ctx = CheckerContext::new_with_shared_def_store(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
        Arc::new(DefinitionStore::new()),
    );

    let sym_id = binder.file_locals.get("Foo").expect("expected Foo");
    let symbol = binder.symbols.get(sym_id).expect("expected symbol for Foo");
    let def_id = ctx.get_or_create_def_id(sym_id);
    let info = ctx
        .definition_store
        .get(def_id)
        .expect("fallback should register DefinitionInfo");

    assert_eq!(info.span, symbol.first_declaration_span);
    assert_eq!(ctx.def_fallback_count.get(), 1);
}

#[test]
fn test_array_helpers_avoid_direct_typekey_interning() {
    let mut checker_rs_files = Vec::new();
    collect_checker_rs_files_recursive(Path::new("src"), &mut checker_rs_files);

    let mut array_type_violations = Vec::new();
    for path in checker_rs_files {
        if path
            .components()
            .any(|component| component.as_os_str() == "tests")
        {
            continue;
        }
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("failed to read {}", path.display()));
        if source.contains("TypeData::Array") {
            array_type_violations.push(path.display().to_string());
        }
    }
    assert!(
        array_type_violations.is_empty(),
        "checker helpers should use solver array constructor APIs, not TypeData::Array; violations: {}",
        array_type_violations.join(", ")
    );

    let type_literal_src = fs::read_to_string("src/types/type_literal_checker.rs")
        .expect("failed to read src/types/type_literal_checker.rs for architecture guard");
    assert!(
        !type_literal_src.contains("TypeData::ReadonlyType"),
        "type_literal_checker should use solver readonly constructor APIs, not TypeData::ReadonlyType"
    );

    let mut type_resolution_src = fs::read_to_string("src/state/type_resolution/core.rs")
        .expect("failed to read src/state/type_resolution/core.rs for architecture guard");
    // Include split-off modules that are part of the type_resolution logical module
    type_resolution_src.push_str(
        &fs::read_to_string("src/state/type_resolution/module.rs")
            .expect("failed to read src/state/type_resolution/module.rs"),
    );
    assert!(
        !type_resolution_src.contains("TypeData::ReadonlyType"),
        "state_type_resolution should use solver readonly constructor APIs, not TypeData::ReadonlyType"
    );
    assert!(
        !type_resolution_src.contains("intern(tsz_solver::TypeData::Lazy("),
        "state_type_resolution should use solver lazy constructor API, not direct TypeData::Lazy interning"
    );

    let type_node_src = fs::read_to_string("src/types/type_node.rs")
        .expect("failed to read src/types/type_node.rs");
    assert!(
        !type_node_src.contains("TypeData::ReadonlyType"),
        "type_node should use solver readonly constructor API, not TypeData::ReadonlyType"
    );
    assert!(
        !type_node_src.contains("TypeData::KeyOf"),
        "type_node should use solver keyof constructor API, not TypeData::KeyOf"
    );
    assert!(
        !type_node_src.contains("TypeData::IndexAccess"),
        "type_node should use solver index_access constructor API, not TypeData::IndexAccess"
    );

    // Read all JSX module files and concatenate for architecture checks.
    let jsx_checker_src = {
        let mut buf = String::new();
        for file in &[
            "src/checkers/jsx/orchestration",
            "src/checkers/jsx/children.rs",
            "src/checkers/jsx/props/mod.rs",
            "src/checkers/jsx/props/resolution.rs",
            "src/checkers/jsx/props/validation.rs",
            "src/checkers/jsx/runtime.rs",
            "src/checkers/jsx/diagnostics.rs",
        ] {
            buf.push_str(&read_checker_source_file(file));
        }
        buf
    };
    assert!(
        !jsx_checker_src.contains("TypeData::IndexAccess"),
        "jsx module should use solver index_access constructor API, not TypeData::IndexAccess"
    );

    let mut context_src = fs::read_to_string("src/context/mod.rs")
        .expect("failed to read src/context/mod.rs for architecture guard");
    // Include sub-modules that are part of the context module
    context_src.push_str(
        &fs::read_to_string("src/context/constructors.rs")
            .expect("failed to read src/context/constructors.rs for architecture guard"),
    );
    context_src.push_str(
        &fs::read_to_string("src/context/resolver.rs")
            .expect("failed to read src/context/resolver.rs for architecture guard"),
    );
    assert!(
        !context_src.contains("self.types.intern(TypeData::Lazy("),
        "context should use solver lazy constructor API, not direct TypeData::Lazy interning"
    );

    // register_resolved_type must use dual-env helpers for DefId mappings
    let def_mapping_src = fs::read_to_string("src/context/def_mapping.rs")
        .expect("failed to read src/context/def_mapping.rs for architecture guard");
    assert!(
        def_mapping_src.contains("register_def_auto_params_in_envs("),
        "register_resolved_type should use dual-env helper for DefId registration (not single-env insert_def)"
    );

    // type_node_resolution must use dual-env helpers for DefId registration
    let type_node_resolution_src = fs::read_to_string("src/types/type_node_resolution.rs")
        .expect("failed to read src/types/type_node_resolution.rs for architecture guard");
    assert!(
        type_node_resolution_src.contains("register_def_in_envs("),
        "type_node_resolution should use dual-env helpers for DefId registration"
    );

    // symbol_types must use dual-env helpers for interface structural type registration
    let symbol_types_src = fs::read_to_string("src/state/type_resolution/symbol_types.rs")
        .expect("failed to read src/state/type_resolution/symbol_types.rs for architecture guard");
    assert!(
        symbol_types_src.contains("register_def_in_envs("),
        "symbol_types should use dual-env helpers for interface DefId registration"
    );

    // global type registration must mirror into type_environment
    let global_src = fs::read_to_string("src/types/type_checking/global.rs")
        .expect("failed to read src/types/type_checking/global.rs for architecture guard");
    assert!(
        global_src.contains("type_environment.try_borrow_mut()"),
        "global type registration must mirror boxed DefId mappings into type_environment"
    );

    let queries_src = fs::read_to_string("src/types/queries/core.rs")
        .expect("failed to read src/types/queries/core.rs for architecture guard");
    assert!(
        !queries_src.contains("self.ctx.types.intern(TypeData::Lazy("),
        "queries/core should use solver lazy constructor API, not direct TypeData::Lazy interning"
    );
    assert!(
        !queries_src.contains("self.ctx.types.intern(TypeData::TypeParameter("),
        "queries/core should use solver type_param constructor API, not direct TypeData::TypeParameter interning"
    );

    let state_checking_members_src = fs::read_to_string("src/state/state_checking_members/mod.rs")
        .expect("failed to read src/state/state_checking_members/mod.rs for architecture guard");
    assert!(
        !state_checking_members_src.contains("TypeData::TypeParameter"),
        "state_checking_members should use solver type_param constructor API, not TypeData::TypeParameter"
    );

    let control_flow_narrowing_src = fs::read_to_string("src/flow/control_flow/narrowing.rs")
        .expect("failed to read src/flow/control_flow/narrowing.rs for architecture guard");
    assert!(
        !control_flow_narrowing_src.contains("intern(TypeData::Lazy("),
        "control_flow_narrowing should use solver lazy constructor API, not direct TypeData::Lazy interning"
    );

    let mut state_type_analysis_src = fs::read_to_string("src/state/type_analysis/mod.rs")
        .expect("failed to read src/state/type_analysis/mod.rs for architecture guard");
    // Include split-off modules that are part of the state_type_analysis logical module
    state_type_analysis_src.push_str(
        &fs::read_to_string("src/state/type_analysis/computed/mod.rs").expect(
            "failed to read src/state/type_analysis/computed/mod.rs for architecture guard",
        ),
    );
    state_type_analysis_src.push_str(
        &fs::read_to_string("src/state/type_analysis/computed_helpers.rs").expect(
            "failed to read src/state/type_analysis/computed_helpers.rs for architecture guard",
        ),
    );
    assert!(
        !state_type_analysis_src.contains("intern(TypeData::TypeQuery("),
        "state_type_analysis should use solver type_query constructor API, not TypeData::TypeQuery"
    );
    assert!(
        !state_type_analysis_src.contains("intern(TypeData::TypeParameter("),
        "state_type_analysis should use solver type_param constructor API, not TypeData::TypeParameter"
    );
    assert!(
        !state_type_analysis_src.contains("intern(tsz_solver::TypeData::Lazy("),
        "state_type_analysis should use solver lazy constructor API, not direct TypeData::Lazy interning"
    );
    assert!(
        !state_type_analysis_src.contains("intern(TypeData::Enum("),
        "state_type_analysis should use solver enum_type constructor API, not TypeData::Enum interning"
    );
    assert!(
        state_type_analysis_src.contains("ensure_relation_input_ready("),
        "state_type_analysis contextual-literal precondition setup should route through ensure_relation_input_ready"
    );

    let function_type_src = fs::read_to_string("src/types/function_type.rs")
        .expect("failed to read src/types/function_type.rs for architecture guard");
    assert!(
        !function_type_src.contains("intern(TypeData::TypeParameter("),
        "function_type should use solver type_param constructor API, not TypeData::TypeParameter"
    );

    let assignability_checker_src = fs::read_to_string(
        "src/assignability/assignability_checker.rs",
    )
    .expect("failed to read src/assignability/assignability_checker.rs for architecture guard");
    assert!(
        !assignability_checker_src.contains("TypeTraversalKind::"),
        "assignability_checker should not implement solver type-graph traversal branches directly"
    );
    assert!(
        !assignability_checker_src.contains("classify_for_traversal("),
        "assignability_checker should use solver visitor helpers instead of traversal classification"
    );
    assert!(
        assignability_checker_src.contains("resolve_and_insert_def_type("),
        "assignability_checker should use centralized DefId resolution helper for type_env preconditions"
    );
    assert!(
        !assignability_checker_src.contains("env.insert_def("),
        "assignability_checker should not insert DefId mappings directly; use centralized helper"
    );
    assert!(
        !assignability_checker_src.contains("contains_infer_types_cached("),
        "assignability_checker should not use checker-local infer-shape cacheability wrappers"
    );
    assert!(
        !assignability_checker_src.contains("visitor::contains_infer_types("),
        "assignability_checker infer-shape cacheability checks should route through query_boundaries::assignability"
    );
    assert!(
        assignability_checker_src.contains("is_relation_cacheable("),
        "assignability_checker should use query_boundaries::assignability::is_relation_cacheable for relation-cache gating"
    );

    let mut state_type_environment_src = fs::read_to_string("src/state/type_environment/mod.rs")
        .expect("failed to read src/state/type_environment/mod.rs for architecture guard");
    // Include split-off module that is part of the state_type_environment logical module
    state_type_environment_src.push_str(
        &fs::read_to_string("src/state/type_environment/lazy.rs")
            .expect("failed to read src/state/type_environment/lazy.rs for architecture guard"),
    );
    // Include core.rs which has application/mapped type evaluation
    let state_type_environment_core_src = fs::read_to_string("src/state/type_environment/core.rs")
        .expect("failed to read src/state/type_environment/core.rs for architecture guard");
    state_type_environment_src.push_str(&state_type_environment_core_src);
    assert!(
        !state_type_environment_core_src.contains("TypeData::TypeParameter("),
        "state_type_environment/core.rs should use solver query (type_param_name / get_type_parameter_info) instead of direct TypeData::TypeParameter pattern matching"
    );
    assert!(
        !state_type_environment_src.contains("intern(TypeData::Enum("),
        "state_type_environment should use solver enum_type constructor API, not TypeData::Enum"
    );
    assert!(
        !state_type_environment_src.contains("intern(TypeData::Literal("),
        "state_type_environment should use solver literal constructors, not TypeData::Literal"
    );
    assert!(
        state_type_environment_src.contains("collect_referenced_types("),
        "state_type_environment should use solver collect_referenced_types visitor helper for traversal preconditions"
    );
    assert!(
        state_type_environment_src.contains("collect_lazy_def_ids("),
        "state_type_environment should use solver collect_lazy_def_ids visitor helper for lazy DefId preconditions"
    );
    assert!(
        state_type_environment_src.contains("collect_enum_def_ids("),
        "state_type_environment should use solver collect_enum_def_ids visitor helper for enum DefId preconditions"
    );
    assert!(
        state_type_environment_src.contains("ensure_relation_input_ready("),
        "state_type_environment relation precondition setup should route through ensure_relation_input_ready"
    );
    assert!(
        state_type_environment_src.contains("collect_type_queries("),
        "state_type_environment should use solver collect_type_queries visitor helper for type-query symbol preconditions"
    );
    assert!(
        state_type_environment_src.contains("resolve_lazy_def_for_type_env("),
        "state_type_environment should centralize lazy DefId precondition resolution in a dedicated helper"
    );
    assert!(
        state_type_environment_src.contains("resolve_enum_def_for_type_env("),
        "state_type_environment should centralize enum DefId precondition resolution in a dedicated helper"
    );

    let type_computation_complex_src = fs::read_to_string("src/types/computation/complex.rs")
        .expect("failed to read src/types/computation/complex.rs for architecture guard");
    assert!(
        !type_computation_complex_src.contains("intern(tsz_solver::TypeData::TypeParameter("),
        "computation/complex should use solver type_param constructor API, not direct TypeData::TypeParameter interning"
    );

    let diagnostics_boundary_src = fs::read_to_string("src/query_boundaries/diagnostics.rs")
        .expect("failed to read src/query_boundaries/diagnostics.rs for architecture guard");
    assert!(
        !diagnostics_boundary_src.contains("TypeTraversalKind::"),
        "query_boundaries/diagnostics should not branch on TypeTraversalKind directly"
    );
    assert!(
        !diagnostics_boundary_src.contains("classify_for_traversal("),
        "query_boundaries/diagnostics should use solver classify_property_traversal API"
    );
    assert!(
        diagnostics_boundary_src.contains("collect_property_name_atoms_for_diagnostics("),
        "query_boundaries/diagnostics should expose solver property-name collector API"
    );

    // error_reporter is now a directory module with submodules
    // Check suggestions.rs submodule where collect_type_property_names is located
    let error_reporter_suggestions_src = fs::read_to_string("src/error_reporter/suggestions.rs")
        .expect("failed to read src/error_reporter/suggestions.rs for architecture guard");
    assert!(
        error_reporter_suggestions_src.contains("collect_property_name_atoms_for_diagnostics(")
            || error_reporter_suggestions_src
                .contains("collect_accessible_property_names_for_suggestion("),
        "error_reporter should use query-boundary solver property-name collection helper"
    );
    assert!(
        !error_reporter_suggestions_src.contains("fn collect_type_property_names_inner("),
        "error_reporter should not own recursive property traversal helpers"
    );
}

#[test]
fn test_assignability_checker_routes_relation_queries_through_query_boundaries() {
    let assignability_source = fs::read_to_string("src/assignability/assignability_checker.rs")
        .expect("failed to read src/assignability/assignability_checker.rs for architecture guard");
    let subtype_source = fs::read_to_string("src/assignability/subtype_identity_checker.rs")
        .expect(
            "failed to read src/assignability/subtype_identity_checker.rs for architecture guard",
        );

    // Neither file should use the raw query_relation helpers
    for (name, source) in [
        ("assignability_checker", assignability_source.as_str()),
        ("subtype_identity_checker", subtype_source.as_str()),
    ] {
        assert!(
            !source.contains("query_relation_with_overrides("),
            "{name} should route compatibility checks through query_boundaries/assignability helpers"
        );
        assert!(
            !source.contains("query_relation_with_resolver("),
            "{name} should route subtype/redecl checks through query_boundaries/assignability helpers"
        );
    }

    // Assignability helpers live in assignability_checker
    assert!(
        assignability_source.contains("is_assignable_with_overrides("),
        "assignability_checker should use query_boundaries::assignability::is_assignable_with_overrides"
    );
    assert!(
        assignability_source.contains("is_assignable_bivariant_with_resolver("),
        "assignability_checker should use query_boundaries::assignability::is_assignable_bivariant_with_resolver"
    );

    // Subtype/redecl/union helpers live in subtype_identity_checker
    assert!(
        subtype_source.contains("is_subtype_with_resolver("),
        "subtype_identity_checker should use query_boundaries::assignability::is_subtype_with_resolver"
    );
    assert!(
        subtype_source.contains("is_redeclaration_identical_with_resolver("),
        "subtype_identity_checker should use query_boundaries::assignability::is_redeclaration_identical_with_resolver"
    );
}

#[test]
fn test_subtype_path_establishes_preconditions_before_subtype_cache_lookup() {
    let source = fs::read_to_string("src/assignability/subtype_identity_checker.rs").expect(
        "failed to read src/assignability/subtype_identity_checker.rs for architecture guard",
    );

    let subtype_start = source
        .find("pub fn is_subtype_of(")
        .expect("missing is_subtype_of in subtype_identity_checker");
    // Extract just the is_subtype_of method body (up to the next pub fn or end of impl)
    let subtype_end = source[subtype_start + 1..]
        .find("pub")
        .map(|offset| subtype_start + 1 + offset)
        .unwrap_or(source.len());
    let subtype_src = &source[subtype_start..subtype_end];

    let ensure_apps_pos = subtype_src
        .find("self.ensure_relation_input_ready(source);")
        .expect("is_subtype_of should establish centralized relation preconditions before checks");
    let lookup_pos = subtype_src
        .find("lookup_subtype_cache(")
        .expect("is_subtype_of should consult solver subtype cache");
    assert!(
        ensure_apps_pos < lookup_pos,
        "is_subtype_of must establish ref/application preconditions before subtype cache lookup"
    );
}

#[test]
fn test_subtype_identity_checker_no_direct_solver_inspection() {
    let source = fs::read_to_string("src/assignability/subtype_identity_checker.rs").expect(
        "failed to read src/assignability/subtype_identity_checker.rs for architecture guard",
    );
    // Must not use raw `.lookup()` — route through query_boundaries wrappers
    assert!(
        !source.contains(".lookup("),
        "subtype_identity_checker must not use raw .lookup(); use query_boundaries helpers instead"
    );
    // Must not match on TypeData variants directly
    assert!(
        !source.contains("TypeData::"),
        "subtype_identity_checker must not inspect TypeData variants; use query_boundaries helpers instead"
    );
}

#[test]
fn test_assignment_and_binding_default_assignability_use_central_gateway_helpers() {
    let assignment_checker_src = {
        let mut s = String::new();
        let dir = Path::new("src/assignability/assignment_checker");
        for entry in fs::read_dir(dir).expect("failed to read assignment_checker directory") {
            let entry = entry.expect("failed to read directory entry");
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("rs")
                && let Ok(content) = fs::read_to_string(&path)
            {
                s.push_str(&content);
            }
        }
        assert!(
            !s.is_empty(),
            "failed to read any .rs files from assignment_checker directory"
        );
        s
    };
    assert!(
        assignment_checker_src.contains("check_assignable_or_report_at("),
        "assignment compatibility should route through check_assignable_or_report_at for centralized mismatch policy"
    );
    assert!(
        assignment_checker_src.contains("ensure_relation_input_ready("),
        "assignment checker relation precondition setup should route through ensure_relation_input_ready"
    );
    assert!(
        !assignment_checker_src.contains("self.ctx.types.is_assignable_to("),
        "assignment checker should route assignability through checker/solver gateway helpers, not direct interner checks"
    );
    assert!(
        !assignment_checker_src.contains("self.ctx.types.is_subtype_of("),
        "assignment checker subtype checks should route through checker/solver gateway helpers, not direct interner checks"
    );
    assert!(
        !assignment_checker_src.contains("ensure_application_symbols_resolved("),
        "assignment checker should not manually orchestrate application-symbol preconditions"
    );

    let type_checking_src = {
        let mut s = fs::read_to_string("src/types/type_checking/core.rs")
            .expect("failed to read src/types/type_checking/core.rs");
        if let Ok(stmts) = fs::read_to_string("src/types/type_checking/core_statement_checks.rs") {
            s.push_str(&stmts);
        }
        s
    };
    assert!(
        type_checking_src.contains("check_assignable_or_report("),
        "binding/default-value assignability should route through check_assignable_or_report"
    );
    assert!(
        type_checking_src.contains("ensure_relation_input_ready("),
        "type_checking return/binding relation precondition setup should route through ensure_relation_input_ready"
    );
    assert!(
        !type_checking_src.contains("ensure_application_symbols_resolved("),
        "type_checking should not manually orchestrate application-symbol preconditions"
    );

    let parameter_checker_src = fs::read_to_string("src/checkers/parameter_checker.rs")
        .expect("failed to read src/checkers/parameter_checker.rs for architecture guard");
    assert!(
        parameter_checker_src.contains("check_assignable_or_report("),
        "parameter initializer assignability should route through check_assignable_or_report"
    );

    let control_flow_assignment_src = fs::read_to_string("src/flow/control_flow/assignment.rs")
        .expect("failed to read src/flow/control_flow/assignment.rs for architecture guard");
    assert!(
        control_flow_assignment_src.contains("is_assignable_to_strict_null("),
        "control-flow assignment nullish compatibility checks should route through checker assignability gateway helpers"
    );
    assert!(
        !control_flow_assignment_src.contains("self.interner.is_assignable_to("),
        "control-flow assignment should not call interner assignability directly"
    );
    assert!(
        !control_flow_assignment_src.contains(".is_assignable_to_with_flags("),
        "control-flow assignment should not use interner relation flags directly"
    );
    assert!(
        !control_flow_assignment_src.contains("tsz_solver::is_subtype_of("),
        "control-flow assignment subtype checks should route through query boundaries, not direct solver helpers"
    );
    assert!(
        control_flow_assignment_src.contains("is_assignable(")
            || control_flow_assignment_src.contains("is_assignable_with_env("),
        "control-flow assignment compatibility checks should route through flow_analysis boundary helpers (is_assignable or is_assignable_with_env)"
    );
    assert!(
        control_flow_assignment_src.contains("widen_literal_to_primitive("),
        "control-flow assignment literal widening should route through flow_analysis boundary helpers"
    );
    assert!(
        control_flow_assignment_src.contains("get_array_element_type("),
        "control-flow assignment for-of element extraction should route through flow_analysis boundary helpers"
    );
    assert!(
        !control_flow_assignment_src.contains("tsz_solver::type_queries::"),
        "control-flow assignment should not call solver type_queries directly; use flow_analysis boundary helpers"
    );
    let control_flow_src = fs::read_to_string("src/flow/control_flow/core.rs")
        .expect("failed to read src/flow/control_flow/core.rs for architecture guard");
    assert!(
        control_flow_src.contains("query::is_assignable_with_env("),
        "FlowAnalyzer assignability should route through flow_analysis boundary helpers"
    );
    assert!(
        control_flow_src.contains("query::is_assignable_strict_null("),
        "FlowAnalyzer strict-null assignability should route through flow_analysis boundary helpers"
    );
    let flow_analysis_definite_src = fs::read_to_string("src/flow/flow_analysis/definite.rs")
        .expect("failed to read src/flow/flow_analysis/definite.rs for architecture guard");
    assert!(
        flow_analysis_definite_src.contains("find_property_in_object_by_str("),
        "flow_analysis_definite property lookup should route through definite_assignment query boundaries"
    );
    assert!(
        !flow_analysis_definite_src.contains("tsz_solver::type_queries::"),
        "flow_analysis_definite should not call solver type_queries directly; use definite_assignment/flow_analysis query boundaries"
    );

    let mut state_type_resolution_src = fs::read_to_string("src/state/type_resolution/core.rs")
        .expect("failed to read src/state/type_resolution/core.rs for architecture guard");
    // Include split-off modules that are part of the type_resolution logical module
    state_type_resolution_src.push_str(
        &fs::read_to_string("src/state/type_resolution/module.rs")
            .expect("failed to read src/state/type_resolution/module.rs"),
    );
    state_type_resolution_src.push_str(
        &fs::read_to_string("src/state/type_resolution/constructors.rs")
            .expect("failed to read src/state/type_resolution/constructors.rs"),
    );
    assert!(
        state_type_resolution_src.contains("ensure_relation_input_ready("),
        "state_type_resolution relation precondition setup should route through ensure_relation_input_ready"
    );

    let mut state_checking_src = fs::read_to_string("src/state/state_checking/mod.rs")
        .expect("failed to read src/state/state_checking/mod.rs for architecture guard");
    // Include split-off modules that are part of the state_checking logical module
    state_checking_src.push_str(
        &fs::read_to_string("src/state/variable_checking/core.rs")
            .expect("failed to read src/state/variable_checking/core.rs for architecture guard"),
    );
    state_checking_src.push_str(
        &fs::read_to_string("src/state/state_checking/property.rs")
            .expect("failed to read src/state/state_checking/property.rs for architecture guard"),
    );
    state_checking_src.push_str(
        &fs::read_to_string("src/state/variable_checking/destructuring.rs").expect(
            "failed to read src/state/variable_checking/destructuring.rs for architecture guard",
        ),
    );
    state_checking_src.push_str(
        &fs::read_to_string("src/state/state_checking/class.rs")
            .expect("failed to read src/state/state_checking/class.rs for architecture guard"),
    );
    assert!(
        state_checking_src.contains("check_assignable_or_report(")
            || state_checking_src.contains("check_assignable_or_report_at("),
        "state_checking assignment-style checks should route through centralized assignability gateways"
    );
    assert!(
        state_checking_src.contains("check_assignable_or_report_generic_at("),
        "state_checking destructuring generic mismatch checks should route through check_assignable_or_report_generic_at"
    );
    assert!(
        state_checking_src.contains("ensure_relation_input_ready("),
        "state_checking relation/query precondition setup should route through ensure_relation_input_ready"
    );
    assert!(
        !state_checking_src.contains("ensure_application_symbols_resolved("),
        "state_checking should not manually orchestrate application-symbol preconditions"
    );
    let state_property_checking_src = fs::read_to_string("src/state/state_checking/property.rs")
        .expect("failed to read src/state/state_checking/property.rs for architecture guard");
    assert!(
        !state_property_checking_src.contains("self.ctx.types.is_subtype_of("),
        "state_property_checking subtype checks should route through checker gateway helpers, not direct interner calls"
    );
    assert!(
        !state_property_checking_src.contains("tsz_solver::type_queries::"),
        "state_property_checking should route solver type-query access through query_boundaries::state::checking"
    );
    let state_variable_checking_destructuring_src = fs::read_to_string(
        "src/state/variable_checking/destructuring.rs",
    )
    .expect("failed to read src/state/variable_checking/destructuring.rs for architecture guard");
    let state_variable_checking_src = fs::read_to_string("src/state/variable_checking/core.rs")
        .expect("failed to read src/state/variable_checking/core.rs for architecture guard");
    let state_class_checking_src = fs::read_to_string("src/state/state_checking/class.rs")
        .expect("failed to read src/state/state_checking/class.rs for architecture guard");
    let state_heritage_checking_src = fs::read_to_string("src/state/state_checking/heritage.rs")
        .expect("failed to read src/state/state_checking/heritage.rs for architecture guard");
    let property_access_type_src = {
        let helpers = fs::read_to_string("src/types/property_access_type/helpers.rs").expect(
            "failed to read src/types/property_access_type/helpers.rs for architecture guard",
        );
        let resolve = fs::read_to_string("src/types/property_access_type/resolve.rs").expect(
            "failed to read src/types/property_access_type/resolve.rs for architecture guard",
        );
        format!("{helpers}\n{resolve}")
    };
    let property_checker_src = fs::read_to_string("src/checkers/property_checker.rs")
        .expect("failed to read src/checkers/property_checker.rs for architecture guard");
    assert!(
        state_variable_checking_src.contains("query::array_element_type("),
        "state_variable_checking array element checks should route through query_boundaries::state::checking"
    );
    assert!(
        state_variable_checking_src.contains("flow_boundary::widen_null_undefined_to_any("),
        "state_variable_checking null/undefined widening should route through flow observation boundary"
    );
    assert!(
        state_variable_checking_src.contains("query::has_type_query_for_symbol("),
        "state_variable_checking symbol type-query checks should route through query_boundaries::state::checking"
    );
    assert!(
        !state_variable_checking_src.contains("tsz_solver::type_queries::"),
        "state_variable_checking should not call solver type_queries directly; use state_checking query boundaries"
    );
    assert!(
        state_variable_checking_destructuring_src
            .contains("flow_boundary::widen_null_undefined_to_any("),
        "state_variable_checking_destructuring null/undefined widening should route through flow observation boundary"
    );
    assert!(
        !state_variable_checking_destructuring_src.contains("tsz_solver::type_queries::"),
        "state_variable_checking_destructuring should not call solver type_queries directly; use state_checking query boundaries"
    );
    assert!(
        state_heritage_checking_src.contains("class_query::construct_signatures_for_type("),
        "state_heritage_checking constructor signature checks should route through query_boundaries::class_type"
    );
    assert!(
        state_heritage_checking_src.contains("class_query::is_generic_mapped_type("),
        "state_heritage_checking mapped-type checks should route through query_boundaries::class_type"
    );
    assert!(
        state_heritage_checking_src.contains("class_query::is_generic_type("),
        "state_heritage_checking generic-type checks should route through query_boundaries::class_type"
    );
    assert!(
        state_class_checking_src.contains("class_query::undefined_is_assignable_to("),
        "state_class_checking undefined-assignability checks should route through query_boundaries::class_type"
    );
    assert!(
        !state_class_checking_src.contains("tsz_solver::type_queries::"),
        "state_class_checking should not call solver type_queries directly; use class_type query boundaries"
    );
    assert!(
        !state_heritage_checking_src.contains("tsz_solver::type_queries::"),
        "state_heritage_checking should not call solver type_queries directly; use class_type query boundaries"
    );
    assert!(
        property_access_type_src.contains("query_boundaries::property_access::"),
        "property_access_type solver queries should route through query_boundaries::property_access"
    );
    assert!(
        !property_access_type_src.contains("tsz_solver::type_queries::"),
        "property_access_type should not call solver type_queries directly; use property_access query boundaries"
    );
    assert!(
        !property_access_type_src.contains("tsz_solver::type_queries::classifiers::"),
        "property_access_type should not call solver type_queries::classifiers directly; use property_access query boundaries"
    );
    assert!(
        property_checker_src.contains("query::is_type_usable_as_property_name("),
        "property_checker computed-name checks should route through query_boundaries::property_checker"
    );
    assert!(
        !property_checker_src.contains("tsz_solver::type_queries::"),
        "property_checker should not call solver type_queries directly; use property_checker query boundaries"
    );
    let assignability_checker_src = fs::read_to_string(
        "src/assignability/assignability_checker.rs",
    )
    .expect("failed to read src/assignability/assignability_checker.rs for architecture guard");
    assert!(
        !assignability_checker_src.contains("self.ctx.types.is_subtype_of("),
        "assignability_checker subtype checks should route through checker/solver query gateways, not direct interner calls"
    );

    let mut state_checking_members_src = fs::read_to_string(
        "src/state/state_checking_members/mod.rs",
    )
    .expect("failed to read src/state/state_checking_members/mod.rs for architecture guard");
    state_checking_members_src.push_str(
        &fs::read_to_string("src/state/state_checking_members/ambient_signature_checks.rs")
            .expect("failed to read ambient_signature_checks.rs for architecture guard"),
    );
    state_checking_members_src.push_str(
        &fs::read_to_string("src/state/state_checking_members/implicit_any_checks.rs")
            .expect("failed to read implicit_any_checks.rs for architecture guard"),
    );
    state_checking_members_src.push_str(
        &fs::read_to_string("src/state/state_checking_members/overload_compatibility.rs")
            .expect("failed to read overload_compatibility.rs for architecture guard"),
    );
    state_checking_members_src.push_str(
        &fs::read_to_string("src/state/state_checking_members/member_access.rs")
            .expect("failed to read member_access.rs for architecture guard"),
    );
    state_checking_members_src.push_str(
        &fs::read_to_string("src/state/state_checking_members/member_declaration_checks.rs")
            .expect("failed to read member_declaration_checks.rs for architecture guard"),
    );
    state_checking_members_src.push_str(
        &fs::read_to_string("src/state/state_checking_members/statement_callback_bridge.rs")
            .expect("failed to read statement_callback_bridge.rs for architecture guard"),
    );
    state_checking_members_src.push_str(
        &fs::read_to_string("src/state/state_checking_members/statement_checks.rs")
            .expect("failed to read statement_checks.rs for architecture guard"),
    );
    assert!(
        state_checking_members_src.contains("check_assignable_or_report("),
        "state_checking_members assignment-style checks should route through check_assignable_or_report"
    );

    let mut type_computation_src = fs::read_to_string("src/types/computation/helpers.rs")
        .expect("failed to read src/types/computation/helpers.rs for architecture guard");
    // Include split-off module that is part of the computation logical module
    type_computation_src.push_str(
        &fs::read_to_string("src/types/computation/binary.rs")
            .expect("failed to read src/types/computation/binary.rs for architecture guard"),
    );
    assert!(
        type_computation_src.contains("check_assignable_or_report("),
        "computation mismatch checks should route through check_assignable_or_report"
    );

    let type_computation_complex_src = fs::read_to_string("src/types/computation/complex.rs")
        .expect("failed to read src/types/computation/complex.rs for architecture guard");
    assert!(
        type_computation_complex_src.contains("check_argument_assignable_or_report("),
        "computation/complex argument mismatch checks should route through check_argument_assignable_or_report"
    );
    // TypeParameterConstraintViolation is now handled as an argument-level
    // mismatch (TS2345), matching tsc behavior. The handler uses
    // check_argument_assignable_or_report, which is already asserted above.
    assert!(
        type_computation_complex_src.contains("ensure_relation_input_ready(")
            && type_computation_complex_src.contains("ensure_relation_inputs_ready("),
        "computation/complex should route relation precondition setup through centralized ensure_relation_input(s)_ready helpers"
    );
    assert!(
        !type_computation_complex_src.contains("ensure_application_symbols_resolved("),
        "computation/complex should not manually orchestrate application-symbol preconditions; use centralized relation precondition helpers"
    );
    let type_computation_access_src = fs::read_to_string("src/types/computation/access.rs")
        .expect("failed to read src/types/computation/access.rs for architecture guard");
    assert!(
        type_computation_access_src.contains("query_boundaries::type_computation::access::"),
        "computation/access solver queries should route through query_boundaries::type_computation::access"
    );
    assert!(
        !type_computation_access_src
            .contains("tsz_solver::type_queries::get_literal_property_name("),
        "computation/access should not call get_literal_property_name directly; use type_computation_access query boundaries"
    );
    assert!(
        !type_computation_access_src.contains("tsz_solver::type_queries::get_tuple_elements("),
        "computation/access should not call get_tuple_elements directly; use type_computation_access query boundaries"
    );
    assert!(
        !type_computation_access_src.contains("tsz_solver::type_queries::is_valid_spread_type("),
        "computation/access should not call is_valid_spread_type directly; use type_computation_access query boundaries"
    );

    let dispatch_src =
        fs::read_to_string("src/dispatch.rs").expect("failed to read src/dispatch.rs for guard");
    let dispatch_yield_src = fs::read_to_string("src/dispatch_yield.rs")
        .expect("failed to read src/dispatch_yield.rs for guard");
    let dispatch_combined = format!("{dispatch_src}\n{dispatch_yield_src}");
    assert!(
        dispatch_combined.contains("check_assignable_or_report("),
        "dispatch mismatch checks should route through check_assignable_or_report"
    );
    assert!(
        dispatch_combined.contains("ensure_relation_input_ready("),
        "dispatch relation precondition setup should route through ensure_relation_input_ready"
    );
    assert!(
        !dispatch_combined.contains("ensure_application_symbols_resolved("),
        "dispatch should not manually orchestrate application-symbol preconditions"
    );

    let class_checker_src = fs::read_to_string("src/classes/class_checker.rs")
        .expect("failed to read src/classes/class_checker.rs for architecture guard");
    assert!(
        class_checker_src.contains("should_report_member_type_mismatch(")
            && class_checker_src.contains("should_report_member_type_mismatch_bivariant("),
        "class member compatibility should use centralized class query-boundary mismatch helpers"
    );

    // NOTE: error_handler.rs was removed — the ErrorHandler trait was dead
    // abstraction (20+ unused trait methods, unused DiagnosticBuilder). The only
    // used method (emit_error_at) is now an inherent method on CheckerState.
    // The TS2322 gateway contract is enforced by the assignability module guards below.

    let call_checker_src = fs::read_to_string("src/checkers/call_checker/mod.rs")
        .expect("failed to read src/checkers/call_checker/mod.rs");
    assert!(
        call_checker_src.contains("ensure_relation_input_ready(")
            && call_checker_src.contains("ensure_relation_inputs_ready("),
        "call_checker should route relation precondition setup through centralized ensure_relation_input(s)_ready helpers"
    );

    let call_boundary_src = fs::read_to_string("src/query_boundaries/checkers/call.rs")
        .expect("failed to read src/query_boundaries/checkers/call.rs");
    assert!(
        !call_boundary_src.contains("CompatChecker::with_resolver("),
        "query_boundaries/call_checker should not construct CompatChecker directly; use solver operations helper"
    );
    assert!(
        !call_boundary_src
            .contains("CallEvaluator::<tsz_solver::CompatChecker>::get_contextual_signature("),
        "query_boundaries/call_checker should not depend on concrete solver checker internals for contextual signature lookup"
    );
    assert!(
        !call_boundary_src.contains("CallEvaluator::new("),
        "query_boundaries/call_checker should not construct CallEvaluator directly; use solver operation helpers"
    );
    assert!(
        call_boundary_src.contains("compute_contextual_types_with_compat_checker("),
        "query_boundaries/call_checker contextual typing should route through solver operations helper"
    );
    assert!(
        !call_boundary_src.contains("pub(crate) fn compute_contextual_types<"),
        "query_boundaries/call_checker should not keep an unused direct CallEvaluator contextual-typing wrapper"
    );
    assert!(
        call_boundary_src.contains("get_contextual_signature_with_compat_checker("),
        "query_boundaries/call_checker contextual signature lookup should route through solver operations helper"
    );

    let assignability_boundary_src = fs::read_to_string("src/query_boundaries/assignability.rs")
        .expect("failed to read src/query_boundaries/assignability.rs");
    assert!(
        !assignability_boundary_src.contains("CompatChecker::with_resolver("),
        "query_boundaries/assignability should not construct CompatChecker directly; use solver relation-query helpers"
    );
    assert!(
        assignability_boundary_src.contains("analyze_assignability_failure_with_resolver("),
        "query_boundaries/assignability failure analysis should route through solver relation-query helpers"
    );

    let generic_checker_src = {
        let mut s = fs::read_to_string("src/checkers/generic_checker/mod.rs")
            .expect("failed to read src/checkers/generic_checker/mod.rs for architecture guard");
        s.push_str(
            &fs::read_to_string("src/checkers/generic_checker/constraint_validation.rs")
                .expect("failed to read src/checkers/generic_checker/constraint_validation.rs"),
        );
        s
    };
    assert!(
        !generic_checker_src.contains("self.ensure_refs_resolved(type_arg);")
            && !generic_checker_src.contains("self.ensure_refs_resolved(instantiated_constraint);"),
        "generic constraint checks should rely on centralized assignability preconditions instead of local ref-resolution traversal"
    );
}

#[test]
fn test_type_cache_surface_excludes_application_and_mapped_eval_caches() {
    let context_src = fs::read_to_string("src/context/mod.rs")
        .expect("failed to read src/context/mod.rs for guard");

    let type_cache_start = context_src
        .find("pub struct TypeCache")
        .expect("missing TypeCache struct in context/mod.rs");
    let checker_context_start = context_src[type_cache_start..]
        .find("pub struct CheckerContext")
        .map(|offset| type_cache_start + offset)
        .expect("missing CheckerContext struct in context/mod.rs");
    let type_cache_src = &context_src[type_cache_start..checker_context_start];

    assert!(
        !type_cache_src.contains("application_eval_cache")
            && !type_cache_src.contains("application_eval_set")
            && !type_cache_src.contains("mapped_eval_cache")
            && !type_cache_src.contains("mapped_eval_set")
            && !type_cache_src.contains("abstract_constructor_types")
            && !type_cache_src.contains("protected_constructor_types")
            && !type_cache_src.contains("private_constructor_types"),
        "TypeCache should not persist checker algorithm caches (eval/constructor-access)"
    );

    assert!(
        !context_src
            .contains("abstract_constructor_types: parent.abstract_constructor_types.clone()")
            && !context_src.contains(
                "protected_constructor_types: parent.protected_constructor_types.clone()"
            )
            && !context_src
                .contains("private_constructor_types: parent.private_constructor_types.clone()"),
        "with_parent should keep constructor-access caches context-local"
    );

    assert!(
        !context_src.contains("contains_infer_types_true:")
            && !context_src.contains("contains_infer_types_false:"),
        "CheckerContext should not retain contains_infer_types memo caches; infer-shape queries should stay solver-owned"
    );
    assert!(
        !context_src.contains("application_eval_cache:")
            && !context_src.contains("mapped_eval_cache:"),
        "CheckerContext should not retain application/mapped evaluation result caches; evaluation memoization should stay solver-owned"
    );
}

#[test]
fn test_direct_assignability_mismatch_decision_usage_is_quarantined() {
    fn collect_checker_rs_files_recursive(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
        let entries = fs::read_dir(dir).unwrap_or_else(|_| {
            panic!("failed to read checker source directory {}", dir.display())
        });
        for entry in entries {
            let entry = entry.expect("failed to read checker source directory entry");
            let path = entry.path();
            if path.is_dir() {
                collect_checker_rs_files_recursive(&path, files);
                continue;
            }
            if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
                files.push(path);
            }
        }
    }

    let mut files = Vec::new();
    collect_checker_rs_files_recursive(Path::new("src"), &mut files);

    let mut violations = Vec::new();
    for path in files {
        let rel = path.display().to_string();
        let allowed = rel.ends_with("src/assignability/assignability_checker.rs")
            || rel.ends_with("src/assignability/assignability_diagnostics.rs")
            || rel.ends_with("src/query_boundaries/class.rs")
            || rel.ends_with("src/query_boundaries/type_checking.rs")
            || rel.contains("/tests/");
        if allowed {
            continue;
        }
        let src = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("failed to read {}", path.display()));
        if src.contains("should_report_assignability_mismatch_bivariant(") {
            violations.push(rel);
        }
    }

    assert!(
        violations.is_empty(),
        "direct should_report_assignability_mismatch_bivariant usage should stay in assignability/query boundary modules; violations: {}",
        violations.join(", ")
    );
}

#[test]
fn test_checker_sources_forbid_solver_internal_imports_typekey_usage_and_raw_interning() {
    fn is_rs_source_file(path: &Path) -> bool {
        path.extension().and_then(|ext| ext.to_str()) == Some("rs")
    }

    fn collect_rs_files_recursive(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
        let entries = fs::read_dir(dir).unwrap_or_else(|_| {
            panic!("failed to read checker source directory {}", dir.display())
        });
        for entry in entries {
            let entry = entry.expect("failed to read checker source directory entry");
            let path = entry.path();
            if path.is_dir() {
                collect_rs_files_recursive(&path, files);
                continue;
            }
            if is_rs_source_file(&path) {
                if path.components().any(|component| {
                    component.as_os_str() == "tests" || component.as_os_str() == "query_boundaries"
                }) {
                    continue;
                }
                files.push(path);
            }
        }
    }

    fn contains_type_data_ident(line: &str) -> bool {
        line.split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
            .any(|token| token == "TypeData")
    }

    fn has_forbidden_checker_type_construction_pattern(line: &str) -> bool {
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") {
            return false;
        }

        line.contains("tsz_solver::types::")
            || line.contains("use tsz_solver::TypeData")
            || line.contains("tsz_solver::TypeData::")
            || line.contains("TypeData::")
            || line.contains("use tsz_solver::TypeKey")
            || line.contains("tsz_solver::TypeKey::")
            || line.contains("TypeKey::")
            || (line.contains("use ") && contains_type_data_ident(line))
            || line.contains("intern(TypeData::")
            || line.contains("intern(tsz_solver::TypeData::")
            || line.contains(".intern(")
    }

    let src_dir = Path::new("src");
    let mut source_files = Vec::new();
    collect_rs_files_recursive(src_dir, &mut source_files);
    let mut violations = Vec::new();
    for path in source_files {
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .expect("checker source filename should be valid UTF-8");
        if file_name == "lib.rs" {
            continue;
        }
        // Allow complex.rs to check for unresolved Lazy types and resolve them by name.
        // This is necessary for lib interface constructor resolution (e.g., ProxyConstructor).
        if file_name == "complex.rs" {
            continue;
        }

        let source = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("failed to read {}", path.display()));
        for (line_index, line) in source.lines().enumerate() {
            if has_forbidden_checker_type_construction_pattern(line) {
                violations.push(format!("{}:{}", path.display(), line_index + 1));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "checker source files must not import solver internals, use TypeData/TypeKey directly, or call raw interner APIs directly; violations: {}",
        violations.join(", ")
    );
}

#[test]
fn test_constructor_checker_uses_solver_anchor_for_abstract_constructor_resolution() {
    let constructor_checker_src = fs::read_to_string("src/classes/constructor_checker.rs")
        .expect("failed to read src/classes/constructor_checker.rs");

    assert!(
        constructor_checker_src.contains("resolve_abstract_constructor_anchor("),
        "constructor_checker should resolve abstract constructor anchors through query boundaries"
    );
    assert!(
        !constructor_checker_src.contains("classify_for_abstract_constructor("),
        "constructor_checker should not perform abstract-constructor shape classification directly"
    );
}

#[test]
fn test_checker_legacy_type_arena_surface_is_removed() {
    let lib_src =
        fs::read_to_string("src/lib.rs").expect("failed to read src/lib.rs for architecture guard");
    assert!(
        !lib_src.contains("pub mod types;"),
        "legacy checker type module must stay removed."
    );
    assert!(
        !lib_src.contains("mod types;"),
        "legacy checker types module declaration must stay removed."
    );
    assert!(
        !lib_src.contains("pub mod arena;"),
        "legacy checker TypeArena module must stay removed."
    );
    assert!(
        !lib_src.contains("pub use arena::TypeArena;"),
        "legacy checker TypeArena re-export must stay removed."
    );
}

#[test]
fn test_diagnostics_property_name_collection_uses_solver_traversal_rules() {
    let interner = TypeInterner::new();

    let a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);
    let b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);
    let nested = interner.union(vec![a, b]);
    let root = interner.union(vec![nested]);

    let depth_0 = crate::query_boundaries::diagnostics::collect_property_name_atoms_for_diagnostics(
        &interner, root, 0,
    );
    assert!(depth_0.is_empty());

    let depth_1 = crate::query_boundaries::diagnostics::collect_property_name_atoms_for_diagnostics(
        &interner, root, 1,
    );
    let mut names: Vec<String> = depth_1
        .into_iter()
        .map(|atom| interner.resolve_atom_ref(atom).to_string())
        .collect();
    names.sort();
    assert_eq!(names, vec!["a".to_string(), "b".to_string()]);
}

#[test]
fn test_solver_sources_forbid_parser_checker_imports() {
    fn is_rs_source_file(path: &Path) -> bool {
        path.extension().and_then(|ext| ext.to_str()) == Some("rs")
    }

    fn collect_solver_rs_files_recursive(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
        let entries = fs::read_dir(dir)
            .unwrap_or_else(|_| panic!("failed to read solver source directory {}", dir.display()));
        for entry in entries {
            let entry = entry.expect("failed to read solver source directory entry");
            let path = entry.path();
            if path.is_dir() {
                collect_solver_rs_files_recursive(&path, files);
                continue;
            }
            if is_rs_source_file(&path) {
                if path
                    .components()
                    .any(|component| component.as_os_str() == "tests")
                {
                    continue;
                }
                files.push(path);
            }
        }
    }

    let solver_src_dir = Path::new("../tsz-solver/src");
    let mut source_files = Vec::new();
    collect_solver_rs_files_recursive(solver_src_dir, &mut source_files);

    let mut violations = Vec::new();
    for path in source_files {
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("failed to read {}", path.display()));
        for (line_index, line) in source.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            let has_forbidden_import =
                line.contains("tsz_parser::") || line.contains("tsz_checker::");
            if has_forbidden_import {
                violations.push(format!("{}:{}", path.display(), line_index + 1));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "solver source must not import parser/checker crates; violations: {}",
        violations.join(", ")
    );
}

#[test]
fn test_ambient_signature_checks_uses_assignability_query_boundary_helpers() {
    let mut src =
        fs::read_to_string("src/state/state_checking_members/ambient_signature_checks.rs")
            .expect("failed to read ambient signature checker for architecture guard");
    src.push_str(
        &fs::read_to_string("src/state/state_checking_members/overload_compatibility.rs")
            .expect("failed to read overload_compatibility.rs for architecture guard"),
    );
    assert!(
        !src.contains("tsz_solver::type_queries::rewrite_function_error_slots_to_any"),
        "ambient_signature_checks should route function error-slot rewrite via query boundaries"
    );
    assert!(
        !src.contains("tsz_solver::type_queries::replace_function_return_type"),
        "ambient_signature_checks should route function return replacement via query boundaries"
    );
    assert!(
        !src.contains("use tsz_solver::type_queries::get_return_type"),
        "ambient_signature_checks should route function return queries via query boundaries"
    );
}

#[test]
fn test_class_inheritance_paths_use_shared_class_declaration_lookup_helper() {
    let instance_src = fs::read_to_string("src/types/class_type/core.rs")
        .expect("failed to read src/types/class_type/core.rs for architecture guard");
    let constructor_src = fs::read_to_string("src/types/class_type/constructor.rs")
        .expect("failed to read src/types/class_type/constructor.rs for architecture guard");

    assert!(
        instance_src.contains("self.get_class_declaration_from_symbol(base_sym_id)"),
        "class_type should route base class declaration lookup through shared helper"
    );
    assert!(
        constructor_src.contains("self.get_class_declaration_from_symbol(base_sym_id)"),
        "class_type_constructor should route base class declaration lookup through shared helper"
    );
    assert!(
        !instance_src.contains("for &decl_idx in &base_symbol.declarations"),
        "class_type should not rescan base symbol declarations on hot inheritance path"
    );
    assert!(
        !constructor_src.contains("for &decl_idx in &base_symbol.declarations"),
        "class_type_constructor should not rescan base symbol declarations on hot inheritance path"
    );

    let instance_lookup_calls = instance_src
        .match_indices("self.get_class_declaration_from_symbol(base_sym_id)")
        .count();
    assert_eq!(
        instance_lookup_calls, 1,
        "class_type should resolve base declaration once per inheritance path"
    );
}

/// Architecture guard: all `push_diagnostic` calls must live in `error_reporter`/ or context/core.rs.
///
/// Direct `push_diagnostic` calls in feature modules bypass diagnostic centralization,
/// creating ad-hoc diagnostic paths that are harder to maintain and audit.
/// All diagnostic emission should route through `error_reporter` methods instead.
#[test]
fn test_no_push_diagnostic_outside_error_reporter() {
    fn collect_rs_files(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
        let entries = fs::read_dir(dir)
            .unwrap_or_else(|_| panic!("failed to read directory {}", dir.display()));
        for entry in entries {
            let entry = entry.expect("failed to read directory entry");
            let path = entry.path();
            if path.is_dir() {
                collect_rs_files(&path, files);
                continue;
            }
            if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
                files.push(path);
            }
        }
    }

    let mut files = Vec::new();
    collect_rs_files(Path::new("src"), &mut files);

    // Known exceptions (empty if all migrations are done)
    let allowlist: &[&str] = &[];

    let mut violations = Vec::new();
    for path in files {
        let rel = path.display().to_string();

        // Skip the legitimate homes for push_diagnostic:
        // - error_reporter/ is where all diagnostics should be emitted
        // - context/core.rs defines the push_diagnostic method itself
        // - tests/ are not production code
        if rel.contains("/error_reporter/")
            || rel.ends_with("context/core.rs")
            || rel.contains("/tests/")
        {
            continue;
        }

        // Check allowlist
        if allowlist.iter().any(|allowed| rel.ends_with(allowed)) {
            continue;
        }

        let src = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("failed to read {}", path.display()));

        for (line_num, line) in src.lines().enumerate() {
            // Skip comments
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with("///") || trimmed.starts_with("*") {
                continue;
            }
            if line.contains("push_diagnostic(") || line.contains(".push_diagnostic(") {
                violations.push(format!("{}:{}", rel, line_num + 1));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "push_diagnostic calls found outside error_reporter/. \
         Move these diagnostics to error_reporter/ methods instead.\n\
         Violations:\n  {}",
        violations.join("\n  ")
    );
}

/// Enforce the 2000 LOC limit for checker files (CLAUDE.md §12).
///
/// Files exceeding the limit are grandfathered with a ceiling that can only shrink.
/// New files must stay under 2000 lines.
#[test]
fn checker_files_stay_under_loc_limit() {
    let checker_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let loc_limit: usize = 2000;

    // Grandfathered files: (relative path from src/, ceiling LOC)
    // These ceilings represent the current state — they can only shrink, never grow.
    // Removed after dropping below 2000 LOC:
    //   complex.rs (926), variable_checking/core.rs (1606),
    //   symbol_types.rs (892), error_reporter/core.rs (1576),
    //   types/computation/call.rs (1805→split), checkers/call_checker.rs (1396),
    //   checkers/jsx/props/mod.rs, checkers/jsx/props/resolution.rs, checkers/jsx/props/validation.rs (1469)
    let grandfathered: &[(&str, usize)] = &[
        ("types/function_type.rs", 2000),
        ("state/type_analysis/computed_commonjs.rs", 2787),
        ("checkers/jsx/props/resolution.rs", 1600),
        ("checkers/jsx/orchestration", 2397),
        ("checkers/call_checker.rs", 2201),
        ("types/property_access_helpers.rs", 2104),
        ("types/property_access_type/resolve.rs", 2500),
        ("declarations/import/core.rs", 2562),
        ("declarations/import/declaration.rs", 2341),
        ("types/computation/call/inner.rs", 2010),
        ("jsdoc/resolution.rs", 2357),
        ("assignability/assignment_checker.rs", 2083),
        ("error_reporter/core.rs", 2358),
        ("error_reporter/call_errors.rs", 2554),
        ("types/type_checking/duplicate_identifiers_helpers.rs", 2125),
        ("types/type_checking/duplicate_identifiers.rs", 2051),
        ("error_reporter/render_failure.rs", 2240),
    ];

    let mut violations = Vec::new();

    fn walk_rs_files(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // Skip test directories
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                if name == "tests" {
                    continue;
                }
                walk_rs_files(&path, files);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                // Skip mod.rs and test files
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                if name == "mod.rs" || name.ends_with("_tests.rs") || name == "test_utils.rs" {
                    continue;
                }
                files.push(path);
            }
        }
    }

    let mut rs_files = Vec::new();
    walk_rs_files(&checker_src, &mut rs_files);

    for file_path in &rs_files {
        let Ok(content) = fs::read_to_string(file_path) else {
            continue;
        };
        // Count non-empty, non-comment lines
        let loc = content
            .lines()
            .filter(|line| {
                let trimmed = line.trim();
                !trimmed.is_empty() && !trimmed.starts_with("//")
            })
            .count();

        let relative = file_path
            .strip_prefix(&checker_src)
            .unwrap_or(file_path)
            .to_string_lossy()
            .replace('\\', "/");

        // Check against grandfathered ceiling or default limit
        let ceiling = grandfathered
            .iter()
            .find(|(path, _)| *path == relative)
            .map(|(_, ceil)| *ceil)
            .unwrap_or(loc_limit);

        if loc > ceiling {
            violations.push(format!(
                "File {relative} has {loc} lines (limit: {ceiling}). Split into submodules."
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "LOC violations found:\n{}",
        violations.join("\n")
    );
}

/// Enforce the `query_boundaries` policy: checker files outside `query_boundaries`/ and tests/
/// should only import "SAFE" items (type handles, structural shapes, visitors) from `tsz_solver`.
/// All computation/construction imports should go through `query_boundaries` wrappers.
///
/// The allowlist below enumerates items that are read-only type handles, structural shapes,
/// or visitor functions that don't perform computation. Everything else must be wrapped.
#[test]
fn test_solver_imports_go_through_query_boundaries() {
    // ── SAFE imports: type handles, structural shapes, visitor functions ──
    // These are read-only identity types or inspection functions that don't
    // perform computation. They may be imported directly by any checker file.
    //
    // Maintain this list alphabetically for easy auditing.
    const SAFE_IMPORTS: &[&str] = &[
        // Type identity handles
        "TypeId",
        "MappedTypeId",
        // Structural shape types (read-only data)
        "CallSignature",
        "CallableShape",
        "FunctionShape",
        "IndexKind",
        "IndexSignature",
        "ObjectShape",
        "ParamInfo",
        "PropertyInfo",
        "TupleElement",
        "TypeParamInfo",
        "TypePredicate",
        "TypePredicateTarget",
        "Visibility",
        // Narrowing/flow data types
        "GuardSense",
        "NarrowingContext",
        "SymbolRef",
        "TypeGuard",
        "TypeofKind",
        // Definition system types
        "def::DefId",
        "def::DefKind",
        "def::DefinitionInfo",
        "def::DefinitionStore",
        // Recursion control
        "recursion::DepthCounter",
        "recursion::RecursionGuard",
        "recursion::RecursionProfile",
        "recursion::RecursionResult",
        // Misc free functions used by a small number of checker files
        // (all others must go through query_boundaries/)
        "is_compiler_managed_type",
        "type_contains_undefined",
    ];

    // ── TODO: These imports bypass query_boundaries but wrappers don't exist yet. ──
    // Each entry is (item, list of files using it). When a wrapper is created,
    // remove the entry and let the test enforce the boundary.
    const TEMPORARILY_ALLOWED: &[&str] = &[
        // All solver imports now go through query_boundaries — list is empty.
    ];

    fn walk_rs(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk_rs(&path, files);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                files.push(path);
            }
        }
    }

    let checker_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    walk_rs(&checker_src, &mut files);

    /// Recursively expand a `use tsz_solver::...` import statement into
    /// individual canonical item paths. Handles nested brace groups like:
    ///   `{TypeId, type_queries::{Foo, Bar}}` -> ["TypeId", "`type_queries::Foo`", "`type_queries::Bar`"]
    /// Also strips `as Alias` suffixes so `CallSignature as SolverCallSignature`
    /// is checked as just `CallSignature`.
    fn expand_import(raw: &str) -> Vec<String> {
        let raw = raw.trim().trim_end_matches(';').trim();

        // Split a top-level comma-separated list respecting brace nesting.
        fn split_top_level(s: &str) -> Vec<String> {
            let mut items = Vec::new();
            let mut depth = 0;
            let mut start = 0;
            for (i, c) in s.char_indices() {
                match c {
                    '{' => depth += 1,
                    '}' => depth -= 1,
                    ',' if depth == 0 => {
                        let item = s[start..i].trim();
                        if !item.is_empty() {
                            items.push(item.to_string());
                        }
                        start = i + 1;
                    }
                    _ => {}
                }
            }
            let last = s[start..].trim();
            if !last.is_empty() {
                items.push(last.to_string());
            }
            items
        }

        fn expand_with_prefix(prefix: &str, body: &str) -> Vec<String> {
            let body = body.trim();
            if body.starts_with('{') && body.ends_with('}') {
                let inner = &body[1..body.len() - 1];
                let parts = split_top_level(inner);
                let mut result = Vec::new();
                for part in parts {
                    result.extend(expand_with_prefix(prefix, &part));
                }
                return result;
            }

            // Check for nested module path: `mod::{A, B}` or `mod::Item`
            if let Some(brace_start) = body.find('{') {
                // Find the `::` before the brace
                let before_brace = body[..brace_start].trim_end_matches(':');
                let sub_prefix = if prefix.is_empty() {
                    before_brace.trim_end_matches(':').to_string()
                } else {
                    format!("{}::{}", prefix, before_brace.trim_end_matches(':'))
                };
                let rest = &body[brace_start..];
                return expand_with_prefix(&sub_prefix, rest);
            }

            // Strip `as Alias` suffix
            let item = if let Some(as_pos) = body.find(" as ") {
                body[..as_pos].trim()
            } else {
                body.trim()
            };

            if item.is_empty() {
                return vec![];
            }

            if prefix.is_empty() {
                vec![item.to_string()]
            } else {
                vec![format!("{}::{}", prefix, item)]
            }
        }

        expand_with_prefix("", raw)
    }

    fn is_allowed(item: &str, safe: &[&str], temp: &[&str]) -> bool {
        safe.contains(&item) || temp.contains(&item)
    }

    let mut violations = Vec::new();

    for path in &files {
        let rel = path
            .strip_prefix(&checker_src)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        // Skip excluded directories
        if rel.starts_with("tests/") || rel.starts_with("query_boundaries/") {
            continue;
        }

        let src = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        // Find all `use tsz_solver::...;` imports (handles multi-line with braces)
        // We scan for lines starting with `use tsz_solver::` and collect until `;`
        // Imports inside `mod tests { ... }` blocks are exempt (test-only setup).
        let mut in_use = false;
        let mut use_buf = String::new();
        // Track whether we are inside an inline test module (`mod tests { ... }`).
        // We use simple open/close brace counting once we see `mod tests`.
        let mut in_test_mod_depth: i32 = 0;

        for line in src.lines() {
            let trimmed = line.trim();
            // Skip comments
            if trimmed.starts_with("//") || trimmed.starts_with("///") {
                continue;
            }

            // Detect entry into a test module and track depth.
            if in_test_mod_depth == 0
                && (trimmed.starts_with("mod tests")
                    || trimmed.starts_with("#[cfg(test)]")
                    || trimmed == "#[cfg(test)]")
            {
                // Count any opening braces on this line to start tracking.
                for ch in trimmed.chars() {
                    if ch == '{' {
                        in_test_mod_depth += 1;
                    } else if ch == '}' {
                        in_test_mod_depth -= 1;
                    }
                }
                continue;
            }
            if in_test_mod_depth > 0 {
                for ch in trimmed.chars() {
                    if ch == '{' {
                        in_test_mod_depth += 1;
                    } else if ch == '}' {
                        in_test_mod_depth -= 1;
                    }
                }
                continue; // skip all content inside test modules
            }

            if !in_use {
                if let Some(rest) = trimmed.strip_prefix("use tsz_solver::") {
                    use_buf.clear();
                    use_buf.push_str(rest);
                    if rest.contains(';') {
                        in_use = false;
                        let stmt = use_buf.trim_end_matches(';').to_string();
                        for item in expand_import(&stmt) {
                            if !is_allowed(&item, SAFE_IMPORTS, TEMPORARILY_ALLOWED) {
                                violations.push(format!(
                                    "File {rel} imports tsz_solver::{item} directly. \
                                     Add a wrapper in query_boundaries/ and use that instead."
                                ));
                            }
                        }
                    } else {
                        in_use = true;
                    }
                }
            } else {
                use_buf.push(' ');
                use_buf.push_str(trimmed);
                if trimmed.contains(';') {
                    in_use = false;
                    let stmt = use_buf.trim_end_matches(';').to_string();
                    for item in expand_import(&stmt) {
                        if !is_allowed(&item, SAFE_IMPORTS, TEMPORARILY_ALLOWED) {
                            violations.push(format!(
                                "File {rel} imports tsz_solver::{item} directly. \
                                 Add a wrapper in query_boundaries/ and use that instead."
                            ));
                        }
                    }
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "query_boundaries policy violations found. Non-allowlisted tsz_solver \
         imports detected outside query_boundaries/:\n  {}",
        violations.join("\n  ")
    );
}

// =============================================================================
// Prompt 4.1 — Architecture Invariant Coverage Checklist
// =============================================================================
//
// CLAUDE.md Rule -> Test Coverage mapping:
//
// SECTION 3: Responsibility Split
// - [x] Scanner: no downstream imports                    -> test_scanner_must_not_import_downstream_crates
// - [x] Parser: no binder/checker/solver imports          -> test_parser_must_not_import_binder_checker_solver
// - [x] Binder: no solver imports                         -> test_binder_must_not_import_solver
// - [x] Emitter: no checker internal imports              -> test_emitter_must_not_import_checker_internals
// - [x] Solver: no parser/checker imports                 -> test_solver_sources_forbid_parser_checker_imports (existing)
//
// SECTION 4: Hard Architecture Rules
// - [x] No TypeKey in checker                             -> test_checker_sources_forbid_solver_internal_imports_typekey_usage_and_raw_interning (existing)
// - [x] No raw interner access in checker                 -> test_checker_sources_forbid_solver_internal_imports_typekey_usage_and_raw_interning (existing)
// - [x] No TypeData construction in checker               -> test_checker_sources_forbid_solver_internal_imports_typekey_usage_and_raw_interning (existing)
// - [x] CallEvaluator quarantined to query_boundaries     -> test_direct_call_evaluator_usage_is_quarantined_to_query_boundaries (existing)
// - [x] No SubtypeChecker construction outside boundaries -> test_no_direct_subtype_checker_construction_outside_query_boundaries
// - [x] No CompatChecker::with_resolver outside boundaries-> assignability/call boundary guards (existing)
// - [x] Solver imports go through query_boundaries        -> test_solver_imports_go_through_query_boundaries (existing)
//
// SECTION 5: Judge/Lawyer Model
// - [x] No direct CompatChecker for TS2322 paths          -> test_assignment_and_binding_default_assignability_use_central_gateway_helpers (existing)
// - [x] Assignability mismatch quarantined                -> test_direct_assignability_mismatch_decision_usage_is_quarantined (existing)
//
// SECTION 6: DefId-First Semantic Type Resolution
// - [x] No ad-hoc TypeData::Lazy interning                -> test_array_helpers_avoid_direct_typekey_interning (existing)
// - [x] ensure_relation_input_ready used before relations  -> test_subtype_path_establishes_preconditions_before_subtype_cache_lookup (existing)
//
// SECTION 11: Solver Contracts
// - [x] No solver cache types in checker                  -> test_no_solver_cache_types_in_checker
// - [x] TypeCache excludes eval caches                    -> test_type_cache_surface_excludes_application_and_mapped_eval_caches (existing)
//
// SECTION 12: Checker Contracts
// - [x] Checker files under 2000 LOC                      -> checker_files_stay_under_loc_limit (existing)
// - [x] All diagnostics through error_reporter            -> test_no_push_diagnostic_outside_error_reporter (existing)
// - [x] query_boundaries coverage ratio tracking          -> test_query_boundaries_coverage_ratio
//
// SECTION 13: Emitter Contracts
// - [x] error_reporter is pure formatting layer           -> test_error_reporter_does_not_perform_type_construction
//
// SECTION 15: Dependency Policy
// - [x] Dependency direction enforcement                  -> tests in Prompt 4.2 below
// - [x] No checker access to solver internals             -> test_checker_sources_forbid_solver_internal_imports_typekey_usage_and_raw_interning (existing)
//
// SECTION 22: TS2322 Priority Rules
// - [x] TS2322 paths through query_boundaries             -> test_assignment_and_binding_default_assignability_use_central_gateway_helpers (existing)
// - [x] No direct CompatChecker for TS2322                -> call boundary guard (existing)
// - [x] Centralized assignability gateways                -> multiple existing tests
//
// RATCHET GUARDS (debt tracking):
// - [x] TEMPORARILY_ALLOWED bypass list capped at 38      -> test_temporarily_allowed_bypass_list_does_not_grow
// - [x] Direct interner type construction capped at 13    -> test_direct_interner_type_construction_ceiling
// - [x] Checker file size ceiling (4 files > 2000 LOC)    -> test_checker_file_size_ceiling
// - [x] Max single file LOC ceiling (2394 lines)          -> test_checker_file_size_ceiling
// - [x] CLI must not import checker internals             -> test_cli_must_not_import_checker_internals
//

// =============================================================================
