#[test]
fn recursive_heritage_conflict_check_does_not_compare_rendered_types() {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/checkers/generic_checker/recursive_heritage_constraint.rs"
    ))
    .expect("recursive heritage checker source should be readable");

    let start = source
        .find("pub(super) fn member_has_conflicting_constraint_property")
        .expect("recursive heritage conflict helper should exist");
    let body = &source[start..];
    let end = body
        .find("\n    }\n}")
        .expect("recursive heritage conflict helper should end before impl close");
    let helper_body = &body[..end];

    assert!(
        !helper_body.contains("format_type_diagnostic"),
        "recursive heritage conflict detection must use structural facts, not rendered type strings"
    );
    assert!(
        helper_body.contains("recursive_heritage_property_types_conflict"),
        "recursive heritage conflict detection should route through the assignability boundary"
    );
}
