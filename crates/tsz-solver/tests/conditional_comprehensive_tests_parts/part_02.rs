/// Suffix slot bug: `[number?, string?] extends [...unknown[], infer A] ? A : never` → `never`.
/// The pattern's required suffix element cannot be satisfied by an optional trailing source slot.
#[test]
fn test_optional_source_suffix_does_not_match_required_suffix_pattern_slot() {
    let interner = TypeInterner::new();
    let infer_a = make_infer(&interner, "A");
    let rest_unknown = interner.array(TypeId::UNKNOWN);

    // Pattern: [...unknown[], infer A] — rest at index 0, A is suffix (required)
    let pattern = interner.tuple(vec![
        make_rest_element(rest_unknown),
        make_tuple_element(infer_a),
    ]);
    let source = interner.tuple(vec![
        make_optional_element(TypeId::NUMBER),
        make_optional_element(TypeId::STRING),
    ]);

    let cond = ConditionalType {
        check_type: source,
        extends_type: pattern,
        true_type: infer_a,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_type(&interner, interner.conditional(cond));
    assert_eq!(
        result,
        TypeId::NEVER,
        "[number?, string?] extends [...unknown[], infer A] must take the false branch: \
         trailing optional source slot cannot satisfy required suffix pattern slot"
    );
}

/// CONTROL — suffix required: `[number, string] extends [...unknown[], infer A] ? A : never`
/// → string (true branch, A = string from the required trailing element).
#[test]
fn test_required_source_suffix_matches_required_suffix_pattern_slot() {
    let interner = TypeInterner::new();
    let infer_a = make_infer(&interner, "A");
    let rest_unknown = interner.array(TypeId::UNKNOWN);

    let pattern = interner.tuple(vec![
        make_rest_element(rest_unknown),
        make_tuple_element(infer_a),
    ]);
    let source = interner.tuple(vec![
        make_tuple_element(TypeId::NUMBER),
        make_tuple_element(TypeId::STRING),
    ]);

    let cond = ConditionalType {
        check_type: source,
        extends_type: pattern,
        true_type: infer_a,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_type(&interner, interner.conditional(cond));
    assert_eq!(
        result,
        TypeId::STRING,
        "[number, string] extends [...unknown[], infer A] should bind A = string (true branch)"
    );
}
