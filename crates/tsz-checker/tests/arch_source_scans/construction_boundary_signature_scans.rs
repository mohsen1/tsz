//! Construction-side signature boundary scans (issue #13022).
//!
//! The modules listed in issue #13022 used to hand-build
//! `FunctionShape`/`CallableShape` literals and intern them inline via
//! `factory().function(..)` / `factory().callable(..)`. That construction
//! traffic now flows through `query_boundaries::construct_signatures` (and
//! `query_boundaries::type_construction` for the index-signature object
//! fallback). These scans pin the converted modules clean so the quarantine
//! does not refill during parity fixes.
//!
//! `checkers/signature_builder.rs` stays the one checker module allowed to
//! assemble `CallSignature` *data* from AST signatures (the shape structs are
//! SAFE read-only data per the `architecture_contract_tests` import policy);
//! what is forbidden here is shape-literal construction of interned type
//! shapes and direct interning calls.

use std::fs;
use std::path::{Path, PathBuf};

/// The issue #13022 module set, including the split-off submodules of
/// `state/type_resolution/constructors.rs` that belong to the same logical
/// module.
const SIGNATURE_CONSTRUCTION_CLEAN_MODULES: &[&str] = &[
    "src/assignability/assignability_checker.rs",
    "src/checkers/signature_builder.rs",
    "src/classes/class_implements_checker/core.rs",
    "src/context/cross_file_query.rs",
    "src/state/state_checking_members/overload_compatibility.rs",
    "src/state/type_resolution/constructors.rs",
    "src/state/type_resolution/constructors/callable_type_arguments.rs",
    "src/state/type_resolution/constructors/heritage_call_returns.rs",
];

/// The designated checker-side `CallSignature` data assembler.
const SIGNATURE_DATA_BUILDER: &str = "src/checkers/signature_builder.rs";
const CALL_DISPLAY_MODULE: &str = "src/types/computation/call_display.rs";
const CONTEXTUAL_FUNCTION_MATERIALIZATION_MODULES: &[&str] = &[
    "src/types/computation/tagged_template.rs",
    "src/types/function_type_helpers.rs",
];

/// Type literals assemble AST-derived signature/index/property facts, but the
/// solver shape construction belongs to query boundaries.
const TYPE_LITERAL_CHECKER: &str = "src/types/type_literal_checker.rs";

/// Declaration/member type analysis may assemble call signatures from syntax,
/// but function/callable shape interning belongs to the construction boundary.
const DECLARATION_FUNCTION_CONSTRUCTION_CLEAN_MODULES: &[&str] = &[
    "src/classes/class_member_info.rs",
    "src/classes/class_summary.rs",
    "src/state/type_analysis/computed/mod.rs",
    "src/state/type_analysis/computed_helpers_binding.rs",
    "src/state/type_analysis/cross_file_direct_functions.rs",
    "src/state/type_analysis/symbol_type_helpers.rs",
];

/// Constructor/call-resolution paths may choose candidate signatures and
/// contextual inputs, but solver function/callable shape construction stays
/// behind the construction boundary.
const CONSTRUCTOR_CALL_CONSTRUCTION_CLEAN_MODULES: &[&str] = &[
    "src/checkers/call_checker/candidate_collection.rs",
    "src/checkers/call_checker/overload_resolution/contextual_retry.rs",
    "src/checkers/call_checker/overload_resolution/resolve_signatures.rs",
    "src/checkers/call_checker/overload_resolution/return_context.rs",
    "src/classes/constructor_checker.rs",
];

fn checker_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn scan_for_patterns(relative: &str, patterns: &[&str], violations: &mut Vec<String>) {
    let source = fs::read_to_string(checker_path(relative))
        .unwrap_or_else(|err| panic!("failed to read {relative}: {err}"));
    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") {
            continue;
        }
        for pattern in patterns {
            if line.contains(pattern) {
                if pattern_is_return_type_header(line, pattern) {
                    continue;
                }
                violations.push(format!(
                    "{relative}:{} contains `{pattern}`",
                    line_index + 1
                ));
            }
        }
    }
}

fn pattern_is_return_type_header(line: &str, pattern: &str) -> bool {
    let Some(arrow_pos) = line.find("->") else {
        return false;
    };
    let Some(pattern_pos) = line.find(pattern) else {
        return false;
    };
    pattern_pos > arrow_pos && line[pattern_pos + pattern.len()..].trim().is_empty()
}

