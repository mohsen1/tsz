#[test]
fn function_intrinsic_extends_callable_in_conditional_types() {
    use crate::types::{FunctionShape, ParamInfo};

    let interner = TypeInterner::new();
    let callable_target = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: None,
            type_id: TypeId::ANY,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::ANY,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let cond = ConditionalType {
        check_type: TypeId::FUNCTION,
        extends_type: callable_target,
        true_type: TypeId::STRING,
        false_type: TypeId::NUMBER,
        is_distributive: false,
    };

    let result = evaluate_type(&interner, interner.conditional(cond));

    assert_eq!(
        result,
        TypeId::STRING,
        "conditional types keep tsc's Function-extends-callable true branch"
    );
}

fn make_rest_element(type_id: TypeId) -> crate::types::TupleElement {
    crate::types::TupleElement {
        type_id,
        name: None,
        optional: false,
        rest: true,
    }
}

fn make_optional_element(type_id: TypeId) -> crate::types::TupleElement {
    crate::types::TupleElement {
        type_id,
        name: None,
        optional: true,
        rest: false,
    }
}

/// Reported bug: `[number?, string?] extends [infer A, ...unknown[]] ? A : never`
/// → `never` (false branch). tsz previously returned `number | undefined` (true branch).
#[test]
fn test_optional_source_prefix_does_not_match_required_pattern_slot() {
    let interner = TypeInterner::new();
    let infer_a = make_infer(&interner, "A");
    let rest_unknown = interner.array(TypeId::UNKNOWN);

    let pattern = interner.tuple(vec![
        make_tuple_element(infer_a),
        make_rest_element(rest_unknown),
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
        "[number?, string?] extends [infer A, ...unknown[]] must take the false branch: \
         source min length (0) < pattern required prefix (1)"
    );
}

/// Same rule with a renamed infer variable (`Elem`) and non-`never` false branch (`"NONE"`).
/// Proves the rule is structural, not keyed on the name "A".
#[test]
fn test_optional_source_prefix_renamed_infer_var_and_false_branch() {
    let interner = TypeInterner::new();
    let infer_elem = make_infer(&interner, "Elem");
    let rest_unknown = interner.array(TypeId::UNKNOWN);
    let none_type = interner.literal_string("NONE");

    let pattern = interner.tuple(vec![
        make_tuple_element(infer_elem),
        make_rest_element(rest_unknown),
    ]);
    let source = interner.tuple(vec![
        make_optional_element(TypeId::BOOLEAN),
        make_optional_element(TypeId::SYMBOL),
    ]);

    let cond = ConditionalType {
        check_type: source,
        extends_type: pattern,
        true_type: infer_elem,
        false_type: none_type,
        is_distributive: false,
    };

    let result = evaluate_type(&interner, interner.conditional(cond));
    assert_eq!(
        result, none_type,
        "[boolean?, symbol?] extends [infer Elem, ...unknown[]] must resolve to false branch \"NONE\""
    );
}

/// Single-element optional source: `[number?] extends [infer A, ...unknown[]]` → false.
#[test]
fn test_single_optional_source_element_does_not_match_required_prefix() {
    let interner = TypeInterner::new();
    let infer_a = make_infer(&interner, "A");
    let rest_unknown = interner.array(TypeId::UNKNOWN);

    let pattern = interner.tuple(vec![
        make_tuple_element(infer_a),
        make_rest_element(rest_unknown),
    ]);
    let source = interner.tuple(vec![make_optional_element(TypeId::NUMBER)]);

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
        "[number?] extends [infer A, ...unknown[]] must take the false branch"
    );
}

/// CONTROL — required leading element: `[number, string?] extends [infer A, ...unknown[]]`
/// must take the TRUE branch and bind A = number.
#[test]
fn test_required_leading_source_element_matches_required_prefix_slot() {
    let interner = TypeInterner::new();
    let infer_a = make_infer(&interner, "A");
    let rest_unknown = interner.array(TypeId::UNKNOWN);

    let pattern = interner.tuple(vec![
        make_tuple_element(infer_a),
        make_rest_element(rest_unknown),
    ]);
    let source = interner.tuple(vec![
        make_tuple_element(TypeId::NUMBER),
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
        TypeId::NUMBER,
        "[number, string?] extends [infer A, ...unknown[]] should bind A = number (true branch)"
    );
}

/// CONTROL — all required: `[number, string] extends [infer A, infer B]`
/// must take the true branch (both elements required in source and pattern).
#[test]
fn test_all_required_elements_match_no_rest() {
    let interner = TypeInterner::new();
    let infer_a = make_infer(&interner, "A");

    let pattern = interner.tuple(vec![
        make_tuple_element(infer_a),
        make_tuple_element(make_infer(&interner, "B")),
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
        TypeId::NUMBER,
        "[number, string] extends [infer A, infer B] should bind A = number (true branch)"
    );
}

/// No-rest case bug: `[number?] extends [infer A] ? A : never` → `never`.
/// Pattern has a single required element; source has a single optional element.
#[test]
fn test_optional_source_does_not_match_required_no_rest_pattern() {
    let interner = TypeInterner::new();
    let infer_a = make_infer(&interner, "A");

    let pattern = interner.tuple(vec![make_tuple_element(infer_a)]);
    let source = interner.tuple(vec![make_optional_element(TypeId::NUMBER)]);

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
        "[number?] extends [infer A] (no rest) must take the false branch"
    );
}

/// CONTROL — no-rest required: `[number] extends [infer A] ? A : never` → number.
#[test]
fn test_required_source_matches_required_no_rest_pattern() {
    let interner = TypeInterner::new();
    let infer_a = make_infer(&interner, "A");

    let pattern = interner.tuple(vec![make_tuple_element(infer_a)]);
    let source = interner.tuple(vec![make_tuple_element(TypeId::NUMBER)]);

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
        TypeId::NUMBER,
        "[number] extends [infer A] (no rest) should bind A = number (true branch)"
    );
}
