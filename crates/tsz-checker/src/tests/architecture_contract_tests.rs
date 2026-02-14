use crate::context::{CheckerContext, CheckerOptions};
use std::fs;
use std::path::Path;
use tsz_binder::BinderState;
use tsz_parser::parser::node::NodeArena;
use tsz_solver::{
    CallSignature, CallableShape, CompatChecker, FunctionShape, ParamInfo, PropertyInfo,
    RelationCacheKey, TypeId, TypeInterner, Visibility,
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
    let mut ctx = CheckerContext::new(
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
    let mut ctx = CheckerContext::new(
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
    let array_type_src = fs::read_to_string("src/array_type.rs")
        .expect("failed to read src/array_type.rs for architecture guard");
    assert!(
        !array_type_src.contains("TypeKey::Array"),
        "array_type helper should use solver array constructor APIs, not TypeKey::Array"
    );

    let type_literal_src = fs::read_to_string("src/type_literal_checker.rs")
        .expect("failed to read src/type_literal_checker.rs for architecture guard");
    assert!(
        !type_literal_src.contains("TypeKey::ReadonlyType"),
        "type_literal_checker should use solver readonly constructor APIs, not TypeKey::ReadonlyType"
    );

    let type_resolution_src = fs::read_to_string("src/state_type_resolution.rs")
        .expect("failed to read src/state_type_resolution.rs for architecture guard");
    assert!(
        !type_resolution_src.contains("TypeKey::ReadonlyType"),
        "state_type_resolution should use solver readonly constructor APIs, not TypeKey::ReadonlyType"
    );
    assert!(
        !type_resolution_src.contains("intern(tsz_solver::TypeKey::Lazy("),
        "state_type_resolution should use solver lazy constructor API, not direct TypeKey::Lazy interning"
    );

    let type_node_src =
        fs::read_to_string("src/type_node.rs").expect("failed to read src/type_node.rs");
    assert!(
        !type_node_src.contains("TypeKey::ReadonlyType"),
        "type_node should use solver readonly constructor API, not TypeKey::ReadonlyType"
    );
    assert!(
        !type_node_src.contains("TypeKey::KeyOf"),
        "type_node should use solver keyof constructor API, not TypeKey::KeyOf"
    );
    assert!(
        !type_node_src.contains("TypeKey::IndexAccess"),
        "type_node should use solver index_access constructor API, not TypeKey::IndexAccess"
    );

    let jsx_checker_src =
        fs::read_to_string("src/jsx_checker.rs").expect("failed to read src/jsx_checker.rs");
    assert!(
        !jsx_checker_src.contains("TypeKey::IndexAccess"),
        "jsx_checker should use solver index_access constructor API, not TypeKey::IndexAccess"
    );

    let context_src = fs::read_to_string("src/context.rs")
        .expect("failed to read src/context.rs for architecture guard");
    assert!(
        !context_src.contains("self.types.intern(TypeKey::Lazy("),
        "context should use solver lazy constructor API, not direct TypeKey::Lazy interning"
    );

    let queries_src = fs::read_to_string("src/type_checking_queries.rs")
        .expect("failed to read src/type_checking_queries.rs for architecture guard");
    assert!(
        !queries_src.contains("self.ctx.types.intern(TypeKey::Lazy("),
        "type_checking_queries should use solver lazy constructor API, not direct TypeKey::Lazy interning"
    );
    assert!(
        !queries_src.contains("self.ctx.types.intern(TypeKey::TypeParameter("),
        "type_checking_queries should use solver type_param constructor API, not direct TypeKey::TypeParameter interning"
    );

    let state_checking_members_src = fs::read_to_string("src/state_checking_members.rs")
        .expect("failed to read src/state_checking_members.rs for architecture guard");
    assert!(
        !state_checking_members_src.contains("TypeKey::TypeParameter"),
        "state_checking_members should use solver type_param constructor API, not TypeKey::TypeParameter"
    );

    let control_flow_narrowing_src = fs::read_to_string("src/control_flow_narrowing.rs")
        .expect("failed to read src/control_flow_narrowing.rs for architecture guard");
    assert!(
        !control_flow_narrowing_src.contains("intern(TypeKey::Lazy("),
        "control_flow_narrowing should use solver lazy constructor API, not direct TypeKey::Lazy interning"
    );

    let state_type_analysis_src = fs::read_to_string("src/state_type_analysis.rs")
        .expect("failed to read src/state_type_analysis.rs for architecture guard");
    assert!(
        !state_type_analysis_src.contains("intern(TypeKey::TypeQuery("),
        "state_type_analysis should use solver type_query constructor API, not TypeKey::TypeQuery"
    );
    assert!(
        !state_type_analysis_src.contains("intern(TypeKey::TypeParameter("),
        "state_type_analysis should use solver type_param constructor API, not TypeKey::TypeParameter"
    );
    assert!(
        !state_type_analysis_src.contains("intern(tsz_solver::TypeKey::Lazy("),
        "state_type_analysis should use solver lazy constructor API, not direct TypeKey::Lazy interning"
    );
    assert!(
        !state_type_analysis_src.contains("intern(TypeKey::Enum("),
        "state_type_analysis should use solver enum_type constructor API, not TypeKey::Enum interning"
    );

    let function_type_src = fs::read_to_string("src/function_type.rs")
        .expect("failed to read src/function_type.rs for architecture guard");
    assert!(
        !function_type_src.contains("intern(TypeKey::TypeParameter("),
        "function_type should use solver type_param constructor API, not TypeKey::TypeParameter"
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

    let state_type_environment_src = fs::read_to_string("src/state_type_environment.rs")
        .expect("failed to read src/state_type_environment.rs for architecture guard");
    assert!(
        !state_type_environment_src.contains("intern(TypeKey::Enum("),
        "state_type_environment should use solver enum_type constructor API, not TypeKey::Enum"
    );
    assert!(
        !state_type_environment_src.contains("intern(TypeKey::Literal("),
        "state_type_environment should use solver literal constructors, not TypeKey::Literal"
    );
    assert!(
        !state_type_environment_src.contains("SymbolResolutionTraversalKind::"),
        "state_type_environment should use solver visitor traversal helpers, not checker-side SymbolResolutionTraversalKind branching"
    );
    assert!(
        !state_type_environment_src.contains("classify_for_symbol_resolution_traversal("),
        "state_type_environment should not classify traversal in checker; use solver visitor APIs instead"
    );

    let type_computation_complex_src = fs::read_to_string("src/type_computation_complex.rs")
        .expect("failed to read src/type_computation_complex.rs for architecture guard");
    assert!(
        !type_computation_complex_src.contains("intern(tsz_solver::TypeKey::TypeParameter("),
        "type_computation_complex should use solver type_param constructor API, not direct TypeKey::TypeParameter interning"
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
}

#[test]
fn test_checker_sources_forbid_direct_typekey_usage_patterns_and_raw_interning() {
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

    fn has_forbidden_checker_type_construction_pattern(line: &str) -> bool {
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") {
            return false;
        }

        line.contains("use tsz_solver::TypeKey")
            || line.contains("use ") && line.contains("TypeKey")
            || line.contains("intern(TypeKey::")
            || line.contains("intern(tsz_solver::TypeKey::")
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
        "checker source files must not import TypeKey or call raw interner APIs directly; violations: {}",
        violations.join(", ")
    );
}

#[test]
fn test_diagnostics_property_traversal_uses_solver_classification_results() {
    let interner = TypeInterner::new();

    let object = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::STRING,
    )]);
    let callable = interner.callable(CallableShape {
        symbol: None,
        call_signatures: vec![CallSignature {
            type_params: Vec::new(),
            params: Vec::new(),
            this_type: None,
            return_type: TypeId::NUMBER,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
    });
    let members = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    assert!(matches!(
        crate::query_boundaries::diagnostics::classify_property_traversal(&interner, object),
        crate::query_boundaries::diagnostics::PropertyTraversal::Object(_)
    ));
    assert!(matches!(
        crate::query_boundaries::diagnostics::classify_property_traversal(&interner, callable),
        crate::query_boundaries::diagnostics::PropertyTraversal::Callable(_)
    ));
    assert!(matches!(
        crate::query_boundaries::diagnostics::classify_property_traversal(&interner, members),
        crate::query_boundaries::diagnostics::PropertyTraversal::Members(_)
    ));
    assert!(matches!(
        crate::query_boundaries::diagnostics::classify_property_traversal(
            &interner,
            TypeId::BOOLEAN
        ),
        crate::query_boundaries::diagnostics::PropertyTraversal::Other
    ));
}

#[test]
fn test_solver_sources_quarantine_parser_checker_imports_to_lower_rs_only() {
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
        let is_legacy_allowlisted = path.ends_with("tsz-solver/src/lower.rs");

        for (line_index, line) in source.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            let has_forbidden_import =
                line.contains("tsz_parser::") || line.contains("tsz_checker::");
            if has_forbidden_import && !is_legacy_allowlisted {
                violations.push(format!("{}:{}", path.display(), line_index + 1));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "solver parser/checker imports must remain quarantined to lower.rs while migration is in progress; violations: {}",
        violations.join(", ")
    );
}
