use std::{fs, path::PathBuf};

#[test]
fn impossible_intersection_pruning_uses_subtype_outcome_boundary() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src/state/type_environment/lazy_impossible_pruning.rs");
    let source = fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
    let compact: String = source.chars().filter(|c| !c.is_whitespace()).collect();

    // The evaluated property type is bound to a local (`prop_type`) before the
    // outcome query, not passed as the raw `prop.type_id` field access — pin
    // the actual call sites rather than an inlined field expression.
    assert!(
        compact.contains("diagnostic_subtype_outcome(prop_type,other).related")
            && compact.contains("diagnostic_subtype_outcome(other,prop_type).related"),
        "literal-discriminant impossibility pruning should use subtype outcomes"
    );
    // There is no separate required-property unit-intersection pruning pass:
    // per this module's own doc comment, a required `never` property alone
    // must not make an object impossible (tsc preserves such members in
    // unions), so no second mechanism should exist to pin here.
    assert!(
        !compact.contains("is_subtype_of(prop_type,other)")
            && !compact.contains("is_subtype_of(other,prop_type)"),
        "impossible-intersection pruning should not consume raw subtype probes"
    );
}
