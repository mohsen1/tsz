use std::fs;
use std::path::Path;

#[test]
fn diagnostic_source_display_narrowing_uses_relation_outcome_boundary() {
    let source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/error_reporter/core/diagnostic_source.rs"),
    )
    .expect("failed to read diagnostic_source.rs");

    let narrowing_block = source
        .split("let expr_is_assignability_narrower =")
        .nth(1)
        .and_then(|tail| tail.split("let expr_is_union_subset_narrower").next())
        .expect("failed to isolate diagnostic source display narrowing block");

    assert!(
        narrowing_block.contains("assign_relation_outcome(expr_display_type, declared_type)"),
        "diagnostic source display narrowing should route expression-to-declared relation through assign_relation_outcome"
    );
    assert!(
        narrowing_block.contains("assign_relation_outcome(declared_type, expr_display_type)"),
        "diagnostic source display narrowing should route declared-to-expression relation through assign_relation_outcome"
    );
    assert!(
        !narrowing_block.contains("diagnostic_relation_boolean_guard("),
        "diagnostic source display narrowing should not use raw diagnostic boolean relation probes"
    );
}
