use std::fs;

#[test]
fn function_type_relation_probes_use_relation_outcome_boundary() {
    let source =
        fs::read_to_string("src/types/function_type.rs").expect("failed to read function_type.rs");
    let compact: String = source.chars().filter(|c| !c.is_whitespace()).collect();

    assert!(
        compact.contains(
            "function_type_compatibility_relation_outcome(from_expected,evaluated_constraint,).related"
        ),
        "contextual constrained type-parameter extraction should use function-type relation outcomes"
    );
    assert!(
        compact
            .contains("function_type_compatibility_relation_outcome(member,instance_type).related"),
        "JS constructor return union member collapse should use function-type relation outcomes"
    );
    assert!(
        compact
            .contains("function_type_compatibility_relation_outcome(instance_type,member).related"),
        "JS constructor return instance/member reverse probe should use function-type relation outcomes"
    );
    assert!(
        !compact.contains("assign_relation_outcome(from_expected,evaluated_constraint,).related")
            && !compact.contains("assign_relation_outcome(member,instance_type).related")
            && !compact.contains("assign_relation_outcome(instance_type,member).related"),
        "function-type diagnostic probes should not use generic assignment relation outcomes"
    );
    assert!(
        compact.contains("diagnostic_subtype_outcome(extracted,from_expected).related")
            && compact.contains("diagnostic_subtype_outcome(from_expected,extracted).related"),
        "contextual function-type specificity checks should use subtype outcomes"
    );
    assert!(
        !compact.contains("is_assignable_to(from_expected,evaluated_constraint)"),
        "contextual constrained type-parameter extraction should not use raw assignability"
    );
    assert!(
        !compact.contains("is_assignable_to(member,instance_type)"),
        "JS constructor return union member collapse should not use raw assignability"
    );
    assert!(
        !compact.contains("is_assignable_to(instance_type,member)"),
        "JS constructor return instance/member reverse probe should not use raw assignability"
    );
    assert!(
        !compact.contains("is_subtype_of(extracted,from_expected)")
            && !compact.contains("is_subtype_of(from_expected,extracted)"),
        "contextual function-type specificity checks should not use raw subtype probes"
    );
}
