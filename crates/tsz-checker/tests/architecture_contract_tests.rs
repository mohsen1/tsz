use crate::context::{CheckerContext, CheckerOptions};
use std::fs;
use std::path::Path;
use tsz_binder::BinderState;
use tsz_parser::parser::node::NodeArena;
use tsz_solver::{
    CompatChecker, FunctionShape, ParamInfo, RelationCacheKey, TypeId, TypeInterner, Visibility,
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
fn test_direct_call_evaluator_usage_is_quarantined_to_query_boundaries() {
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
        let allowed = rel.contains("/query_boundaries/") || rel.contains("/tests/");
        if allowed {
            continue;
        }

        let src = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("failed to read {}", path.display()));
        for (line_index, line) in src.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }

            if line.contains("tsz_solver::CallEvaluator") || line.contains("CallEvaluator::new(") {
                violations.push(format!("{}:{}", rel, line_index + 1));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "direct CallEvaluator usage should stay in query_boundaries modules; violations: {}",
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
fn test_control_flow_avoids_direct_union_interning() {
    let src = fs::read_to_string("src/flow/control_flow/mod.rs")
        .expect("failed to read src/flow/control_flow/mod.rs for architecture guard");
    assert!(
        !src.contains("interner.union("),
        "control_flow should route union construction through query_boundaries/flow_analysis"
    );
}

/// Architecture contract: checker code outside `query_boundaries/` and `tests/`
/// must not call `tsz_solver::type_queries::data::` functions directly.
///
/// The `data` sub-module of `type_queries` is the lowest-level internal data
/// accessor for solver type representations. Checker code should use the
/// thin wrappers in `query_boundaries/common.rs` (or other boundary modules)
/// instead.
///
/// Allowed exceptions:
/// - Files inside `query_boundaries/` (they ARE the boundary)
/// - Test files
#[test]
fn test_no_direct_type_queries_data_access_outside_query_boundaries() {
    fn collect_rs_files(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries {
            let entry = entry.expect("failed to read directory entry");
            let path = entry.path();
            if path.is_dir() {
                collect_rs_files(&path, files);
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
                files.push(path);
            }
        }
    }

    let mut files = Vec::new();
    collect_rs_files(Path::new("src"), &mut files);

    let mut violations = Vec::new();
    for path in &files {
        let rel = path.display().to_string();
        // Allow query_boundaries and test files
        if rel.contains("/query_boundaries/") || rel.contains("/tests/") {
            continue;
        }

        let src = fs::read_to_string(path)
            .unwrap_or_else(|_| panic!("failed to read {}", path.display()));
        for (line_index, line) in src.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            if line.contains("tsz_solver::type_queries::data::") {
                violations.push(format!("{}:{}: {}", rel, line_index + 1, trimmed));
            }
        }
    }

    if !violations.is_empty() {
        panic!(
            "Found {} direct tsz_solver::type_queries::data:: accesses outside query_boundaries.\n\
             These should use boundary wrappers in query_boundaries/common.rs instead.\n\
             Violations:\n  {}",
            violations.len(),
            violations.join("\n  ")
        );
    }
}

/// Architecture contract: checker code outside `query_boundaries/` and `tests/`
/// must not construct `tsz_solver::RelationPolicy` or `tsz_solver::RelationContext`
/// directly.
///
/// These solver-internal policy types should only be constructed inside
/// `query_boundaries` where they translate checker-level concepts (`RelationRequest`,
/// `RelationFlags`) to solver-level knobs.
#[test]
fn test_no_direct_relation_policy_construction_outside_query_boundaries() {
    fn collect_rs_files(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries {
            let entry = entry.expect("failed to read directory entry");
            let path = entry.path();
            if path.is_dir() {
                collect_rs_files(&path, files);
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
                files.push(path);
            }
        }
    }

    let mut files = Vec::new();
    collect_rs_files(Path::new("src"), &mut files);

    let mut violations = Vec::new();
    for path in &files {
        let rel = path.display().to_string();
        if rel.contains("/query_boundaries/") || rel.contains("/tests/") {
            continue;
        }

        let src = fs::read_to_string(path)
            .unwrap_or_else(|_| panic!("failed to read {}", path.display()));
        for (line_index, line) in src.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            if line.contains("tsz_solver::RelationPolicy")
                || line.contains("tsz_solver::RelationContext")
            {
                violations.push(format!("{}:{}: {}", rel, line_index + 1, trimmed));
            }
        }
    }

    if !violations.is_empty() {
        panic!(
            "Found {} direct RelationPolicy/RelationContext uses outside query_boundaries.\n\
             These should be constructed only in query_boundaries/assignability.rs.\n\
             Violations:\n  {}",
            violations.len(),
            violations.join("\n  ")
        );
    }
}