/// The issue #13022 modules must not intern signature-bearing (or
/// index-signature object) types directly; construction goes through
/// `query_boundaries::construct_signatures` / `type_construction`.
#[test]
fn issue_13022_modules_do_not_intern_signature_types_directly() {
    const INTERNING_PATTERNS: &[&str] = &[".function(", ".callable(", ".object_with_index("];

    let mut violations = Vec::new();
    for module in SIGNATURE_CONSTRUCTION_CLEAN_MODULES {
        scan_for_patterns(module, INTERNING_PATTERNS, &mut violations);
    }
    assert!(
        violations.is_empty(),
        "issue #13022 modules must intern function/callable types through \
         query_boundaries::construct_signatures, not direct factory/db calls:\n{}",
        violations.join("\n")
    );
}

/// The issue #13022 modules must not rebuild interned type shapes as literals;
/// the conversion/rebuild helpers in `query_boundaries::construct_signatures`
/// own the field-by-field shape traffic.
#[test]
fn issue_13022_modules_do_not_build_type_shape_literals() {
    const SHAPE_LITERAL_PATTERNS: &[&str] = &[
        "CallableShape {",
        "CallableShape::default()",
        "FunctionShape {",
        "ObjectShape {",
        "IndexSignature {",
    ];

    let mut violations = Vec::new();
    for module in SIGNATURE_CONSTRUCTION_CLEAN_MODULES {
        scan_for_patterns(module, SHAPE_LITERAL_PATTERNS, &mut violations);
    }
    assert!(
        violations.is_empty(),
        "issue #13022 modules must not hand-build interned type shape literals; \
         use query_boundaries::construct_signatures helpers:\n{}",
        violations.join("\n")
    );
}

/// `call_display.rs` also builds temporary function surfaces for relation and
/// contextual-return checks. Those function types are signature-bearing
/// construction and must stay behind the same boundary, but the module still
/// assembles read-only `CallSignature` data for display skeletons.
#[test]
fn call_display_routes_function_type_construction_through_boundary() {
    const PATTERNS: &[&str] = &[
        ".factory().function(",
        ".factory.function(",
        ".types.function(",
        "FunctionShape::new(",
    ];

    let mut violations = Vec::new();
    scan_for_patterns(CALL_DISPLAY_MODULE, PATTERNS, &mut violations);
    assert!(
        violations.is_empty(),
        "call_display.rs must intern temporary function types through \
         query_boundaries::construct_signatures:\n{}",
        violations.join("\n")
    );
}

/// Contextual typing helpers may assemble `FunctionShape` request data, but
/// interned function types must flow through `construct_signatures`.
#[test]
fn contextual_function_materialization_routes_interning_through_boundary() {
    const PATTERNS: &[&str] = &[
        ".factory().function(",
        ".factory.function(",
        ".types.function(",
        ".ctx.types.function(",
        "FunctionShape::new(",
    ];

    let mut violations = Vec::new();
    for module in CONTEXTUAL_FUNCTION_MATERIALIZATION_MODULES {
        scan_for_patterns(module, PATTERNS, &mut violations);
    }
    assert!(
        violations.is_empty(),
        "contextual function helpers must intern temporary function types through \
         query_boundaries::construct_signatures:\n{}",
        violations.join("\n")
    );
}

/// `CallSignature` literals are signature *data* assembly and belong to the
/// designated builder module (`checkers/signature_builder.rs`) or the
/// boundary itself; the other issue #13022 modules round-trip through
/// `function_shape_from_call_signature` / `call_signature_from_function_shape`.
#[test]
fn call_signature_literals_stay_in_signature_builder() {
    let mut violations = Vec::new();
    for module in SIGNATURE_CONSTRUCTION_CLEAN_MODULES {
        if *module == SIGNATURE_DATA_BUILDER {
            continue;
        }
        scan_for_patterns(module, &["CallSignature {"], &mut violations);
    }
    assert!(
        violations.is_empty(),
        "CallSignature literal assembly outside the designated builder module; \
         use the query_boundaries::construct_signatures conversion helpers:\n{}",
        violations.join("\n")
    );
}

/// Type-literal lowering may collect `CallSignature`, `IndexSignature`, and
/// `PropertyInfo` facts from AST members, but it must not directly intern
/// solver `FunctionShape`/`CallableShape`/`ObjectShape` instances.
#[test]
fn type_literal_checker_routes_shape_construction_through_boundaries() {
    const FORBIDDEN_PATTERNS: &[&str] = &[
        ".function(",
        ".callable(",
        ".object_with_index(",
        ".object_with_flags_and_symbol(",
        ".object_with_late_bound_members(",
        ".object_with_symbol(",
        "CallableShape {",
        "CallableShape::default()",
        "FunctionShape {",
        "ObjectShape {",
        "ObjectShape::default()",
    ];

    let mut violations = Vec::new();
    scan_for_patterns(TYPE_LITERAL_CHECKER, FORBIDDEN_PATTERNS, &mut violations);
    assert!(
        violations.is_empty(),
        "type literal checker must route solver shape construction through \
         query_boundaries helpers while keeping AST fact assembly local:\n{}",
        violations.join("\n")
    );
}

