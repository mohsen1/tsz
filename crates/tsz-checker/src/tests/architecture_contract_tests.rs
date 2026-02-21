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
        visibility: Visibility::Public,
        parent_id: None,
    }]);

    let dog = interner.object(vec![
        tsz_solver::PropertyInfo {
            name: animal_name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
            visibility: Visibility::Public,
            parent_id: None,
        },
        tsz_solver::PropertyInfo {
            name: dog_breed,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
            visibility: Visibility::Public,
            parent_id: None,
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

    let type_literal_src = fs::read_to_string("src/type_literal_checker.rs")
        .expect("failed to read src/type_literal_checker.rs for architecture guard");
    assert!(
        !type_literal_src.contains("TypeData::ReadonlyType"),
        "type_literal_checker should use solver readonly constructor APIs, not TypeData::ReadonlyType"
    );

    let mut type_resolution_src = fs::read_to_string("src/state_type_resolution.rs")
        .expect("failed to read src/state_type_resolution.rs for architecture guard");
    // Include split-off module that is part of the state_type_resolution logical module
    type_resolution_src.push_str(
        &fs::read_to_string("src/state_type_resolution_module.rs")
            .expect("failed to read src/state_type_resolution_module.rs"),
    );
    assert!(
        !type_resolution_src.contains("TypeData::ReadonlyType"),
        "state_type_resolution should use solver readonly constructor APIs, not TypeData::ReadonlyType"
    );
    assert!(
        !type_resolution_src.contains("intern(tsz_solver::TypeData::Lazy("),
        "state_type_resolution should use solver lazy constructor API, not direct TypeData::Lazy interning"
    );

    let type_node_src =
        fs::read_to_string("src/type_node.rs").expect("failed to read src/type_node.rs");
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

    let jsx_checker_src =
        fs::read_to_string("src/jsx_checker.rs").expect("failed to read src/jsx_checker.rs");
    assert!(
        !jsx_checker_src.contains("TypeData::IndexAccess"),
        "jsx_checker should use solver index_access constructor API, not TypeData::IndexAccess"
    );

    let mut context_src = fs::read_to_string("src/context.rs")
        .expect("failed to read src/context.rs for architecture guard");
    // Include split-off modules that are part of the context logical module
    context_src.push_str(
        &fs::read_to_string("src/context_constructors.rs")
            .expect("failed to read src/context_constructors.rs for architecture guard"),
    );
    context_src.push_str(
        &fs::read_to_string("src/context_resolver.rs")
            .expect("failed to read src/context_resolver.rs for architecture guard"),
    );
    assert!(
        !context_src.contains("self.types.intern(TypeData::Lazy("),
        "context should use solver lazy constructor API, not direct TypeData::Lazy interning"
    );

    let queries_src = fs::read_to_string("src/type_checking_queries.rs")
        .expect("failed to read src/type_checking_queries.rs for architecture guard");
    assert!(
        !queries_src.contains("self.ctx.types.intern(TypeData::Lazy("),
        "type_checking_queries should use solver lazy constructor API, not direct TypeData::Lazy interning"
    );
    assert!(
        !queries_src.contains("self.ctx.types.intern(TypeData::TypeParameter("),
        "type_checking_queries should use solver type_param constructor API, not direct TypeData::TypeParameter interning"
    );

    let state_checking_members_src = fs::read_to_string("src/state_checking_members.rs")
        .expect("failed to read src/state_checking_members.rs for architecture guard");
    assert!(
        !state_checking_members_src.contains("TypeData::TypeParameter"),
        "state_checking_members should use solver type_param constructor API, not TypeData::TypeParameter"
    );

    let control_flow_narrowing_src = fs::read_to_string("src/control_flow_narrowing.rs")
        .expect("failed to read src/control_flow_narrowing.rs for architecture guard");
    assert!(
        !control_flow_narrowing_src.contains("intern(TypeData::Lazy("),
        "control_flow_narrowing should use solver lazy constructor API, not direct TypeData::Lazy interning"
    );

    let mut state_type_analysis_src = fs::read_to_string("src/state_type_analysis.rs")
        .expect("failed to read src/state_type_analysis.rs for architecture guard");
    // Include split-off modules that are part of the state_type_analysis logical module
    state_type_analysis_src.push_str(
        &fs::read_to_string("src/state_type_analysis_computed.rs")
            .expect("failed to read src/state_type_analysis_computed.rs for architecture guard"),
    );
    state_type_analysis_src.push_str(
        &fs::read_to_string("src/state_type_analysis_computed_helpers.rs").expect(
            "failed to read src/state_type_analysis_computed_helpers.rs for architecture guard",
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

    let function_type_src = fs::read_to_string("src/function_type.rs")
        .expect("failed to read src/function_type.rs for architecture guard");
    assert!(
        !function_type_src.contains("intern(TypeData::TypeParameter("),
        "function_type should use solver type_param constructor API, not TypeData::TypeParameter"
    );

    let assignability_checker_src = fs::read_to_string("src/assignability_checker.rs")
        .expect("failed to read src/assignability_checker.rs for architecture guard");
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

    let mut state_type_environment_src = fs::read_to_string("src/state_type_environment.rs")
        .expect("failed to read src/state_type_environment.rs for architecture guard");
    // Include split-off module that is part of the state_type_environment logical module
    state_type_environment_src.push_str(
        &fs::read_to_string("src/state_type_environment_lazy.rs")
            .expect("failed to read src/state_type_environment_lazy.rs for architecture guard"),
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
        !state_type_environment_src.contains("SymbolResolutionTraversalKind::"),
        "state_type_environment should use solver visitor traversal helpers, not checker-side SymbolResolutionTraversalKind branching"
    );
    assert!(
        !state_type_environment_src.contains("classify_for_symbol_resolution_traversal("),
        "state_type_environment should not classify traversal in checker; use solver visitor APIs instead"
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

    let type_computation_complex_src = fs::read_to_string("src/type_computation_complex.rs")
        .expect("failed to read src/type_computation_complex.rs for architecture guard");
    assert!(
        !type_computation_complex_src.contains("intern(tsz_solver::TypeData::TypeParameter("),
        "type_computation_complex should use solver type_param constructor API, not direct TypeData::TypeParameter interning"
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
    let source = fs::read_to_string("src/assignability_checker.rs")
        .expect("failed to read src/assignability_checker.rs for architecture guard");

    assert!(
        !source.contains("query_relation_with_overrides("),
        "assignability_checker should route compatibility checks through query_boundaries/assignability helpers"
    );
    assert!(
        !source.contains("query_relation_with_resolver("),
        "assignability_checker should route subtype/redecl checks through query_boundaries/assignability helpers"
    );
    assert!(
        source.contains("is_assignable_with_overrides("),
        "assignability_checker should use query_boundaries::assignability::is_assignable_with_overrides"
    );
    assert!(
        source.contains("is_assignable_with_resolver("),
        "assignability_checker should use query_boundaries::assignability::is_assignable_with_resolver"
    );
    assert!(
        source.contains("is_assignable_bivariant_with_resolver("),
        "assignability_checker should use query_boundaries::assignability::is_assignable_bivariant_with_resolver"
    );
    assert!(
        source.contains("is_subtype_with_resolver("),
        "assignability_checker should use query_boundaries::assignability::is_subtype_with_resolver"
    );
    assert!(
        source.contains("is_redeclaration_identical_with_resolver("),
        "assignability_checker should use query_boundaries::assignability::is_redeclaration_identical_with_resolver"
    );
}

#[test]
fn test_subtype_path_establishes_preconditions_before_subtype_cache_lookup() {
    let source = fs::read_to_string("src/assignability_checker.rs")
        .expect("failed to read src/assignability_checker.rs for architecture guard");

    let subtype_start = source
        .find("pub fn is_subtype_of(")
        .expect("missing is_subtype_of in assignability_checker");
    let subtype_end = source[subtype_start..]
        .find("pub fn is_subtype_of_with_env(")
        .map(|offset| subtype_start + offset)
        .expect("missing is_subtype_of_with_env in assignability_checker");
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

    let with_env_src = &source[subtype_end..];
    assert!(
        with_env_src.contains("self.ensure_relation_input_ready(source);")
            && with_env_src.contains("self.ensure_relation_input_ready(target);"),
        "is_subtype_of_with_env should establish centralized relation preconditions for both sides"
    );
}

#[test]
fn test_assignment_and_binding_default_assignability_use_central_gateway_helpers() {
    let assignment_checker_src = fs::read_to_string("src/assignment_checker.rs")
        .expect("failed to read src/assignment_checker.rs for architecture guard");
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

    let type_checking_src = fs::read_to_string("src/type_checking.rs")
        .expect("failed to read src/type_checking.rs for architecture guard");
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

    let parameter_checker_src = fs::read_to_string("src/parameter_checker.rs")
        .expect("failed to read src/parameter_checker.rs for architecture guard");
    assert!(
        parameter_checker_src.contains("check_assignable_or_report("),
        "parameter initializer assignability should route through check_assignable_or_report"
    );

    let control_flow_assignment_src = fs::read_to_string("src/control_flow_assignment.rs")
        .expect("failed to read src/control_flow_assignment.rs for architecture guard");
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
        control_flow_assignment_src.contains("are_types_mutually_subtype("),
        "control-flow assignment subtype compatibility checks should route through flow_analysis boundary helpers"
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
    let control_flow_src = fs::read_to_string("src/control_flow.rs")
        .expect("failed to read src/control_flow.rs for architecture guard");
    assert!(
        control_flow_src.contains("query::is_assignable_with_env("),
        "FlowAnalyzer assignability should route through flow_analysis boundary helpers"
    );
    assert!(
        control_flow_src.contains("query::is_assignable_strict_null("),
        "FlowAnalyzer strict-null assignability should route through flow_analysis boundary helpers"
    );
    let flow_analysis_definite_src = fs::read_to_string("src/flow_analysis_definite.rs")
        .expect("failed to read src/flow_analysis_definite.rs for architecture guard");
    assert!(
        flow_analysis_definite_src.contains("find_property_in_object_by_str("),
        "flow_analysis_definite property lookup should route through definite_assignment query boundaries"
    );
    assert!(
        !flow_analysis_definite_src.contains("tsz_solver::type_queries::"),
        "flow_analysis_definite should not call solver type_queries directly; use definite_assignment/flow_analysis query boundaries"
    );

    let mut state_type_resolution_src = fs::read_to_string("src/state_type_resolution.rs")
        .expect("failed to read src/state_type_resolution.rs for architecture guard");
    // Include split-off module that is part of the state_type_resolution logical module
    state_type_resolution_src.push_str(
        &fs::read_to_string("src/state_type_resolution_module.rs")
            .expect("failed to read src/state_type_resolution_module.rs"),
    );
    assert!(
        state_type_resolution_src.contains("ensure_relation_input_ready("),
        "state_type_resolution relation precondition setup should route through ensure_relation_input_ready"
    );

    let mut state_checking_src = fs::read_to_string("src/state_checking.rs")
        .expect("failed to read src/state_checking.rs for architecture guard");
    // Include split-off modules that are part of the state_checking logical module
    state_checking_src.push_str(
        &fs::read_to_string("src/state_variable_checking.rs")
            .expect("failed to read src/state_variable_checking.rs for architecture guard"),
    );
    state_checking_src.push_str(
        &fs::read_to_string("src/state_property_checking.rs")
            .expect("failed to read src/state_property_checking.rs for architecture guard"),
    );
    state_checking_src.push_str(
        &fs::read_to_string("src/state_variable_checking_destructuring.rs").expect(
            "failed to read src/state_variable_checking_destructuring.rs for architecture guard",
        ),
    );
    state_checking_src.push_str(
        &fs::read_to_string("src/state_class_checking.rs")
            .expect("failed to read src/state_class_checking.rs for architecture guard"),
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
    let state_property_checking_src = fs::read_to_string("src/state_property_checking.rs")
        .expect("failed to read src/state_property_checking.rs for architecture guard");
    assert!(
        !state_property_checking_src.contains("self.ctx.types.is_subtype_of("),
        "state_property_checking subtype checks should route through checker gateway helpers, not direct interner calls"
    );
    assert!(
        !state_property_checking_src.contains("tsz_solver::type_queries::"),
        "state_property_checking should route solver type-query access through query_boundaries::state_checking"
    );
    let state_variable_checking_destructuring_src = fs::read_to_string(
        "src/state_variable_checking_destructuring.rs",
    )
    .expect("failed to read src/state_variable_checking_destructuring.rs for architecture guard");
    let state_variable_checking_src = fs::read_to_string("src/state_variable_checking.rs")
        .expect("failed to read src/state_variable_checking.rs for architecture guard");
    let state_class_checking_src = fs::read_to_string("src/state_class_checking.rs")
        .expect("failed to read src/state_class_checking.rs for architecture guard");
    let property_access_type_src = fs::read_to_string("src/property_access_type.rs")
        .expect("failed to read src/property_access_type.rs for architecture guard");
    let property_checker_src = fs::read_to_string("src/property_checker.rs")
        .expect("failed to read src/property_checker.rs for architecture guard");
    assert!(
        state_variable_checking_src.contains("query::array_element_type("),
        "state_variable_checking array element checks should route through query_boundaries::state_checking"
    );
    assert!(
        state_variable_checking_src.contains("query::is_only_null_or_undefined("),
        "state_variable_checking null/undefined checks should route through query_boundaries::state_checking"
    );
    assert!(
        state_variable_checking_src.contains("query::has_type_query_for_symbol("),
        "state_variable_checking symbol type-query checks should route through query_boundaries::state_checking"
    );
    assert!(
        !state_variable_checking_src.contains("tsz_solver::type_queries::"),
        "state_variable_checking should not call solver type_queries directly; use state_checking query boundaries"
    );
    assert!(
        state_variable_checking_destructuring_src.contains("query::is_only_null_or_undefined("),
        "state_variable_checking_destructuring null/undefined checks should route through query_boundaries::state_checking"
    );
    assert!(
        !state_variable_checking_destructuring_src.contains("tsz_solver::type_queries::"),
        "state_variable_checking_destructuring should not call solver type_queries directly; use state_checking query boundaries"
    );
    assert!(
        state_class_checking_src.contains("class_query::construct_signatures_for_type("),
        "state_class_checking constructor signature checks should route through query_boundaries::class_type"
    );
    assert!(
        state_class_checking_src.contains("class_query::is_mapped_type("),
        "state_class_checking mapped-type checks should route through query_boundaries::class_type"
    );
    assert!(
        state_class_checking_src.contains("class_query::is_generic_type("),
        "state_class_checking generic-type checks should route through query_boundaries::class_type"
    );
    assert!(
        state_class_checking_src.contains("class_query::type_includes_undefined("),
        "state_class_checking undefined-inclusion checks should route through query_boundaries::class_type"
    );
    assert!(
        !state_class_checking_src.contains("tsz_solver::type_queries::"),
        "state_class_checking should not call solver type_queries directly; use class_type query boundaries"
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
        !property_access_type_src.contains("tsz_solver::type_queries_classifiers::"),
        "property_access_type should not call solver type_queries_classifiers directly; use property_access query boundaries"
    );
    assert!(
        property_checker_src.contains("query::is_type_usable_as_property_name("),
        "property_checker computed-name checks should route through query_boundaries::property_checker"
    );
    assert!(
        !property_checker_src.contains("tsz_solver::type_queries::"),
        "property_checker should not call solver type_queries directly; use property_checker query boundaries"
    );
    let assignability_checker_src = fs::read_to_string("src/assignability_checker.rs")
        .expect("failed to read src/assignability_checker.rs for architecture guard");
    assert!(
        !assignability_checker_src.contains("self.ctx.types.is_subtype_of("),
        "assignability_checker subtype checks should route through checker/solver query gateways, not direct interner calls"
    );

    let mut state_checking_members_src = fs::read_to_string("src/state_checking_members.rs")
        .expect("failed to read src/state_checking_members.rs for architecture guard");
    state_checking_members_src.push_str(
        &fs::read_to_string("src/state_checking_members/ambient_signature_checks.rs")
            .expect("failed to read ambient_signature_checks.rs for architecture guard"),
    );
    state_checking_members_src.push_str(
        &fs::read_to_string("src/state_checking_members/member_access.rs")
            .expect("failed to read member_access.rs for architecture guard"),
    );
    state_checking_members_src.push_str(
        &fs::read_to_string("src/state_checking_members/member_declaration_checks.rs")
            .expect("failed to read member_declaration_checks.rs for architecture guard"),
    );
    state_checking_members_src.push_str(
        &fs::read_to_string("src/state_checking_members/statement_callback_bridge.rs")
            .expect("failed to read statement_callback_bridge.rs for architecture guard"),
    );
    state_checking_members_src.push_str(
        &fs::read_to_string("src/state_checking_members/statement_checks.rs")
            .expect("failed to read statement_checks.rs for architecture guard"),
    );
    assert!(
        state_checking_members_src.contains("check_assignable_or_report("),
        "state_checking_members assignment-style checks should route through check_assignable_or_report"
    );

    let type_computation_src = fs::read_to_string("src/type_computation.rs")
        .expect("failed to read src/type_computation.rs for architecture guard");
    assert!(
        type_computation_src.contains("check_assignable_or_report("),
        "type_computation mismatch checks should route through check_assignable_or_report"
    );

    let type_computation_complex_src = fs::read_to_string("src/type_computation_complex.rs")
        .expect("failed to read src/type_computation_complex.rs for architecture guard");
    assert!(
        type_computation_complex_src.contains("check_argument_assignable_or_report("),
        "type_computation_complex argument mismatch checks should route through check_argument_assignable_or_report"
    );
    assert!(
        type_computation_complex_src.contains("check_assignable_or_report_generic_at("),
        "type_computation_complex callback constraint mismatch checks should route through check_assignable_or_report_generic_at"
    );
    assert!(
        type_computation_complex_src.contains("ensure_relation_input_ready(")
            && type_computation_complex_src.contains("ensure_relation_inputs_ready("),
        "type_computation_complex should route relation precondition setup through centralized ensure_relation_input(s)_ready helpers"
    );
    assert!(
        !type_computation_complex_src.contains("ensure_application_symbols_resolved("),
        "type_computation_complex should not manually orchestrate application-symbol preconditions; use centralized relation precondition helpers"
    );
    let type_computation_access_src = fs::read_to_string("src/type_computation_access.rs")
        .expect("failed to read src/type_computation_access.rs for architecture guard");
    assert!(
        type_computation_access_src.contains("query_boundaries::type_computation_access::"),
        "type_computation_access solver queries should route through query_boundaries::type_computation_access"
    );
    assert!(
        !type_computation_access_src
            .contains("tsz_solver::type_queries::get_literal_property_name("),
        "type_computation_access should not call get_literal_property_name directly; use type_computation_access query boundaries"
    );
    assert!(
        !type_computation_access_src.contains("tsz_solver::type_queries::get_tuple_elements("),
        "type_computation_access should not call get_tuple_elements directly; use type_computation_access query boundaries"
    );
    assert!(
        !type_computation_access_src.contains("tsz_solver::type_queries::is_valid_spread_type("),
        "type_computation_access should not call is_valid_spread_type directly; use type_computation_access query boundaries"
    );

    let dispatch_src =
        fs::read_to_string("src/dispatch.rs").expect("failed to read src/dispatch.rs for guard");
    assert!(
        dispatch_src.contains("check_assignable_or_report("),
        "dispatch mismatch checks should route through check_assignable_or_report"
    );
    assert!(
        dispatch_src.contains("ensure_relation_input_ready("),
        "dispatch relation precondition setup should route through ensure_relation_input_ready"
    );
    assert!(
        !dispatch_src.contains("ensure_application_symbols_resolved("),
        "dispatch should not manually orchestrate application-symbol preconditions"
    );

    let class_checker_src = fs::read_to_string("src/class_checker.rs")
        .expect("failed to read src/class_checker.rs for architecture guard");
    assert!(
        class_checker_src.contains("should_report_member_type_mismatch(")
            && class_checker_src.contains("should_report_member_type_mismatch_bivariant("),
        "class member compatibility should use centralized class query-boundary mismatch helpers"
    );

    let error_handler_src = fs::read_to_string("src/error_handler.rs")
        .expect("failed to read src/error_handler.rs for architecture guard");
    assert!(
        error_handler_src.contains("check_assignable_or_report("),
        "error_handler type-mismatch emission should route through centralized assignability gateway"
    );
    assert!(
        !error_handler_src.contains("error_type_not_assignable_at(")
            && !error_handler_src.contains("error_type_not_assignable_with_reason_at("),
        "error_handler should not directly emit TS2322 diagnostics; use assignability gateway helpers"
    );

    let call_checker_src =
        fs::read_to_string("src/call_checker.rs").expect("failed to read src/call_checker.rs");
    assert!(
        call_checker_src.contains("ensure_relation_input_ready(")
            && call_checker_src.contains("ensure_relation_inputs_ready("),
        "call_checker should route relation precondition setup through centralized ensure_relation_input(s)_ready helpers"
    );

    let call_boundary_src = fs::read_to_string("src/query_boundaries/call_checker.rs")
        .expect("failed to read src/query_boundaries/call_checker.rs");
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

    let generic_checker_src = fs::read_to_string("src/generic_checker.rs")
        .expect("failed to read src/generic_checker.rs for architecture guard");
    assert!(
        !generic_checker_src.contains("self.ensure_refs_resolved(type_arg);")
            && !generic_checker_src.contains("self.ensure_refs_resolved(instantiated_constraint);"),
        "generic constraint checks should rely on centralized assignability preconditions instead of local ref-resolution traversal"
    );
}

#[test]
fn test_type_cache_surface_excludes_application_and_mapped_eval_caches() {
    let context_src =
        fs::read_to_string("src/context.rs").expect("failed to read src/context.rs for guard");

    let type_cache_start = context_src
        .find("pub struct TypeCache")
        .expect("missing TypeCache struct in context.rs");
    let checker_context_start = context_src[type_cache_start..]
        .find("pub struct CheckerContext")
        .map(|offset| type_cache_start + offset)
        .expect("missing CheckerContext struct in context.rs");
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
        let allowed = rel.ends_with("src/assignability_checker.rs")
            || rel.ends_with("src/query_boundaries/class.rs")
            || rel.ends_with("src/query_boundaries/type_checking.rs")
            || rel.contains("/tests/");
        if allowed {
            continue;
        }
        let src = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("failed to read {}", path.display()));
        if src.contains("should_report_assignability_mismatch(")
            || src.contains("should_report_assignability_mismatch_bivariant(")
        {
            violations.push(rel);
        }
    }

    assert!(
        violations.is_empty(),
        "direct should_report_assignability_mismatch usage should stay in assignability/query boundary modules; violations: {}",
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
            || line.contains(".union2(")
            || line.contains(".intersection2(")
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
    let constructor_checker_src = fs::read_to_string("src/constructor_checker.rs")
        .expect("failed to read src/constructor_checker.rs");

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
    let src = fs::read_to_string("src/state_checking_members/ambient_signature_checks.rs")
        .expect("failed to read ambient signature checker for architecture guard");
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
