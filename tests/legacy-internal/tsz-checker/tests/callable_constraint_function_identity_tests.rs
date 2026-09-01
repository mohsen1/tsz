#[test]
fn callable_constraint_function_detection_uses_lib_identity_helper() {
    let source = include_str!("../checkers/generic_checker/callable_constraint_helpers.rs");
    assert!(
        !source.contains("symbol.escaped_name == \"Function\""),
        "callable Function constraint detection must use lib/global identity, not a local name check"
    );
    assert!(
        source.contains("sym_id_is_lib_function"),
        "expected callable Function constraint detection to route through the lib identity helper"
    );
}

#[test]
fn instanceof_function_fallback_uses_lib_identity_helper() {
    let source = include_str!("../types/computation/binary_support.rs");
    assert!(
        !source.contains("symbol.escaped_name == \"Function\""),
        "instanceof Function fallback must use lib/global identity, not a local name check"
    );
    assert!(
        source.contains("sym_id_is_lib_function"),
        "expected instanceof Function fallback to route through the lib identity helper"
    );
}
