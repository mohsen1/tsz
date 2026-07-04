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
const CALL_CONTEXT_FUNCTION_SURFACE_MODULES: &[&str] = &[
    "src/types/computation/call_inference.rs",
    "src/types/computation/call/callee_context.rs",
    "src/types/computation/complex.rs",
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
        // A `Shape {` token in return-type position (`fn f(..) -> Shape {`)
        // is a function header, not a shape-literal construction.
        if trimmed.contains("->") {
            continue;
        }
        for pattern in patterns {
            if line.contains(pattern) {
                violations.push(format!(
                    "{relative}:{} contains `{pattern}`",
                    line_index + 1
                ));
            }
        }
    }
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

/// Call inference and callee/new-expression context helpers may assemble
/// request shapes, but the interned function/callable surfaces must flow
/// through `construct_signatures`.
#[test]
fn call_context_function_surfaces_route_interning_through_boundary() {
    const PATTERNS: &[&str] = &[
        ".factory().function(",
        ".factory().callable(",
        ".factory.function(",
        ".factory.callable(",
        ".types.function(",
        ".types.callable(",
        ".ctx.types.function(",
        ".ctx.types.callable(",
        "FunctionShape::new(",
    ];

    let mut violations = Vec::new();
    for module in CALL_CONTEXT_FUNCTION_SURFACE_MODULES {
        scan_for_patterns(module, PATTERNS, &mut violations);
    }
    assert!(
        violations.is_empty(),
        "call context helpers must intern temporary function/callable types through \
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

/// The boundary helpers this campaign introduced must keep their definitions
/// in `query_boundaries/construct_signatures.rs` (not drift back into
/// `common.rs` or call sites).
#[test]
fn construct_signatures_boundary_owns_construction_helpers() {
    let source = fs::read_to_string(checker_path("src/query_boundaries/construct_signatures.rs"))
        .expect("failed to read query_boundaries/construct_signatures.rs");
    for helper in [
        "function_shape_from_call_signature",
        "call_signature_from_function_shape",
        "function_type_from_shape",
        "function_type_from_parts",
        "function_type_with_params_replaced",
        "function_type_from_call_signature",
        "function_type_from_call_signature_preserving_method",
        "call_only_callable_type",
        "construct_only_callable_type",
        "callable_type_from_shape",
        "callable_with_signatures_replaced",
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
        "callable_type_from_shape",
        "callable_with_signatures_replaced",
        "instantiated_callable_from_base",
        "map_function_shape_types",
    ] {
        assert!(
            !common.contains(&format!("fn {helper}(")),
            "construction helper `{helper}` must not migrate into common.rs"
        );
    }
}
