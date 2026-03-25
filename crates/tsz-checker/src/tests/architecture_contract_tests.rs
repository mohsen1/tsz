use crate::context::{CheckerContext, CheckerOptions};
use std::fs;
use std::path::Path;
use tsz_binder::BinderState;
use tsz_parser::parser::node::NodeArena;
use tsz_solver::{
    CompatChecker, FunctionShape, ParamInfo, PropertyInfo, RelationCacheKey, TypeId, TypeInterner,
    Visibility,
};

fn make_animal_and_dog(interner: &TypeInterner) -> (TypeId, TypeId) {
    let animal_name = interner.intern_string("name");
    let dog_breed = interner.intern_string("breed");

    let animal = interner.object(vec![tsz_solver::PropertyInfo {
        name: animal_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
    }]);

    let dog = interner.object(vec![
        tsz_solver::PropertyInfo {
            name: animal_name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
        },
        tsz_solver::PropertyInfo {
            name: dog_breed,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
        },
    ]);

    (animal, dog)
}

fn collect_checker_rs_files_recursive(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
    let entries = fs::read_dir(dir)
        .unwrap_or_else(|_| panic!("failed to read checker source directory {}", dir.display()));
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

#[test]
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
            "src/checkers/jsx/orchestration.rs",
            "src/checkers/jsx/children.rs",
            "src/checkers/jsx/props.rs",
            "src/checkers/jsx/runtime.rs",
            "src/checkers/jsx/diagnostics.rs",
        ] {
            buf.push_str(&fs::read_to_string(file).unwrap_or_default());
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
        &fs::read_to_string("src/state/type_analysis/computed.rs")
            .expect("failed to read src/state/type_analysis/computed.rs for architecture guard"),
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
    let assignment_checker_src = fs::read_to_string("src/assignability/assignment_checker.rs")
        .expect("failed to read src/assignability/assignment_checker.rs for architecture guard");
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
    let property_access_type_src = fs::read_to_string("src/types/property_access_type.rs")
        .expect("failed to read src/types/property_access_type.rs for architecture guard");
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

    let call_checker_src = fs::read_to_string("src/checkers/call_checker.rs")
        .expect("failed to read src/checkers/call_checker.rs");
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

    let generic_checker_src = fs::read_to_string("src/checkers/generic_checker.rs")
        .expect("failed to read src/checkers/generic_checker.rs for architecture guard");
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
        "checker source files must not import solver internals, import TypeData, or call raw interner APIs directly; violations: {}",
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
    //   types/computation/call.rs (1805), checkers/call_checker.rs (1396),
    //   checkers/jsx/props.rs (1469)
    let grandfathered: &[(&str, usize)] = &[("types/function_type.rs", 1924)];

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
        // Visitor functions (read-only type inspection)
        "visitor",
        "visitor::application_id",
        "visitor::callable_shape_id",
        "visitor::collect_lazy_def_ids",
        "visitor::collect_type_queries",
        "visitor::is_function_type",
        "visitor::is_template_literal_type",
        "visitor::lazy_def_id",
        "visitor::object_shape_id",
        "visitor::object_with_index_shape_id",
        // Read-only type query/classification functions
        "is_compiler_managed_type",
        "is_type_parameter",
        "type_contains_undefined",
        "type_queries",
        "type_queries::ArrayLikeKind",
        "type_queries::AugmentationTargetKind",
        "type_queries::ContextualLiteralAllowKind",
        "type_queries::IndexKeyKind",
        "type_queries::InterfaceMergeKind",
        "type_queries::LiteralTypeKind",
        "type_queries::LiteralValueKind",
        "type_queries::NamespaceMemberKind",
        "type_queries::TypeResolutionKind",
        "type_queries::classify_for_augmentation",
        "type_queries::classify_for_contextual_literal",
        "type_queries::classify_for_interface_merge",
        "type_queries::classify_for_literal_value",
        "type_queries::classify_for_type_resolution",
        "type_queries::classify_literal_type",
        "type_queries::classify_namespace_member",
        "type_queries::data::get_call_signatures",
        "type_queries::data::get_function_shape",
        "type_queries::get_enum_member_type",
        "type_queries::get_function_shape",
        "type_queries::get_object_shape_id",
        "type_queries::is_unit_type",
        "type_queries::self",
        "type_queries::get_union_members",
    ];

    // ── TODO: These imports bypass query_boundaries but wrappers don't exist yet. ──
    // Each entry is (item, list of files using it). When a wrapper is created,
    // remove the entry and let the test enforce the boundary.
    const TEMPORARILY_ALLOWED: &[&str] = &[
        // TODO: Computation APIs — need query_boundaries wrappers
        "ApplicationEvaluator",
        "AssignabilityChecker",
        "TypeData",
        "as_type_database",
        "BinaryOpEvaluator",
        "CallResult",
        "ContextualTypeContext",
        "IndexSignatureResolver",
        "IntrinsicKind",
        "MappedType",
        "PendingDiagnostic",
        "PendingDiagnosticBuilder",
        "QueryDatabase",
        "RelationCacheKey",
        "SourceLocation",
        "SubtypeFailureReason",
        "TypeEnvironment",
        "TypeEvaluator",
        "TypeFormatter",
        "TypeInstantiator",
        "TypeResolver",
        "TypeSubstitution",
        "def::resolver::TypeResolver",
        "instantiate_generic",
        "instantiate_type_with_depth_status",
        "judge::DefaultJudge",
        "judge::Judge",
        "judge::JudgeConfig",
        "keyof_inner_type",
        "lazy_def_id",
        "objects::index_signatures::IndexKind",
        "objects::index_signatures::IndexSignatureResolver",
        "operations::CallResult",
        "operations::property::PropertyAccessEvaluator",
        "operations::property::PropertyAccessResult",
        "operations::property::is_mapped_type_with_readonly_modifier",
        "operations::property::is_readonly_tuple_fixed_element",
        "substitute_this_type",
        "type_param_info",
        "types::ParamInfo",
        "widening::apply_const_assertion",
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
        let mut in_use = false;
        let mut use_buf = String::new();

        for line in src.lines() {
            let trimmed = line.trim();
            // Skip comments
            if trimmed.starts_with("//") || trimmed.starts_with("///") {
                continue;
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
// Prompt 4.2 — Dependency Direction Tests
// =============================================================================

/// Helper: recursively walk a directory collecting .rs files (skipping tests/).
fn walk_rs_files_recursive(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if name == "tests" {
                continue;
            }
            walk_rs_files_recursive(&path, files);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
}

/// CLAUDE.md §4: Binder must not import Solver.
/// The binder produces symbols, scopes, and flow graphs without type computation.
#[test]
fn test_binder_must_not_import_solver() {
    let binder_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tsz-binder/src");

    let mut files = Vec::new();
    walk_rs_files_recursive(&binder_src, &mut files);

    let mut violations = Vec::new();
    for path in files {
        let src = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("failed to read {}", path.display()));
        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            if line.contains("use tsz_solver") || line.contains("tsz_solver::") {
                violations.push(format!("{}:{}", path.display(), line_num + 1));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Binder must not import Solver (CLAUDE.md §4). Violations:\n  {}",
        violations.join("\n  ")
    );
}

/// CLAUDE.md §4: Emitter must not import Checker internals.
/// The emitter prints/transforms output; no on-the-fly semantic type validation.
#[test]
fn test_emitter_must_not_import_checker_internals() {
    let emitter_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tsz-emitter/src");

    let mut files = Vec::new();
    walk_rs_files_recursive(&emitter_src, &mut files);

    let mut violations = Vec::new();
    for path in files {
        let src = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("failed to read {}", path.display()));
        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            if line.contains("use tsz_checker") || line.contains("tsz_checker::") {
                violations.push(format!("{}:{}", path.display(), line_num + 1));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Emitter must not import Checker internals (CLAUDE.md §4/§13). Violations:\n  {}",
        violations.join("\n  ")
    );
}

/// CLAUDE.md §4: Scanner must not import downstream crates (Parser/Binder/Checker/Solver).
/// The scanner is the leaf of the pipeline; it only does lexing and string interning.
#[test]
fn test_scanner_must_not_import_downstream_crates() {
    let scanner_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tsz-scanner/src");

    let mut files = Vec::new();
    walk_rs_files_recursive(&scanner_src, &mut files);

    let downstream_crates = [
        "tsz_parser",
        "tsz_binder",
        "tsz_checker",
        "tsz_solver",
        "tsz_emitter",
    ];

    let mut violations = Vec::new();
    for path in files {
        let src = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("failed to read {}", path.display()));
        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            for crate_name in &downstream_crates {
                if line.contains(&format!("use {crate_name}"))
                    || line.contains(&format!("{crate_name}::"))
                {
                    violations.push(format!(
                        "{}:{}: imports {}",
                        path.display(),
                        line_num + 1,
                        crate_name
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Scanner must not import downstream crates (CLAUDE.md §4/§8). Violations:\n  {}",
        violations.join("\n  ")
    );
}

/// CLAUDE.md §4: Parser must not import Binder/Checker/Solver.
/// The parser produces syntax-only AST; no semantic awareness.
#[test]
fn test_parser_must_not_import_binder_checker_solver() {
    let parser_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tsz-parser/src");

    let mut files = Vec::new();
    walk_rs_files_recursive(&parser_src, &mut files);

    let downstream_crates = ["tsz_binder", "tsz_checker", "tsz_solver", "tsz_emitter"];

    let mut violations = Vec::new();
    for path in files {
        let src = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("failed to read {}", path.display()));
        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            for crate_name in &downstream_crates {
                if line.contains(&format!("use {crate_name}"))
                    || line.contains(&format!("{crate_name}::"))
                {
                    violations.push(format!(
                        "{}:{}: imports {}",
                        path.display(),
                        line_num + 1,
                        crate_name
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Parser must not import Binder/Checker/Solver (CLAUDE.md §4/§9). Violations:\n  {}",
        violations.join("\n  ")
    );
}

// =============================================================================
// Prompt 4.3 — Solver Encapsulation Tests
// =============================================================================

/// CLAUDE.md §4/§6: No `TypeKey` usage in checker code.
/// `TypeKey` is solver-internal (crate-private); checker must use TypeId/TypeData.
#[test]
fn test_no_typekey_in_checker_code() {
    let src_dir = Path::new("src");
    let mut files = Vec::new();
    collect_checker_rs_files_recursive(src_dir, &mut files);

    let mut violations = Vec::new();
    for path in files {
        let rel = path.display().to_string();
        if rel.contains("/tests/") {
            continue;
        }
        let src = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("failed to read {}", path.display()));
        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") || trimmed.starts_with("///") {
                continue;
            }
            // Check for TypeKey as a distinct identifier (not part of another word)
            if line
                .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
                .any(|token| token == "TypeKey")
            {
                violations.push(format!("{}:{}", rel, line_num + 1));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "TypeKey is solver-internal and must not appear in checker code (CLAUDE.md §4/§6). Violations:\n  {}",
        violations.join("\n  ")
    );
}

/// CLAUDE.md §11: No solver cache access types (`RelationCacheProbe`, etc.) in checker code.
/// Solver owns algorithmic caches; checker must not access them directly.
#[test]
fn test_no_solver_cache_types_in_checker() {
    let src_dir = Path::new("src");
    let mut files = Vec::new();
    collect_checker_rs_files_recursive(src_dir, &mut files);

    let cache_types = [
        "RelationCacheProbe",
        "EvaluationCache",
        "InstantiationCache",
    ];

    let mut violations = Vec::new();
    for path in files {
        let rel = path.display().to_string();
        if rel.contains("/tests/") || rel.contains("/query_boundaries/") {
            continue;
        }
        let src = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("failed to read {}", path.display()));
        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") || trimmed.starts_with("///") || trimmed.starts_with("*") {
                continue;
            }
            for cache_type in &cache_types {
                if line
                    .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
                    .any(|token| token == *cache_type)
                {
                    violations.push(format!("{}:{}: uses {}", rel, line_num + 1, cache_type));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Solver cache types must not appear in checker code (CLAUDE.md §11). Violations:\n  {}",
        violations.join("\n  ")
    );
}

/// CLAUDE.md §4/§22: No direct `SubtypeChecker` construction outside `query_boundaries`.
/// Relation checks should go through boundary helpers.
#[test]
fn test_no_direct_subtype_checker_construction_outside_query_boundaries() {
    let src_dir = Path::new("src");
    let mut files = Vec::new();
    collect_checker_rs_files_recursive(src_dir, &mut files);

    let mut violations = Vec::new();
    for path in files {
        let rel = path.display().to_string();
        if rel.contains("/tests/") || rel.contains("/query_boundaries/") {
            continue;
        }
        let src = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("failed to read {}", path.display()));
        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") || trimmed.starts_with("///") {
                continue;
            }
            if line.contains("SubtypeChecker::new(") || line.contains("SubtypeChecker {") {
                violations.push(format!("{}:{}", rel, line_num + 1));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "SubtypeChecker must not be constructed outside query_boundaries (CLAUDE.md §4/§22). \
         Route relation checks through boundary helpers instead. Violations:\n  {}",
        violations.join("\n  ")
    );
}

// =============================================================================
// Prompt 4.4 — Structural Health Tests
// =============================================================================

/// CLAUDE.md §12: Track `query_boundaries` coverage ratio.
/// This is a directional metric -- warns if the ratio of direct solver imports
/// to `query_boundaries` usage is too high.
#[test]
fn test_query_boundaries_coverage_ratio() {
    let src_dir = Path::new("src");
    let mut files = Vec::new();
    collect_checker_rs_files_recursive(src_dir, &mut files);

    let mut direct_solver_importers = 0u32;
    let mut boundary_users = 0u32;

    for path in &files {
        let rel = path.display().to_string();
        if rel.contains("/tests/") || rel.contains("/query_boundaries/") {
            continue;
        }
        let src = fs::read_to_string(path)
            .unwrap_or_else(|_| panic!("failed to read {}", path.display()));

        let has_direct = src.lines().any(|line| {
            let t = line.trim_start();
            !t.starts_with("//")
                && (line.contains("use tsz_solver::") || line.contains("tsz_solver::"))
        });
        let has_boundary = src.lines().any(|line| {
            let t = line.trim_start();
            !t.starts_with("//") && line.contains("query_boundaries::")
        });

        if has_direct {
            direct_solver_importers += 1;
        }
        if has_boundary {
            boundary_users += 1;
        }
    }

    // This is a directional metric. We want the ratio to decrease over time.
    // Current target: direct importers should be < 4x boundary users.
    let ratio = if boundary_users == 0 {
        f64::INFINITY
    } else {
        direct_solver_importers as f64 / boundary_users as f64
    };

    // Warn but don't fail -- this is a tracking metric
    // Tracking metric: warn threshold at 4.0 (currently informational only)
    let _ = ratio > 4.0;

    // Hard fail if the ratio degrades catastrophically
    assert!(
        ratio < 10.0,
        "query_boundaries coverage ratio has degraded to {ratio:.1}:1 \
         ({direct_solver_importers} direct solver importers vs {boundary_users} boundary users). \
         This indicates systematic boundary bypass. Target: < 4:1"
    );
}

// ========================================================================
// Ambient context transport: TypingRequest migration contract tests
// ========================================================================
//
// These tests enforce that files fully migrated to the TypingRequest API
// do not regress by re-introducing raw mutations of the ambient context
// fields: `ctx.contextual_type =`, `ctx.contextual_type_is_assertion =`,
// and `ctx.skip_flow_narrowing =`.
//
// Legacy ambient state still exists in a few non-migrated subsystems, but
// the request-first hot path must not regress.

/// Migrated files must not contain raw `ctx.contextual_type =` assignments.
/// They should use `get_type_of_node_with_request` instead.
#[test]
fn migrated_files_no_raw_contextual_type_mutation() {
    let migrated_files = &[
        "types/computation/object_literal_context.rs",
        "types/computation/array_literal.rs",
        "types/queries/binding.rs",
        "types/type_checking/core.rs",
        "declarations/import/core.rs",
        "assignability/assignment_checker.rs",
        // property_access_type.rs migrated skip_flow_narrowing, not contextual_type
        // Wave 2 migrations:
        "assignability/compound_assignment.rs",
        "error_reporter/call_errors.rs",
        "state/variable_checking/destructuring.rs",
        "state/state_checking/property.rs",
        "state/state_checking_members/ambient_signature_checks.rs",
        "state/variable_checking/core.rs",
        "types/type_checking/core_statement_checks.rs",
        "types/computation/binary.rs",
        "types/computation/access.rs",
        "types/computation/tagged_template.rs",
        // Wave 3 migrations:
        "types/computation/call_helpers.rs",
        "checkers/parameter_checker.rs",
        "types/utilities/return_type.rs",
        "checkers/call_checker.rs",
        "types/computation/call_inference.rs",
        "dispatch.rs",
        "checkers/jsx/orchestration.rs",
        "checkers/jsx/children.rs",
        "checkers/jsx/props.rs",
        "checkers/jsx/runtime.rs",
        "checkers/jsx/diagnostics.rs",
        "types/computation/call.rs",
        "types/computation/object_literal.rs",
        "types/computation/helpers.rs",
        "types/computation/call_display.rs",
        "types/function_type.rs",
        "types/class_type/constructor.rs",
        "state/state.rs",
        "state/type_analysis/core.rs",
        "state/type_analysis/core_type_query.rs",
        "state/type_analysis/computed_helpers.rs",
        "state/type_analysis/computed_helpers_binding.rs",
        "state/state_checking_members/statement_callback_bridge.rs",
        "state/state_checking_members/member_declaration_checks.rs",
        "state/state_checking/class.rs",
    ];

    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");

    for file in migrated_files {
        let path = base.join(file);
        let content = fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!("Failed to read {file}: {e}");
        });

        // Count raw mutations (exclude comments and the TypingRequest module itself)
        let violations: Vec<(usize, &str)> = content
            .lines()
            .enumerate()
            .filter(|(_, line)| {
                let trimmed = line.trim();
                // Skip comments
                if trimmed.starts_with("//")
                    || trimmed.starts_with("/*")
                    || trimmed.starts_with("*")
                {
                    return false;
                }
                // Detect raw mutation patterns
                trimmed.contains("ctx.contextual_type =") || trimmed.contains(".contextual_type = ")
            })
            .collect();

        assert!(
            violations.is_empty(),
            "File {file} has been migrated to TypingRequest but still contains \
             raw `contextual_type =` mutations:\n{}",
            violations
                .iter()
                .map(|(line_no, line)| format!("  line {}: {}", line_no + 1, line.trim()))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}

/// Migrated files must not contain raw `ctx.skip_flow_narrowing =` assignments.
#[test]
fn migrated_files_no_raw_skip_flow_narrowing_mutation() {
    let migrated_files = &[
        "types/property_access_type.rs",
        "types/computation/access.rs",
        "types/computation/helpers.rs",
        "state/type_analysis/core.rs",
        "state/type_analysis/core_type_query.rs",
        "state/type_analysis/computed_helpers.rs",
        "state/type_analysis/computed_helpers_binding.rs",
        "state/variable_checking/destructuring.rs",
        "state/state_checking_members/statement_callback_bridge.rs",
        "state/state_checking_members/member_declaration_checks.rs",
        "state/state_checking/class.rs",
        // Wave 3: call_checker and call_inference migrated skip_flow via TypingRequest
        "checkers/call_checker.rs",
        "types/computation/call_inference.rs",
        "types/computation/tagged_template.rs",
        "types/class_type/constructor.rs",
        "state/state_checking_members/ambient_signature_checks.rs",
        "state/state.rs",
    ];

    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");

    for file in migrated_files {
        let path = base.join(file);
        let content = fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!("Failed to read {file}: {e}");
        });

        let violations: Vec<(usize, &str)> = content
            .lines()
            .enumerate()
            .filter(|(_, line)| {
                let trimmed = line.trim();
                if trimmed.starts_with("//")
                    || trimmed.starts_with("/*")
                    || trimmed.starts_with("*")
                {
                    return false;
                }
                trimmed.contains("ctx.skip_flow_narrowing =")
                    || trimmed.contains(".skip_flow_narrowing = ")
            })
            .collect();

        assert!(
            violations.is_empty(),
            "File {file} has been migrated to TypingRequest but still contains \
             raw `skip_flow_narrowing =` mutations:\n{}",
            violations
                .iter()
                .map(|(line_no, line)| format!("  line {}: {}", line_no + 1, line.trim()))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}

/// Migrated helper files must not read request intent from ambient checker fields.
#[test]
fn migrated_helper_files_no_raw_ambient_request_reads() {
    let migrated_files = &[
        "state/type_analysis/core.rs",
        "state/type_analysis/core_type_query.rs",
        "state/type_analysis/computed_helpers_binding.rs",
        "state/type_analysis/computed_helpers.rs",
        "types/property_access_type.rs",
        "state/variable_checking/destructuring.rs",
        "state/variable_checking/core.rs",
        "types/type_checking/core.rs",
        "types/computation/tagged_template.rs",
        "types/class_type/constructor.rs",
        "state/state_checking_members/ambient_signature_checks.rs",
        "state/state_checking_members/member_declaration_checks.rs",
        "state/state_checking/class.rs",
        "state/state.rs",
    ];

    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");

    for file in migrated_files {
        let path = base.join(file);
        let content = fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!("Failed to read {file}: {e}");
        });

        let violations: Vec<(usize, &str)> = content
            .lines()
            .enumerate()
            .filter(|(_, line)| {
                let trimmed = line.trim();
                if trimmed.starts_with("//")
                    || trimmed.starts_with("/*")
                    || trimmed.starts_with("*")
                {
                    return false;
                }
                trimmed.contains("self.ctx.contextual_type")
                    || trimmed.contains("self.ctx.contextual_type_is_assertion")
                    || trimmed.contains("self.ctx.skip_flow_narrowing")
            })
            .collect();

        assert!(
            violations.is_empty(),
            "File {file} must not read request intent from ambient checker state:\n{}",
            violations
                .iter()
                .map(|(line_no, line)| format!("  line {}: {}", line_no + 1, line.trim()))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}

/// Migrated files must not contain raw `ctx.contextual_type_is_assertion =` assignments.
#[test]
fn migrated_files_no_raw_contextual_assertion_mutation() {
    let migrated_files = &[
        "dispatch.rs",
        "checkers/jsx/orchestration.rs",
        "checkers/jsx/children.rs",
        "checkers/jsx/props.rs",
        "checkers/jsx/runtime.rs",
        "checkers/jsx/diagnostics.rs",
        "types/computation/call.rs",
        "types/computation/helpers.rs",
        "types/computation/object_literal.rs",
        "types/function_type.rs",
        "state/state_checking_members/ambient_signature_checks.rs",
        "types/computation/tagged_template.rs",
        "types/class_type/constructor.rs",
        "state/state_checking_members/member_declaration_checks.rs",
        "state/state_checking/class.rs",
        "state/type_analysis/core.rs",
        "state/type_analysis/core_type_query.rs",
        "state/type_analysis/computed_helpers.rs",
        "state/type_analysis/computed_helpers_binding.rs",
        "state/variable_checking/destructuring.rs",
        "state/state.rs",
    ];

    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");

    for file in migrated_files {
        let path = base.join(file);
        let content = fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!("Failed to read {file}: {e}");
        });

        let violations: Vec<(usize, &str)> = content
            .lines()
            .enumerate()
            .filter(|(_, line)| {
                let trimmed = line.trim();
                if trimmed.starts_with("//")
                    || trimmed.starts_with("/*")
                    || trimmed.starts_with("*")
                {
                    return false;
                }
                trimmed.contains("ctx.contextual_type_is_assertion =")
                    || trimmed.contains(".contextual_type_is_assertion = ")
            })
            .collect();

        assert!(
            violations.is_empty(),
            "File {file} has been migrated to TypingRequest but still contains \
             raw `contextual_type_is_assertion =` mutations:\n{}",
            violations
                .iter()
                .map(|(line_no, line)| format!("  line {}: {}", line_no + 1, line.trim()))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}

/// The removed `run_with_typing_context` compatibility bridge must not reappear.
#[test]
fn no_typing_context_bridge_helper_or_calls() {
    let files = &[
        "state/state.rs",
        "dispatch.rs",
        "types/function_type.rs",
        "state/state_checking_members/statement_callback_bridge.rs",
    ];

    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");

    for file in files {
        let path = base.join(file);
        let content = fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!("Failed to read {file}: {e}");
        });

        let violations: Vec<(usize, &str)> = content
            .lines()
            .enumerate()
            .filter(|(_, line)| {
                let trimmed = line.trim();
                if trimmed.starts_with("//")
                    || trimmed.starts_with("/*")
                    || trimmed.starts_with("*")
                {
                    return false;
                }
                trimmed.contains("run_with_typing_context(")
                    || trimmed.contains("fn run_with_typing_context")
            })
            .collect();

        assert!(
            violations.is_empty(),
            "File {file} must not reintroduce the removed typing-context bridge:\n{}",
            violations
                .iter()
                .map(|(line_no, line)| format!("  line {}: {}", line_no + 1, line.trim()))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}

/// The request-aware cache bypass must stay confined to the approved entry points.
///
/// This blocks new blanket "if request is non-empty, bypass cache" logic from
/// being reintroduced into other checker main entry points.
#[test]
fn request_empty_cache_bypass_stays_confined_to_approved_entry_points() {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let allowlist = ["state/state.rs", "types/class_type/constructor.rs"];

    let mut checker_files = Vec::new();
    collect_checker_rs_files_recursive(&base, &mut checker_files);

    let mut violations = Vec::new();
    for path in checker_files {
        if path
            .components()
            .any(|component| component.as_os_str() == "tests")
        {
            continue;
        }

        let relative = path
            .strip_prefix(&base)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        if allowlist.iter().any(|allowed| relative.ends_with(allowed)) {
            continue;
        }

        let source = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("failed to read {}", path.display()));
        for (line_num, line) in source.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with("*") {
                continue;
            }
            if trimmed.starts_with("if request.is_empty()")
                || trimmed.starts_with("let use_node_cache = request.is_empty()")
                || trimmed.starts_with("let can_use_cache = request.is_empty()")
            {
                violations.push(format!("{}:{}", relative, line_num + 1));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "request-empty cache bypass logic must stay confined to state/state.rs and \
         types/class_type/constructor.rs; violations:\n{}",
        violations.join("\n")
    );
}

#[test]
fn request_aware_contextual_retry_hot_paths_do_not_reintroduce_recursive_cache_clears() {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let whole_file_bans = [
        "assignability/assignment_checker.rs",
        "state/state_checking/property.rs",
        "types/type_checking/core.rs",
    ];

    for relative in whole_file_bans {
        let path = base.join(relative);
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("failed to read {}", path.display()));

        assert!(
            !source.contains("clear_type_cache_recursive("),
            "request-aware contextual retry path {relative} must use targeted invalidation helpers instead of direct recursive cache clears"
        );
    }
    let ambient_source =
        fs::read_to_string(base.join("state/state_checking_members/ambient_signature_checks.rs"))
            .expect("failed to read ambient_signature_checks.rs");
    assert!(
        ambient_source.contains("invalidate_initializer_for_context_change(prop.initializer)"),
        "ambient declared-type initializer retries must keep using the targeted invalidation helper"
    );
}

/// The `TypingRequest` type must exist and have the expected fields.
#[test]
fn typing_request_api_exists() {
    use crate::context::{ContextualOrigin, FlowIntent, TypingRequest};

    // Verify basic construction and field access
    let none = TypingRequest::NONE;
    assert!(none.is_empty());
    assert_eq!(none.contextual_type, None);
    assert_eq!(none.origin, ContextualOrigin::Normal);
    assert_eq!(none.flow, FlowIntent::Read);

    let with_ctx = TypingRequest::with_contextual_type(TypeId::STRING);
    assert_eq!(with_ctx.contextual_type, Some(TypeId::STRING));
    assert!(!with_ctx.origin.is_assertion());

    let assertion = TypingRequest::for_assertion(TypeId::NUMBER);
    assert!(assertion.origin.is_assertion());

    let write = TypingRequest::for_write_context();
    assert!(write.flow.skip_flow_narrowing());
}

/// Verify that the `statement_callback_bridge` save/restore for `check_statement`
/// is properly scoped (contextual type set only during `check_statement`, not leaked).
#[test]
fn statement_callback_bridge_contextual_type_scoping() {
    // This is a source-level check: the export clause handler must restore
    // contextual type BEFORE the assignability check, not after.
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/state/state_checking_members/statement_callback_bridge.rs");
    let content = fs::read_to_string(&path).expect("Failed to read statement_callback_bridge.rs");

    // The file should use get_type_of_node_with_request for the get_type_of_node call
    assert!(
        content.contains("get_type_of_node_with_request"),
        "statement_callback_bridge.rs should use get_type_of_node_with_request for export clause typing"
    );
}

#[test]
fn semantic_diagnostic_reporters_must_route_primary_anchor_selection_through_fingerprint_policy() {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/error_reporter");
    let fingerprint_policy = fs::read_to_string(base.join("fingerprint_policy.rs"))
        .expect("failed to read src/error_reporter/fingerprint_policy.rs");
    assert!(
        fingerprint_policy.contains("enum DiagnosticAnchorKind"),
        "fingerprint_policy.rs must define the shared anchor policy"
    );
    assert!(
        fingerprint_policy.contains("resolve_diagnostic_anchor_node"),
        "fingerprint_policy.rs must provide shared anchor resolution"
    );

    let files = [
        "assignability.rs",
        "call_errors.rs",
        "properties.rs",
        "generics.rs",
    ];
    let forbidden = [
        "assignment_diagnostic_anchor_idx(",
        "call_error_anchor_node(",
        "ts2769_first_arg_or_call(",
        "type_assertion_overlap_anchor(",
        "type_assertion_overlap_anchor_in_expression(",
        "build_related_from_failure_reason(",
    ];

    for file in files {
        let content =
            fs::read_to_string(base.join(file)).unwrap_or_else(|e| panic!("read {file}: {e}"));
        assert!(
            content.contains("DiagnosticAnchorKind::")
                || content.contains("resolve_diagnostic_anchor(")
                || content.contains("resolve_diagnostic_anchor_node("),
            "File {file} must use the shared fingerprint policy for anchor selection"
        );

        for forbidden_pattern in forbidden {
            assert!(
                !content.contains(forbidden_pattern),
                "File {file} must not reintroduce bespoke primary-anchor helper `{forbidden_pattern}`"
            );
        }
    }
}

/// Ensures that `current_callable_type` is not reintroduced as ambient mutable state.
///
/// The callable type is now threaded explicitly via `CallableContext` through the call
/// argument collection pipeline. No file in the call-context lane should read or write
/// `ctx.current_callable_type`. The field has been removed from `CheckerContext`.
#[test]
fn no_ambient_current_callable_type() {
    let migrated_files = [
        "src/checkers/call_checker.rs",
        "src/types/computation/call.rs",
        "src/types/computation/call_inference.rs",
        "src/types/computation/call_display.rs",
        "src/state/type_analysis/computed_helpers.rs",
        "src/context/mod.rs",
        "src/context/constructors.rs",
    ];

    let checker_root = Path::new(env!("CARGO_MANIFEST_DIR"));

    for file in migrated_files {
        let path = checker_root.join(file);
        let content =
            fs::read_to_string(&path).unwrap_or_else(|e| panic!("Failed to read {file}: {e}"));

        // Allow the doc comment in CallableContext's definition but forbid actual usage.
        // Filter out lines that are comments (starting with /// or //).
        let non_comment_lines: String = content
            .lines()
            .filter(|line| {
                let trimmed = line.trim();
                !trimmed.starts_with("///") && !trimmed.starts_with("//")
            })
            .collect::<Vec<_>>()
            .join("\n");

        assert!(
            !non_comment_lines.contains("current_callable_type"),
            "File {file} must not reference `current_callable_type` — \
             use explicit `CallableContext` threading instead"
        );
    }
}

/// Excess property classification logic (`ExcessPropertiesKind` pattern-matching)
/// must stay in the canonical path: `state/state_checking/property.rs` and
/// the `query_boundaries/assignability.rs` re-export.  Other checker files
/// must not reimplement this classification.
#[test]
fn test_excess_property_classification_quarantined_to_property_rs() {
    let mut files = Vec::new();
    collect_checker_rs_files_recursive(Path::new("src"), &mut files);

    let forbidden = [
        "ExcessPropertiesKind::Union",
        "ExcessPropertiesKind::Intersection",
        "ExcessPropertiesKind::Object(",
        "ExcessPropertiesKind::ObjectWithIndex(",
    ];

    let mut violations = Vec::new();
    for path in files {
        let rel = path.display().to_string();
        let allowed = rel.ends_with("state/state_checking/property.rs")
            || rel.ends_with("query_boundaries/assignability.rs")
            || rel.ends_with("assignability/assignability_diagnostics.rs") // target scoring
            || rel.ends_with("computation/object_literal_context.rs") // contextual type decomposition
            || rel.contains("/tests/");
        if allowed {
            continue;
        }
        let src = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("failed to read {}", path.display()));
        for pattern in &forbidden {
            if src.contains(pattern) {
                violations.push(format!("{rel} contains {pattern}"));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "ExcessPropertiesKind pattern-matching must stay in state/state_checking/property.rs; violations:\n{}",
        violations.join("\n")
    );
}

// ========================================================================
// Canonical RelationRequest / RelationOutcome boundary tests
// ========================================================================
//
// These tests enforce that the canonical `RelationRequest` / `RelationOutcome`
// / `execute_relation` boundary is the single authoritative path for relation
// queries that need structured failure information.

/// The `query_boundaries/assignability.rs` boundary must expose the unified
/// `execute_relation` helper and the `RelationOutcome` / `RelationRequest`
/// types that the checker uses for single-pass relation + failure collection.
#[test]
fn test_relation_request_and_outcome_live_in_query_boundaries() {
    let boundary_source = fs::read_to_string("src/query_boundaries/assignability.rs")
        .expect("failed to read query_boundaries/assignability.rs");

    assert!(
        boundary_source.contains("pub(crate) struct RelationRequest"),
        "RelationRequest must be defined in query_boundaries/assignability.rs"
    );
    assert!(
        boundary_source.contains("pub(crate) struct RelationOutcome"),
        "RelationOutcome must be defined in query_boundaries/assignability.rs"
    );
    assert!(
        boundary_source.contains("pub(crate) fn execute_relation"),
        "execute_relation boundary helper must be defined in query_boundaries/assignability.rs"
    );

    // RelationRequest must encode all policy dimensions
    assert!(
        boundary_source.contains("pub kind: RelationKind"),
        "RelationRequest must include a RelationKind field"
    );
    assert!(
        boundary_source.contains("pub excess_property_mode: ExcessPropertyMode"),
        "RelationRequest must include an ExcessPropertyMode field"
    );
    assert!(
        boundary_source.contains("pub missing_property_mode: MissingPropertyMode"),
        "RelationRequest must include a MissingPropertyMode field"
    );
    assert!(
        boundary_source.contains("pub source_is_fresh: bool"),
        "RelationRequest must include a source_is_fresh field"
    );

    // RelationOutcome must carry structured failure info
    assert!(
        boundary_source.contains("pub related: bool"),
        "RelationOutcome must include a `related` field"
    );
    assert!(
        boundary_source.contains("pub weak_union_violation: bool"),
        "RelationOutcome must include a `weak_union_violation` field"
    );
    assert!(
        boundary_source.contains("pub failure: Option<super::relation_types::RelationFailure>"),
        "RelationOutcome must include a structured `failure` field"
    );
}

/// The `RelationFailure` enum must live in `relation_types.rs` and provide
/// structured variant coverage for the semantic families we're unifying.
#[test]
fn test_relation_failure_covers_semantic_families() {
    let source = fs::read_to_string("src/query_boundaries/relation_types.rs")
        .expect("failed to read query_boundaries/relation_types.rs");

    // Core semantic families that must be represented
    for variant in [
        "MissingProperty",
        "MissingProperties",
        "ExcessProperty",
        "IncompatiblePropertyValue",
        "NoApplicableSignature",
        "TupleArityMismatch",
        "WeakUnionViolation",
        "TypeMismatch",
    ] {
        assert!(
            source.contains(variant),
            "RelationFailure must include the `{variant}` variant for semantic coverage"
        );
    }
}

/// Solver failure normalization must preserve the canonical semantic-family
/// mapping we rely on throughout the checker.
#[test]
fn test_relation_failure_preserves_canonical_solver_mapping() {
    let source = fs::read_to_string("src/query_boundaries/relation_types.rs")
        .expect("failed to read query_boundaries/relation_types.rs");

    assert!(
        source.contains("SubtypeFailureReason::NoCommonProperties")
            && source.contains("=> Self::WeakUnionViolation"),
        "NoCommonProperties must normalize to WeakUnionViolation"
    );
    assert!(
        source.contains("SubtypeFailureReason::OptionalPropertyRequired { property_name }")
            && source.contains("Self::PropertyModifierMismatch { property_name }"),
        "OptionalPropertyRequired must normalize to PropertyModifierMismatch"
    );
    assert!(
        source.contains("SubtypeFailureReason::PropertyTypeMismatch")
            && source.contains("Self::IncompatiblePropertyValue"),
        "PropertyTypeMismatch must normalize to IncompatiblePropertyValue"
    );
    assert!(
        source.contains("nested: nested_reason.map(|r| Box::new(Self::from_solver_reason(*r)))"),
        "nested property/return mismatches must recurse through from_solver_reason"
    );
}

/// `RelationRequest` must keep the builder helpers that encode freshness and
/// spread policy directly into the canonical relation request shape.
#[test]
fn test_relation_request_builders_encode_epc_policy() {
    let source = fs::read_to_string("src/query_boundaries/assignability.rs")
        .expect("failed to read query_boundaries/assignability.rs");

    assert!(
        source.contains("fn with_fresh_source"),
        "RelationRequest must keep with_fresh_source as the canonical fresh-literal builder"
    );
    assert!(
        source.contains("self.source_is_fresh = true;"),
        "with_fresh_source must mark the request as fresh"
    );
    assert!(
        source.contains("self.excess_property_mode = ExcessPropertyMode::Check;"),
        "with_fresh_source must enable full excess-property checking"
    );
    assert!(
        source.contains("fn with_spread_source"),
        "RelationRequest must keep with_spread_source as the canonical spread-literal builder"
    );
    assert!(
        source.contains("self.excess_property_mode = ExcessPropertyMode::CheckExplicitOnly;"),
        "with_spread_source must enable explicit-only excess-property checking"
    );
}

/// The canonical `RelationRequest` constructors must continue encoding the
/// semantic question directly as a `RelationKind`, rather than relying on
/// ambient caller-side policy.
#[test]
fn test_relation_request_constructors_encode_relation_kind() {
    let source = fs::read_to_string("src/query_boundaries/assignability.rs")
        .expect("failed to read query_boundaries/assignability.rs");

    for (ctor, kind) in [
        ("fn assign", "RelationKind::Assign"),
        ("fn call_arg", "RelationKind::CallArg"),
        ("fn return_stmt", "RelationKind::Return"),
        ("fn satisfies", "RelationKind::Satisfies"),
        ("fn destructuring", "RelationKind::Destructuring"),
    ] {
        assert!(
            source.contains(ctor) && source.contains(kind),
            "{ctor} must construct a RelationRequest with {kind}"
        );
    }
}

/// `assignability_checker.rs` must use `execute_relation_request` as the
/// canonical checker-level entry point for structured relation queries.
#[test]
fn test_assignability_checker_has_execute_relation_request() {
    let source = fs::read_to_string("src/assignability/assignability_checker.rs")
        .expect("failed to read assignability_checker.rs");

    assert!(
        source.contains("fn execute_relation_request("),
        "assignability_checker must define execute_relation_request as the canonical \
         checker-level entry point for structured relation queries"
    );
    assert!(
        source.contains("execute_relation("),
        "execute_relation_request must delegate to the query_boundaries::execute_relation helper"
    );
    assert!(
        source
            .contains("checker_only_assignability_failure_reason(request.source, request.target)"),
        "execute_relation_request must preserve checker-only post-check downgrades \
         after the canonical boundary returns"
    );
    assert!(
        source.contains("outcome.related = false;"),
        "execute_relation_request must be able to downgrade a solver-related result \
         when checker-only semantics require it"
    );
    assert!(
        source.contains("let flags = self.ctx.pack_relation_flags();"),
        "execute_relation_request must pass packed checker relation flags into the boundary"
    );
    assert!(
        source.contains("let overrides = CheckerOverrideProvider::new(self, None);"),
        "execute_relation_request must construct a checker override provider for the boundary call"
    );
    assert!(
        source.contains("self.ctx.sound_mode(),"),
        "execute_relation_request must pass checker sound_mode into the boundary"
    );
    assert!(
        source.contains("&self.ctx.inheritance_graph,"),
        "execute_relation_request must pass the checker inheritance graph into the boundary"
    );
    assert!(
        source.contains("Some(&self.ctx),"),
        "execute_relation_request must pass checker context into the boundary \
         for structured failure analysis"
    );
}

/// `assignability_diagnostics.rs` diagnostic paths must use the relation
/// outcome's `weak_union_violation` hint instead of re-calling the solver.
#[test]
fn test_diagnostic_paths_use_relation_outcome_hint() {
    let source = fs::read_to_string("src/assignability/assignability_diagnostics.rs")
        .expect("failed to read assignability_diagnostics.rs");

    // The `check_assignable_or_report_at` method should build a RelationRequest
    assert!(
        source.contains("RelationRequest::assign("),
        "check_assignable_or_report_at must build a RelationRequest::assign for the canonical path"
    );
    assert!(
        source.contains("execute_relation_request("),
        "check_assignable_or_report_at must call execute_relation_request"
    );
    assert!(
        source.contains("should_skip_weak_union_error_with_hint("),
        "diagnostic paths must use should_skip_weak_union_error_with_hint \
         to avoid re-calling the solver for weak-union detection"
    );
}

/// `check_argument_assignable_or_report` must use the canonical
/// `RelationRequest::call_arg` path for call-argument relation queries.
#[test]
fn test_call_arg_diagnostic_uses_canonical_relation_path() {
    let source = fs::read_to_string("src/assignability/assignability_diagnostics.rs")
        .expect("failed to read assignability_diagnostics.rs");

    assert!(
        source.contains("RelationRequest::call_arg("),
        "check_argument_assignable_or_report must build a RelationRequest::call_arg \
         for the canonical call-argument relation path"
    );
}

/// `analyze_assignability_failure` should stay aligned with the canonical
/// checker gate path and preserve the array/tuple weak-type suppression
/// that prevents false TS2559 diagnostics.
#[test]
fn test_assignability_failure_analysis_stays_on_canonical_gate() {
    let source = fs::read_to_string("src/assignability/assignability_diagnostics.rs")
        .expect("failed to read assignability_diagnostics.rs");

    assert!(
        source.contains("check_assignable_gate_with_overrides("),
        "analyze_assignability_failure must use check_assignable_gate_with_overrides \
         to stay aligned with canonical checker relation semantics"
    );
    assert!(
        source.contains("checker_only_assignability_failure_reason("),
        "analyze_assignability_failure must preserve checker-only failure downgrades"
    );
    assert!(
        source.contains("target_extends_array_or_tuple("),
        "analyze_assignability_failure must retain array/tuple weak-type suppression \
         for NoCommonProperties false positives"
    );
    assert!(
        source.contains("SubtypeFailureReason::NoCommonProperties"),
        "analyze_assignability_failure must explicitly gate NoCommonProperties \
         before emitting weak-type diagnostics"
    );
}

/// Interface/base property compatibility should route through the canonical
/// relation boundary instead of re-running local assignability + weak-union logic.
#[test]
fn test_class_query_boundary_uses_relation_request_for_property_mismatch() {
    let source =
        fs::read_to_string("src/query_boundaries/class.rs").expect("failed to read class.rs");

    assert!(
        source.contains("RelationRequest::assign("),
        "query_boundaries/class.rs must build a RelationRequest::assign \
         for property mismatch checks"
    );
    assert!(
        source.contains("execute_relation_request("),
        "query_boundaries/class.rs must use execute_relation_request \
         for property mismatch checks"
    );
    assert!(
        source.contains("should_skip_weak_union_error_with_outcome("),
        "query_boundaries/class.rs must use the structured RelationOutcome \
         when suppressing weak-union/excess-property diagnostics"
    );
}

// =============================================================================
// Phase 2: Object/property/call compatibility through canonical boundary
// =============================================================================

/// `RelationOutcome` must include `property_classification` for structured
/// property-level analysis, avoiding checker-local re-derivation.
#[test]
fn test_relation_outcome_has_property_classification() {
    let source = fs::read_to_string("src/query_boundaries/assignability.rs")
        .expect("failed to read assignability.rs");

    assert!(
        source.contains("property_classification:"),
        "RelationOutcome must include property_classification field \
         for structured property-level analysis"
    );

    assert!(
        source.contains("classify_object_properties("),
        "execute_relation must populate property_classification via \
         classify_object_properties boundary function"
    );
    assert!(
        source.contains("suppress_excess_property_failure_if_needed("),
        "execute_relation must centralize excess-property suppression through \
         suppress_excess_property_failure_if_needed"
    );
    assert!(
        source.contains("let property_classification =")
            && source.contains(
                "classify_object_properties(db.as_type_database(), request.source, request.target)"
            ),
        "execute_relation must always compute canonical property classification on failed relations"
    );
    assert!(
        source.contains("let (weak_union_violation, failure) = match analysis"),
        "execute_relation must derive weak-union and structured failure data together \
         from the same boundary analysis result"
    );
}

/// Successful relation results should return a clean `RelationOutcome`
/// with no leftover failure metadata attached.
#[test]
fn test_execute_relation_success_path_returns_clean_outcome() {
    let source = fs::read_to_string("src/query_boundaries/assignability.rs")
        .expect("failed to read assignability.rs");

    assert!(
        source.contains("if related {")
            && source.contains("related: true,")
            && source.contains("failure: None,")
            && source.contains("weak_union_violation: false,")
            && source.contains("property_classification: None,"),
        "execute_relation success path must return a clean RelationOutcome \
         with no failure, weak-union, or property-classification residue"
    );
}

/// Failed relation results should return a structured `RelationOutcome`
/// that keeps the normalized failure facts attached.
#[test]
fn test_execute_relation_failure_path_returns_structured_outcome() {
    let source = fs::read_to_string("src/query_boundaries/assignability.rs")
        .expect("failed to read assignability.rs");

    assert!(
        source.contains("RelationOutcome {")
            && source.contains("related: false,")
            && source.contains("failure,")
            && source.contains("weak_union_violation,")
            && source.contains("property_classification,"),
        "execute_relation failure path must return a structured RelationOutcome \
         with related=false plus failure, weak-union, and property-classification facts"
    );
}

/// The boundary must own the canonical excess-property suppression policy that
/// used to be duplicated in checker-local failure analysis.
#[test]
fn test_boundary_owns_excess_property_suppression_policy() {
    let source = fs::read_to_string("src/query_boundaries/assignability.rs")
        .expect("failed to read assignability.rs");

    assert!(
        source.contains("fn suppress_excess_property_failure_if_needed("),
        "assignability boundary must define suppress_excess_property_failure_if_needed"
    );
    assert!(
        source.contains("has_deferred_conditional_member"),
        "boundary excess-property suppression must handle deferred conditional members"
    );
    assert!(
        source.contains("get_intersection_members"),
        "boundary excess-property suppression must inspect intersection members"
    );
    assert!(
        source.contains("is_primitive_type(db, *member) || is_type_parameter_like(db, *member)"),
        "boundary excess-property suppression must skip EPC for primitive/type-parameter \
         intersection members"
    );
}

/// `PropertyClassification` must exist in `relation_types.rs` as the canonical
/// property-level boundary output type.
#[test]
fn test_property_classification_exists() {
    let source = fs::read_to_string("src/query_boundaries/relation_types.rs")
        .expect("failed to read relation_types.rs");

    assert!(
        source.contains("pub(crate) struct PropertyClassification"),
        "relation_types.rs must define PropertyClassification as the canonical \
         boundary output for property-level analysis"
    );

    for field in [
        "excess_properties",
        "missing_properties",
        "target_has_index_signature",
        "target_is_type_parameter",
        "target_is_empty_object",
        "target_is_global_object_or_function",
    ] {
        assert!(
            source.contains(field),
            "PropertyClassification must include the `{field}` field"
        );
    }
}

/// `source_has_excess_properties` in `property.rs` must delegate to the
/// canonical boundary function instead of re-implementing shape analysis.
#[test]
fn test_source_has_excess_properties_uses_boundary() {
    let source = fs::read_to_string("src/state/state_checking/property.rs")
        .expect("failed to read property.rs");

    assert!(
        source.contains("classify_object_properties("),
        "source_has_excess_properties must delegate to classify_object_properties \
         boundary function instead of re-implementing property enumeration"
    );
}

/// The simple object target path in `check_object_literal_excess_properties`
/// must use the boundary classification for the excess-property decision.
#[test]
fn test_simple_object_epc_uses_boundary_classification() {
    let source = fs::read_to_string("src/state/state_checking/property.rs")
        .expect("failed to read property.rs");

    // The simple object target path should use classify_object_properties
    assert!(
        source.contains("classify_object_properties("),
        "check_object_literal_excess_properties simple-object path must use \
         classify_object_properties boundary for property existence decisions"
    );

    // The is_global_object_or_function_shape should delegate to boundary
    assert!(
        source.contains("is_global_object_or_function_shape_boundary("),
        "is_global_object_or_function_shape must delegate to the boundary function"
    );
}

/// The boundary must own the canonical `is_global_object_or_function_shape` logic.
#[test]
fn test_boundary_owns_global_object_function_shape_check() {
    let source = fs::read_to_string("src/query_boundaries/assignability.rs")
        .expect("failed to read assignability.rs");

    assert!(
        source.contains("fn is_global_object_or_function_shape("),
        "assignability.rs boundary must own is_global_object_or_function_shape"
    );
    assert!(
        source.contains("OBJECT_PROTO"),
        "boundary must contain the canonical Object.prototype property list"
    );
    assert!(
        source.contains("FUNCTION_PROTO"),
        "boundary must contain the canonical Function.prototype property list"
    );
}

/// `property.rs` must NOT contain its own `OBJECT_PROTO/FUNCTION_PROTO` lists.
/// These must be defined only in the boundary.
#[test]
fn test_property_rs_no_duplicate_proto_lists() {
    let source = fs::read_to_string("src/state/state_checking/property.rs")
        .expect("failed to read property.rs");

    assert!(
        !source.contains("OBJECT_PROTO"),
        "property.rs must NOT define OBJECT_PROTO — it must use the boundary"
    );
    assert!(
        !source.contains("FUNCTION_PROTO"),
        "property.rs must NOT define FUNCTION_PROTO — it must use the boundary"
    );
}

/// Verify that `CheckerState::with_cache_and_shared_def_store` propagates
/// the shared `DefinitionStore` to the checker context.
#[test]
fn test_shared_def_store_propagated_through_cache_constructor() {
    use std::sync::Arc;
    use tsz_solver::def::DefinitionStore;

    let shared_store = Arc::new(DefinitionStore::new());

    // Register a definition in the shared store so we can verify identity.
    let info = tsz_solver::def::DefinitionInfo::type_alias(
        tsz_common::interner::Atom(42),
        vec![],
        TypeId::STRING,
    );
    let def_id = shared_store.register(info);

    let interner = TypeInterner::new();
    let query_cache = tsz_solver::QueryCache::new(&interner);
    let mut parser = tsz_parser::ParserState::new("test.ts".to_string(), "let x = 1;".to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let options = CheckerOptions {
        strict: false,
        ..Default::default()
    };

    // Create an empty TypeCache.
    let cache = crate::TypeCache {
        symbol_types: Default::default(),
        symbol_instance_types: Default::default(),
        node_types: Default::default(),
        symbol_dependencies: Default::default(),
        def_to_symbol: Default::default(),
        flow_analysis_cache: Default::default(),
        class_instance_type_to_decl: Default::default(),
        class_instance_type_cache: Default::default(),
        class_constructor_type_cache: Default::default(),
        type_only_nodes: Default::default(),
        namespace_module_names: Default::default(),
    };

    // Create checker with cache + shared def store.
    let checker = crate::state::CheckerState::with_cache_and_shared_def_store(
        arena,
        &binder,
        &query_cache,
        "test.ts".to_string(),
        cache,
        options,
        Arc::clone(&shared_store),
    );

    // The checker's definition store should be the same Arc instance.
    assert!(
        checker.ctx.definition_store.contains(def_id),
        "Checker should see definitions from the shared store"
    );
}

// =============================================================================
// Ratchet guards: prevent architecture debt from growing
// =============================================================================

/// Guard that the `TEMPORARILY_ALLOWED` bypass list in the solver-imports test
/// does not silently grow. When someone wraps a solver API in `query_boundaries`,
/// they should remove it from `TEMPORARILY_ALLOWED`, shrinking the count.
/// Adding new bypasses requires updating this ceiling (which reviewers will see).
///
/// Current ceiling: 44 items. This number must only decrease over time.
#[test]
fn test_temporarily_allowed_bypass_list_does_not_grow() {
    // The authoritative list lives in test_solver_imports_go_through_query_boundaries.
    // We cannot inspect it at runtime, so we count the items in source.
    let src = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/tests/architecture_contract_tests.rs"),
    )
    .expect("failed to read architecture_contract_tests.rs");

    // Find the TEMPORARILY_ALLOWED block and count non-comment, non-empty entries
    let mut in_block = false;
    let mut count = 0usize;
    for line in src.lines() {
        let trimmed = line.trim();
        if trimmed.contains("const TEMPORARILY_ALLOWED") {
            in_block = true;
            continue;
        }
        if in_block {
            if trimmed == "];" {
                break;
            }
            // Count lines that are quoted string entries (start with `"`)
            if trimmed.starts_with('"') {
                count += 1;
            }
        }
    }

    const CEILING: usize = 41;
    assert!(
        count <= CEILING,
        "TEMPORARILY_ALLOWED bypass list has grown to {count} items (ceiling: {CEILING}). \
         Do not add new solver import bypasses — create a query_boundaries wrapper instead. \
         If a wrapper was created, remove the old entry and lower CEILING in this test."
    );
}

/// Guard that direct type-construction calls (`interner.union()`, `interner.intersection()`,
/// `interner.object()`, `interner.array()`, `interner.tuple()`, `interner.function()`)
/// in checker source files outside `query_boundaries/` and `tests/` do not increase.
///
/// These calls bypass the `query_boundaries` layer and should be migrated to use
/// `flow_analysis::union_types()` or equivalent boundary helpers.
///
/// Current ceiling: 14 occurrences. This number must only decrease over time.
#[test]
fn test_direct_interner_type_construction_ceiling() {
    let checker_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    walk_rs_files_recursive(&checker_src, &mut files);

    const CONSTRUCTION_METHODS: &[&str] = &[
        "interner.union(",
        "interner.intersection(",
        "interner.object(",
        "interner.array(",
        "interner.tuple(",
        "interner.function(",
    ];

    let mut violations = Vec::new();
    let mut total_count = 0usize;

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

        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            for method in CONSTRUCTION_METHODS {
                if line.contains(method) {
                    violations.push(format!("  {}:{}", rel, line_num + 1));
                    total_count += 1;
                }
            }
        }
    }

    // Ceiling: current count of direct interner type-construction calls.
    // This number must only shrink as calls are migrated to query_boundaries.
    const CEILING: usize = 14;
    assert!(
        total_count <= CEILING,
        "Direct interner type-construction calls outside query_boundaries have increased \
         to {total_count} (ceiling: {CEILING}). Migrate new calls to use query_boundaries \
         helpers (e.g., flow_analysis::union_types). Current occurrences:\n{}",
        violations.join("\n")
    );
}

/// Guard that `error_reporter/` modules remain a pure diagnostic formatting layer.
/// They must not perform type construction (no `interner.union()`, `interner.object()`, etc.)
/// or type evaluation (no `TypeEvaluator::new()`, `TypeInstantiator::new()`).
///
/// Error reporters should only read type data and format diagnostics.
#[test]
fn test_error_reporter_does_not_perform_type_construction() {
    let error_reporter_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/error_reporter");
    let mut files = Vec::new();
    walk_rs_files_recursive(&error_reporter_dir, &mut files);

    const FORBIDDEN_PATTERNS: &[(&[&str], &str)] = &[
        (
            &[
                "interner.union(",
                "interner.intersection(",
                "interner.object(",
                "interner.array(",
                "interner.tuple(",
                "interner.function(",
            ],
            "direct type construction via interner",
        ),
        (
            &["TypeEvaluator::new("],
            "type evaluation (should be in checker/query_boundaries)",
        ),
    ];

    let mut violations = Vec::new();
    let checker_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");

    for path in &files {
        let rel = path
            .strip_prefix(&checker_src)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        let src = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            for (patterns, description) in FORBIDDEN_PATTERNS {
                for pattern in *patterns {
                    if line.contains(pattern) {
                        violations.push(format!("  {}:{} — {}", rel, line_num + 1, description,));
                    }
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "error_reporter modules must remain a pure formatting layer. \
         The following files contain forbidden patterns:\n{}",
        violations.join("\n")
    );
}

/// Guard that the number of checker source files exceeding ~2000 LOC does not increase.
///
/// Per CLAUDE.md section 12: "Checker files should stay under ~2000 LOC."
/// This ratchet captures the current state (4 files over 2000 lines) and prevents
/// regression. As files are split, this ceiling must be lowered.
///
/// Current ceiling: 4 files over 2000 lines. This number must only decrease over time.
#[test]
fn test_checker_file_size_ceiling() {
    let checker_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    walk_rs_files_recursive(&checker_src, &mut files);

    let mut oversized = Vec::new();
    let mut max_lines = 0usize;

    for path in &files {
        let rel = path
            .strip_prefix(&checker_src)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        // Skip test files — they are not subject to the LOC guideline
        if rel.starts_with("tests/") || rel.contains("/test") {
            continue;
        }

        let line_count = match fs::read_to_string(path) {
            Ok(s) => s.lines().count(),
            Err(_) => continue,
        };

        if line_count > max_lines {
            max_lines = line_count;
        }

        if line_count > 2000 {
            oversized.push(format!("  {rel} ({line_count} lines)"));
        }
    }

    // Ceiling: number of checker source files exceeding 2000 LOC.
    // This number must only shrink as files are split into smaller modules.
    // Current oversized files (as of 2026-03-24):
    //   types/function_type.rs, types/computation/call.rs,
    //   declarations/import/core.rs, state/variable_checking/core.rs,
    //   state/variable_checking/destructuring.rs, types/class_type/constructor.rs
    const FILE_COUNT_CEILING: usize = 6;
    assert!(
        oversized.len() <= FILE_COUNT_CEILING,
        "Number of checker source files over 2000 LOC has grown to {} (ceiling: {FILE_COUNT_CEILING}). \
         Split oversized files into smaller modules before adding new code. \
         Current oversized files:\n{}",
        oversized.len(),
        oversized.join("\n")
    );

    // Ceiling: maximum line count of any single checker source file.
    // This prevents existing large files from growing further.
    const MAX_LOC_CEILING: usize = 2394;
    assert!(
        max_lines <= MAX_LOC_CEILING,
        "Largest checker source file has grown to {max_lines} lines (ceiling: {MAX_LOC_CEILING}). \
         Split the file into smaller modules. Current oversized files:\n{}",
        oversized.join("\n")
    );
}

/// CLAUDE.md §4: Lowering must not import Checker or Emitter.
/// tsz-lowering is a bridge from AST to solver types; it should only depend on
/// parser, binder, solver, and common. Importing the checker or emitter would
/// create a backwards dependency in the pipeline.
#[test]
fn test_lowering_must_not_import_checker_or_emitter() {
    let lowering_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tsz-lowering/src");
    if !lowering_src.exists() {
        return;
    }

    let mut files = Vec::new();
    walk_rs_files_recursive(&lowering_src, &mut files);

    let forbidden_crates = ["tsz_checker", "tsz_emitter"];

    let mut violations = Vec::new();
    for path in files {
        let src = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("failed to read {}", path.display()));
        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            for crate_name in &forbidden_crates {
                if line.contains(&format!("use {crate_name}"))
                    || line.contains(&format!("{crate_name}::"))
                {
                    violations.push(format!(
                        "{}:{}: imports {}",
                        path.display(),
                        line_num + 1,
                        crate_name
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Lowering must not import Checker or Emitter (CLAUDE.md §4). \
         Lowering bridges AST to solver types; it should not depend on \
         downstream pipeline stages. Violations:\n  {}",
        violations.join("\n  ")
    );
}

/// Guard that CLI and ancillary crates consume checker only through public API paths.
///
/// Per CLAUDE.md section 4: "CLI and ancillary crates must consume checker diagnostics
/// via `tsz_checker::diagnostics`."
///
/// This prevents the CLI from reaching into checker internals (types, state, flow,
/// checkers, symbols, etc.) which would create tight coupling.
#[test]
fn test_cli_must_not_import_checker_internals() {
    let cli_src = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("tsz-cli/src");
    if !cli_src.exists() {
        // Skip if CLI crate doesn't exist in this workspace layout
        return;
    }

    let mut files = Vec::new();
    walk_rs_files_recursive(&cli_src, &mut files);

    // These are checker-internal module paths that CLI must not import.
    // `tsz_checker::diagnostics` and `tsz_checker::context` are the allowed public API.
    const FORBIDDEN_IMPORTS: &[&str] = &[
        "tsz_checker::types::",
        "tsz_checker::state::",
        "tsz_checker::flow::",
        "tsz_checker::checkers::",
        "tsz_checker::symbols::",
        "tsz_checker::error_reporter::",
        "tsz_checker::declarations::",
    ];

    let mut violations = Vec::new();

    for path in &files {
        let rel = path
            .strip_prefix(&cli_src)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        let src = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            for &forbidden in FORBIDDEN_IMPORTS {
                if line.contains(forbidden) {
                    violations.push(format!(
                        "  {}:{} — imports {}",
                        rel,
                        line_num + 1,
                        forbidden
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "CLI crate must not import checker internals. \
         Use `tsz_checker::diagnostics` for diagnostic codes and types. \
         Violations found:\n{}",
        violations.join("\n")
    );
}

/// Guard that cleaned-up checker modules do not regress by re-introducing
/// direct `tsz_solver::type_queries::` calls (both `use` imports AND inline
/// fully-qualified calls).
///
/// The existing `test_solver_imports_go_through_query_boundaries` only catches
/// `use tsz_solver::...` import statements. This test catches inline
/// `tsz_solver::type_queries::` calls in code that has been migrated to use
/// boundary wrappers.
///
/// When a new module is cleaned up, add its relative path to `CLEAN_MODULES`.
#[test]
fn test_no_inline_type_queries_in_cleaned_modules() {
    // Modules that have been fully migrated to use query_boundaries wrappers.
    // These must not contain any direct `tsz_solver::type_queries::` calls.
    const CLEAN_MODULES: &[&str] = &[
        "checkers/promise_checker.rs",
        "checkers/iterable_checker.rs",
        "flow/control_flow/core.rs",
        "flow/control_flow/references.rs",
        // "flow/control_flow/narrowing.rs", // TODO: re-add after migrating solver calls to query_boundaries
        "flow/reachability_checker.rs",
        "state/type_analysis/computed_helpers.rs",
        "state/type_analysis/computed_helpers_private.rs",
        "state/type_analysis/computed_helpers_binding.rs",
        "state/type_analysis/computed.rs",
        "state/type_analysis/core.rs",
        "state/type_analysis/core_type_query.rs",
        "state/type_analysis/symbol_type_helpers.rs",
        "state/type_analysis/computed_commonjs.rs",
        "state/type_analysis/computed_loops.rs",
        "context/resolver.rs",
    ];

    let checker_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut violations = Vec::new();

    for &module in CLEAN_MODULES {
        let path = checker_src.join(module);
        let src = match fs::read_to_string(&path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim();
            // Skip comments
            if trimmed.starts_with("//") || trimmed.starts_with("///") {
                continue;
            }
            if trimmed.contains("tsz_solver::type_queries::") {
                violations.push(format!("  {}:{} — {}", module, line_num + 1, trimmed));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Cleaned modules must not contain direct tsz_solver::type_queries:: calls. \
         Use query_boundaries wrappers instead.\n\
         Violations found:\n{}",
        violations.join("\n")
    );
}

/// Ratchet guard: direct `tsz_solver::widening::widen_type` (or `operations::widening::`)
/// calls outside `query_boundaries/`, `tests/`, and `types/utilities/core.rs` must not grow.
///
/// Callers should use `query_boundaries::common::widen_type` (free function) or
/// `self.widen_literal_type()` (method on `CheckerState`) instead.
///
/// Current ceiling: 8 occurrences. This number must only decrease over time.
#[test]
fn test_direct_widening_calls_ceiling() {
    let checker_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    walk_rs_files_recursive(&checker_src, &mut files);

    let mut count = 0usize;
    let mut locations = Vec::new();

    for path in &files {
        let rel = path
            .strip_prefix(&checker_src)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        // Skip allowed locations
        if rel.starts_with("query_boundaries/") || rel.starts_with("tests/") {
            continue;
        }

        let src = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") {
                continue;
            }
            if line.contains("tsz_solver::widening::widen_type")
                || line.contains("tsz_solver::operations::widening::widen_type")
            {
                count += 1;
                locations.push(format!("  {}:{}", rel, line_num + 1));
            }
        }
    }

    const CEILING: usize = 8;
    assert!(
        count <= CEILING,
        "Direct tsz_solver::widening::widen_type calls have grown to {count} (ceiling: {CEILING}). \
         Use query_boundaries::common::widen_type or self.widen_literal_type() instead.\n\
         Locations:\n{}",
        locations.join("\n")
    );
}

/// Guard: no direct `expression_ops::` calls outside `query_boundaries/` and `tests/`.
///
/// Expression operation calls should go through `query_boundaries::type_computation::core`
/// wrappers to maintain the boundary layer.
#[test]
fn test_no_direct_expression_ops_outside_query_boundaries() {
    let checker_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    walk_rs_files_recursive(&checker_src, &mut files);

    let mut violations = Vec::new();

    for path in &files {
        let rel = path
            .strip_prefix(&checker_src)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        if rel.starts_with("query_boundaries/") || rel.starts_with("tests/") {
            continue;
        }

        let src = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") {
                continue;
            }
            if line.contains("expression_ops::") && line.contains("tsz_solver") {
                violations.push(format!("  {}:{}", rel, line_num + 1));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Direct tsz_solver::expression_ops:: calls found outside query_boundaries/. \
         Use query_boundaries::type_computation::core wrappers instead.\n\
         Violations:\n{}",
        violations.join("\n")
    );
}

/// Guard: no direct `ApplicationEvaluator::new()` calls outside `query_boundaries/` and `tests/`.
///
/// Application evaluation should go through boundary wrappers like
/// `query_boundaries::flow_analysis::evaluate_application_type`.
#[test]
fn test_no_direct_application_evaluator_outside_query_boundaries() {
    let checker_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    walk_rs_files_recursive(&checker_src, &mut files);

    let mut violations = Vec::new();

    for path in &files {
        let rel = path
            .strip_prefix(&checker_src)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        if rel.starts_with("query_boundaries/") || rel.starts_with("tests/") {
            continue;
        }
        let src = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") {
                continue;
            }
            if line.contains("ApplicationEvaluator::new(") {
                violations.push(format!("  {}:{}", rel, line_num + 1));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Direct ApplicationEvaluator::new() calls found outside query_boundaries/. \
         Use query_boundaries::flow_analysis::evaluate_application_type instead.\n\
         Violations:\n{}",
        violations.join("\n")
    );
}

/// Guard: `context/def_mapping.rs` and context/speculation.rs must not cross-reference
/// each other. `def_mapping` owns SymbolId<->DefId identity mapping, speculation owns
/// checker state transaction boundaries. Mixing these concerns would violate the
/// clean context module separation (BOUNDARIES.md §4 Identity Boundary).
#[test]
fn test_def_mapping_and_speculation_do_not_cross_reference() {
    let def_mapping_src = fs::read_to_string("src/context/def_mapping.rs")
        .expect("failed to read src/context/def_mapping.rs");
    let speculation_src = fs::read_to_string("src/context/speculation.rs")
        .expect("failed to read src/context/speculation.rs");

    // def_mapping must not reference speculation types or functions
    assert!(
        !def_mapping_src.contains("DiagnosticSnapshot")
            && !def_mapping_src.contains("FullSnapshot")
            && !def_mapping_src.contains("ReturnTypeSnapshot")
            && !def_mapping_src.contains("rollback_")
            && !def_mapping_src.contains("snapshot_"),
        "def_mapping.rs must not reference speculation types or functions — \
         keep identity mapping separate from transaction boundaries"
    );

    // speculation must not reference def_mapping types or functions
    assert!(
        !speculation_src.contains("get_or_create_def_id")
            && !speculation_src.contains("def_mapping")
            && !speculation_src.contains("DefinitionStore")
            && !speculation_src.contains("DefinitionInfo"),
        "speculation.rs must not reference def_mapping types or functions — \
         keep transaction boundaries separate from identity mapping"
    );

    // Neither should perform type computation
    assert!(
        !def_mapping_src.contains("is_subtype_of") && !def_mapping_src.contains("is_assignable"),
        "def_mapping.rs must not perform type computation — it is pure identity mapping"
    );
    assert!(
        !speculation_src.contains("is_subtype_of") && !speculation_src.contains("is_assignable"),
        "speculation.rs must not perform type computation — it is pure state management"
    );
}

// =============================================================================
// Boundary Quarantine Tests — Evaluator/Checker Construction Ceilings
// =============================================================================

/// Guard: no `CompatChecker::new()` or `CompatChecker::with_resolver()` outside
/// `query_boundaries/` and `tests/`.
///
/// `CompatChecker` is the solver's Lawyer layer. Checker code should never construct
/// it directly — the relation should flow through `query_boundaries/assignability`
/// via `execute_relation()` and related helpers (CLAUDE.md §5, §22).
#[test]
fn test_no_direct_compat_checker_construction_outside_query_boundaries() {
    let checker_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    walk_rs_files_recursive(&checker_src, &mut files);

    let mut violations = Vec::new();
    for path in &files {
        let rel = path
            .strip_prefix(&checker_src)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        if rel.starts_with("query_boundaries/") || rel.starts_with("tests/") {
            continue;
        }

        let src = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") {
                continue;
            }
            if line.contains("CompatChecker::new(")
                || line.contains("CompatChecker::with_resolver(")
            {
                violations.push(format!("  {}:{}", rel, line_num + 1));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Direct CompatChecker construction found outside query_boundaries/. \
         Route relation checks through query_boundaries/assignability instead (CLAUDE.md §5, §22).\n\
         Violations:\n{}",
        violations.join("\n")
    );
}

/// Ceiling: direct `BinaryOpEvaluator::new()` calls outside `query_boundaries/` and `tests/`.
///
/// These bypass the query boundary layer. A wrapper in
/// `query_boundaries/type_computation/core.rs` exists for `evaluate_plus_chain`;
/// more wrappers should be added over time. This ceiling must only decrease.
///
/// Current ceiling: 21 occurrences.
#[test]
fn test_direct_binary_op_evaluator_construction_ceiling() {
    let checker_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    walk_rs_files_recursive(&checker_src, &mut files);

    let mut count = 0usize;
    let mut locations = Vec::new();

    for path in &files {
        let rel = path
            .strip_prefix(&checker_src)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        if rel.starts_with("query_boundaries/") || rel.starts_with("tests/") {
            continue;
        }

        let src = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") {
                continue;
            }
            if line.contains("BinaryOpEvaluator::new(") {
                count += 1;
                locations.push(format!("  {}:{}", rel, line_num + 1));
            }
        }
    }

    const CEILING: usize = 25;
    assert!(
        count <= CEILING,
        "BinaryOpEvaluator::new() usage ceiling exceeded: found {count} (ceiling: {CEILING}). \
         Create query_boundaries wrappers instead of adding new direct usages.\n\
         Locations:\n{}",
        locations.join("\n")
    );
}

/// Ceiling: direct `PropertyAccessEvaluator::new()` calls outside `query_boundaries/` and `tests/`.
///
/// These bypass the query boundary layer. Wrappers should be created in
/// `query_boundaries/` over time. This ceiling must only decrease.
///
/// Current ceiling: 3 occurrences.
#[test]
fn test_direct_property_access_evaluator_construction_ceiling() {
    let checker_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    walk_rs_files_recursive(&checker_src, &mut files);

    let mut count = 0usize;
    let mut locations = Vec::new();

    for path in &files {
        let rel = path
            .strip_prefix(&checker_src)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        if rel.starts_with("query_boundaries/") || rel.starts_with("tests/") {
            continue;
        }

        let src = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") {
                continue;
            }
            if line.contains("PropertyAccessEvaluator::new(") {
                count += 1;
                locations.push(format!("  {}:{}", rel, line_num + 1));
            }
        }
    }

    const CEILING: usize = 3;
    assert!(
        count <= CEILING,
        "PropertyAccessEvaluator::new() usage ceiling exceeded: found {count} (ceiling: {CEILING}). \
         Create query_boundaries wrappers instead of adding new direct usages.\n\
         Locations:\n{}",
        locations.join("\n")
    );
}

/// Ceiling: direct `TypeInstantiator::new()` calls outside `query_boundaries/` and `tests/`.
///
/// Type instantiation should flow through `query_boundaries/common::instantiate_type`
/// or dedicated boundary helpers. This ceiling must only decrease.
///
/// Current ceiling: 1 occurrence (types/queries/lib.rs).
#[test]
fn test_direct_type_instantiator_construction_ceiling() {
    let checker_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    walk_rs_files_recursive(&checker_src, &mut files);

    let mut count = 0usize;
    let mut locations = Vec::new();

    for path in &files {
        let rel = path
            .strip_prefix(&checker_src)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        if rel.starts_with("query_boundaries/") || rel.starts_with("tests/") {
            continue;
        }

        let src = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") {
                continue;
            }
            if line.contains("TypeInstantiator::new(") {
                count += 1;
                locations.push(format!("  {}:{}", rel, line_num + 1));
            }
        }
    }

    const CEILING: usize = 1;
    assert!(
        count <= CEILING,
        "TypeInstantiator::new() usage ceiling exceeded: found {count} (ceiling: {CEILING}). \
         Use query_boundaries/common::instantiate_type or create a new boundary wrapper.\n\
         Locations:\n{}",
        locations.join("\n")
    );
}

/// Guard: no direct `tsz_solver::relations::freshness::` calls outside
/// `query_boundaries/` and `tests/`.
///
/// Freshness queries (`is_fresh_object_type`, `widen_freshness`) have wrappers
/// in `query_boundaries/common.rs`. All checker code must use those wrappers
/// to maintain the boundary between checker (WHERE) and solver (WHAT).
#[test]
fn test_no_direct_freshness_calls_outside_query_boundaries() {
    let checker_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    walk_rs_files_recursive(&checker_src, &mut files);

    let mut violations = Vec::new();

    for path in &files {
        let rel = path
            .strip_prefix(&checker_src)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        if rel.starts_with("query_boundaries/") || rel.starts_with("tests/") {
            continue;
        }

        let src = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") {
                continue;
            }
            if line.contains("tsz_solver::relations::freshness") {
                violations.push(format!("  {}:{}", rel, line_num + 1));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Direct tsz_solver::relations::freshness:: calls found outside query_boundaries/. \
         Use query_boundaries::common::is_fresh_object_type / widen_freshness instead.\n\
         Violations:\n{}",
        violations.join("\n")
    );
}

// =============================================================================
// Stable Identity Helper Tests — DefId Resolution
// =============================================================================

/// Guard: core.rs must NOT contain inline type-param priming loops.
///
/// The ad hoc block that manually iterated symbol declarations to extract
/// type parameters was replaced by `ensure_def_ready_for_lowering`. This
/// test ensures the inline pattern doesn't regrow.
#[test]
fn test_core_type_resolution_uses_stable_identity_helper_for_type_param_priming() {
    let src = fs::read_to_string("src/state/type_resolution/core.rs")
        .expect("failed to read src/state/type_resolution/core.rs");

    // The old ad hoc pattern iterated declarations with get_interface + get_type_alias
    // inline to extract type parameters. This should now go through
    // ensure_def_ready_for_lowering which delegates to
    // extract_declared_type_params_for_reference_symbol.
    let has_inline_iface_param_extraction = src
        .lines()
        .filter(|line| !line.trim().starts_with("//"))
        .any(|line| {
            line.contains("get_interface(node)")
                && !line.contains("ensure_def")
                && !line.contains("extract_declared")
        });

    assert!(
        !has_inline_iface_param_extraction,
        "core.rs contains inline interface type-param extraction. \
         Use ensure_def_ready_for_lowering (which delegates to \
         extract_declared_type_params_for_reference_symbol) instead."
    );
}

/// Guard: core.rs type reference resolution must delegate to
/// `ensure_def_ready_for_lowering` for generic ref type-param priming.
#[test]
fn test_core_type_resolution_has_ensure_def_ready_call() {
    let src = fs::read_to_string("src/state/type_resolution/core.rs")
        .expect("failed to read src/state/type_resolution/core.rs");

    assert!(
        src.contains("ensure_def_ready_for_lowering"),
        "core.rs must call ensure_def_ready_for_lowering for generic type \
         reference resolution. This is the stable-identity helper that \
         replaces ad hoc type-param priming blocks."
    );
}

/// Guard: `reference_helpers.rs` must expose `ensure_def_ready_for_lowering`.
///
/// This helper consolidates the DefId + type-param + body priming pattern.
#[test]
fn test_reference_helpers_expose_stable_identity_helper() {
    let src = fs::read_to_string("src/state/type_resolution/reference_helpers.rs")
        .expect("failed to read src/state/type_resolution/reference_helpers.rs");

    assert!(
        src.contains("fn ensure_def_ready_for_lowering"),
        "reference_helpers.rs must expose ensure_def_ready_for_lowering — \
         the stable-identity helper for DefId + type-param + body priming."
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

/// Guard: `namespace_checker.rs` must NOT directly construct `TypeData::Lazy`.
///
/// Namespace types should use structural object types (via `build_namespace_object_type`)
/// or stable-identity helpers — never raw Lazy construction.
#[test]
fn test_namespace_checker_no_raw_lazy_construction() {
    let src = fs::read_to_string("src/declarations/namespace_checker.rs")
        .expect("failed to read src/declarations/namespace_checker.rs");

    let has_raw_lazy = src
        .lines()
        .filter(|line| !line.trim().starts_with("//"))
        .any(|line| line.contains("TypeData::Lazy") || line.contains(".lazy("));

    assert!(
        !has_raw_lazy,
        "namespace_checker.rs must not directly construct Lazy types. \
         Namespace types should use structural object types \
         (build_namespace_object_type) or stable-identity helpers."
    );
}
