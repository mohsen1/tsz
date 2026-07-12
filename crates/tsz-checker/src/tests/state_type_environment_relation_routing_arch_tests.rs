use std::{fs, path::PathBuf};

#[test]
fn impossible_intersection_pruning_uses_subtype_outcome_boundary() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src/state/type_environment/lazy_impossible_pruning.rs");
    let source = fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
    let compact: String = source.chars().filter(|c| !c.is_whitespace()).collect();

    assert!(
        compact.contains("diagnostic_subtype_outcome(prop.type_id,other).related")
            && compact.contains("diagnostic_subtype_outcome(other,prop.type_id).related"),
        "literal-discriminant impossibility pruning should use subtype outcomes"
    );
    assert!(
        compact.contains("diagnostic_subtype_outcome(evaluated_member,other).related")
            && compact.contains("diagnostic_subtype_outcome(other,evaluated_member).related"),
        "required-property unit-intersection pruning should use subtype outcomes"
    );
    assert!(
        !compact.contains("is_subtype_of(prop.type_id,other)")
            && !compact.contains("is_subtype_of(other,prop.type_id)")
            && !compact.contains("is_subtype_of(evaluated_member,other)")
            && !compact.contains("is_subtype_of(other,evaluated_member)"),
        "impossible-intersection pruning should not consume raw subtype probes"
    );
}