#[test]
fn test_ambient_signature_checks_uses_assignability_query_boundary_helpers() {
    let src = fs::read_to_string("src/state/state_checking_members/ambient_signature_checks.rs").expect(
        "failed to read state/state_checking_members/ambient_signature_checks.rs for architecture guard",
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

/// Architecture contract: checker code outside `query_boundaries/` and `tests/`
/// must not construct `tsz_solver::TypeEvaluator` directly.
///
/// `TypeEvaluator` is the solver's internal evaluation engine. Checker code should
/// use the boundary wrappers in `query_boundaries/state/type_environment.rs`
/// (`evaluate_type_with_resolver`, `evaluate_type_with_cache`,
/// `evaluate_type_suppressing_this`) instead of constructing `TypeEvaluator` directly.
#[test]
fn test_no_direct_type_evaluator_construction_outside_query_boundaries() {
    fn collect_rs_files(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries {
            let entry = entry.expect("failed to read directory entry");
            let path = entry.path();
            if path.is_dir() {
                collect_rs_files(&path, files);
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
                files.push(path);
            }
        }
    }

    let mut files = Vec::new();
    collect_rs_files(Path::new("src"), &mut files);

    let mut violations = Vec::new();
    for path in &files {
        let rel = path.display().to_string();
        if rel.contains("/query_boundaries/") || rel.contains("/tests/") {
            continue;
        }

        let src = fs::read_to_string(path)
            .unwrap_or_else(|_| panic!("failed to read {}", path.display()));
        for (line_index, line) in src.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            if line.contains("TypeEvaluator::with_resolver")
                || line.contains("TypeEvaluator::new(")
                || line.contains("use tsz_solver::TypeEvaluator")
            {
                violations.push(format!("{}:{}: {}", rel, line_index + 1, trimmed));
            }
        }
    }

    if !violations.is_empty() {
        panic!(
            "Found {} direct TypeEvaluator uses outside query_boundaries.\n\
             These should use boundary wrappers in query_boundaries/state/type_environment.rs:\n\
             - evaluate_type_with_resolver (simple evaluation)\n\
             - evaluate_type_with_cache (with cache seeding/draining)\n\
             - evaluate_type_suppressing_this (heritage merging)\n\
             Violations:\n  {}",
            violations.len(),
            violations.join("\n  ")
        );
    }
}

/// Architecture contract: checker code outside `query_boundaries/` and `tests/`
/// must not pattern-match on `tsz_solver::TypeData` variants directly.
///
/// Direct TypeData matching exposes solver-internal type representation details.
/// Checker code should use solver query functions (wrapped through
/// `query_boundaries/`) for type classification instead.
#[test]
fn test_no_direct_type_data_pattern_matching_outside_query_boundaries() {
    fn collect_rs_files(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries {
            let entry = entry.expect("failed to read directory entry");
            let path = entry.path();
            if path.is_dir() {
                collect_rs_files(&path, files);
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
                files.push(path);
            }
        }
    }

    let mut files = Vec::new();
    collect_rs_files(Path::new("src"), &mut files);

    let mut violations = Vec::new();
    for path in &files {
        let rel = path.display().to_string();
        if rel.contains("/query_boundaries/") || rel.contains("/tests/") {
            continue;
        }
        let src = fs::read_to_string(path)
            .unwrap_or_else(|_| panic!("failed to read {}", path.display()));
        for (line_index, line) in src.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            // Detect direct TypeData variant matching (e.g., TypeData::Application,
            // TypeData::Lazy, etc.)
            if line.contains("tsz_solver::TypeData::") {
                violations.push(format!("{}:{}: {}", rel, line_index + 1, trimmed));
            }
        }
    }

    if !violations.is_empty() {
        panic!(
            "Found {} direct tsz_solver::TypeData:: pattern matches outside query_boundaries.\n\
             Checker code should use boundary wrappers (e.g., is_application_type, \n\
             classify_for_type_resolution) instead of matching TypeData variants directly.\n\
             Violations:\n  {}",
            violations.len(),
            violations.join("\n  ")
        );
    }
}
