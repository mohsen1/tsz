use std::fs;

#[test]
fn union_excess_required_property_filter_uses_relation_outcome_boundary() {
    let source = fs::read_to_string("src/state/state_checking/property.rs")
        .expect("failed to read state_checking/property.rs");
    let compact: String = source.chars().filter(|ch| !ch.is_whitespace()).collect();

    assert!(
        compact.contains(
            "union_excess_required_property_relation_outcome(source_prop.type_id,target_prop.type_id,).related"
        ),
        "union excess fallback required-property filtering should use the union excess relation request"
    );
    assert!(
        !compact.contains("assign_relation_outcome(source_prop.type_id,target_prop.type_id)")
            && !compact
                .contains("assign_relation_outcome(source_prop.type_id,target_prop.type_id,)"),
        "union excess fallback required-property filtering should not use the generic assignment request"
    );
}
