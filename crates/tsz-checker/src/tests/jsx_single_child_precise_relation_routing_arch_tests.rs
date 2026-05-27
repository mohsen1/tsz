use std::fs;
use std::path::Path;

#[test]
fn jsx_single_child_precise_type_uses_relation_outcome_boundary() {
    let source_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/checkers/jsx/children.rs");
    let source = fs::read_to_string(&source_path).expect("read JSX children source");

    let function_start = source
        .find("pub(super) fn check_jsx_children_shape")
        .expect("find JSX children shape helper");
    let rest = &source[function_start..];
    let function_end = rest
        .find("\n    pub(super) fn jsx_children_shape_diagnostic_takes_precedence")
        .expect("find next helper");
    let function = &rest[..function_end];
    let precise_branch_start = function
        .find("if child_count == 1")
        .expect("find single-child precise type branch");
    let precise_branch_end = function[precise_branch_start..]
        .find("match child_count")
        .expect("find child count match");
    let precise_branch = &function[precise_branch_start..precise_branch_start + precise_branch_end];

    assert!(
        !precise_branch.contains("diagnostic_relation_boolean_guard"),
        "single JSX child precise-type fallback must use the shared relation outcome boundary"
    );
    assert_eq!(
        precise_branch.matches("assign_relation_outcome").count(),
        1,
        "the synthesized-child to precise-children relation should route through RelationOutcome"
    );
}