/// Declaration/member type paths can build `CallSignature` data with existing
/// checker helpers, but direct solver function/callable shape interning stays
/// behind `query_boundaries::construct_signatures`.
#[test]
fn declaration_function_paths_route_shape_construction_through_boundaries() {
    const FORBIDDEN_PATTERNS: &[&str] = &[
        ".function(",
        ".callable(",
        "CallableShape {",
        "CallableShape::default()",
        "FunctionShape {",
        "FunctionShape::new(",
    ];

    let mut violations = Vec::new();
    for module in DECLARATION_FUNCTION_CONSTRUCTION_CLEAN_MODULES {
        scan_for_patterns(module, FORBIDDEN_PATTERNS, &mut violations);
    }
    assert!(
        violations.is_empty(),
        "declaration/member type paths must route function/callable solver \
         shape construction through query_boundaries::construct_signatures:\n{}",
        violations.join("\n")
    );
}

/// Constructor/call-resolution paths can build contextual and candidate
/// signature inputs, but direct function/callable shape interning belongs to
/// `query_boundaries::construct_signatures`.
#[test]
fn constructor_call_paths_route_shape_construction_through_boundaries() {
    const FORBIDDEN_PATTERNS: &[&str] = &[
        ".function(",
        ".callable(",
        "CallableShape {",
        "CallableShape::default()",
        "FunctionShape {",
        "FunctionShape::new(",
    ];

    let mut violations = Vec::new();
    for module in CONSTRUCTOR_CALL_CONSTRUCTION_CLEAN_MODULES {
        scan_for_patterns(module, FORBIDDEN_PATTERNS, &mut violations);
    }
    assert!(
        violations.is_empty(),
        "constructor/call-resolution paths must route function/callable solver \
         shape construction through query_boundaries::construct_signatures:\n{}",
        violations.join("\n")
    );
}

/// The boundary helpers this campaign introduced must keep their definitions
/// in `query_boundaries/construct_signatures.rs` (not drift back into
/// `common.rs` or call sites).
#[test]
fn construct_signatures_boundary_owns_construction_helpers() {
    let source = fs::read_to_string(checker_path("src/query_boundaries/construct_signatures.rs"))
        .expect("failed to read query_boundaries/construct_signatures.rs");
    for helper in [
        "function_shape_from_call_signature",
        "function_shape_from_call_signature_preserving_method",
        "call_signature_from_function_shape",
        "function_type_from_shape",
        "function_type_with_return_type",
        "function_type_from_parts",
        "function_type_with_params_replaced",
        "function_type_from_call_signature",
        "function_type_from_call_signature_preserving_method",
        "method_function_type_from_call_signature",
        "call_only_callable_type",
        "construct_only_callable_type",
        "type_literal_callable_type",
        "callable_with_signatures_replaced",
        "callable_with_abstract_flag",
        "callable_with_construct_return_type",
        "callable_with_properties_replaced",
        "callable_with_call_signatures_and_erased_metadata",
        "instantiated_callable_from_base",
        "map_function_shape_types",
    ] {
        assert!(
            source.contains(&format!("fn {helper}(")),
            "query_boundaries::construct_signatures must own the `{helper}` helper"
        );
    }

    let common = fs::read_to_string(checker_path("src/query_boundaries/common.rs"))
        .expect("failed to read query_boundaries/common.rs");
    for helper in [
        "call_only_callable_type",
        "construct_only_callable_type",
        "type_literal_callable_type",
        "callable_with_signatures_replaced",
        "callable_with_abstract_flag",
        "callable_with_construct_return_type",
        "callable_with_properties_replaced",
        "callable_with_call_signatures_and_erased_metadata",
        "instantiated_callable_from_base",
        "map_function_shape_types",
    ] {
        assert!(
            !common.contains(&format!("fn {helper}(")),
            "construction helper `{helper}` must not migrate into common.rs"
        );
    }
}

/// Object-shape helpers for inline type literals live in the construction
/// boundary, not in checker call sites.
#[test]
fn type_construction_boundary_owns_type_literal_object_helpers() {
    let source = fs::read_to_string(checker_path("src/query_boundaries/type_construction.rs"))
        .expect("failed to read query_boundaries/type_construction.rs");
    for helper in [
        "type_literal_object_with_index",
        "type_literal_extra_number_index_object",
        "type_literal_object",
    ] {
        assert!(
            source.contains(&format!("fn {helper}(")),
            "query_boundaries::type_construction must own the `{helper}` helper"
        );
    }
}
