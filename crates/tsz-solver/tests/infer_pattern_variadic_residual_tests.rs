use crate::construction::TypeInterner;
use crate::evaluation::evaluate::evaluate_type;
use crate::{ConditionalType, TupleElement, TypeData, TypeId, TypeParamInfo};

fn infer_var(interner: &TypeInterner, name: &str) -> TypeId {
    let name = interner.intern_string(name);
    interner.intern(TypeData::Infer(TypeParamInfo {
        name,
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::TypeParamOrigin::User,
    }))
}

fn tuple_elem(type_id: TypeId) -> TupleElement {
    TupleElement {
        type_id,
        name: None,
        optional: false,
        rest: false,
    }
}

fn rest_tuple_elem(type_id: TypeId) -> TupleElement {
    TupleElement {
        type_id,
        name: None,
        optional: false,
        rest: true,
    }
}

#[test]
fn conditional_infer_head_from_variadic_source_with_array_rest() {
    let interner = TypeInterner::new();
    let infer_h = infer_var(&interner, "H");
    let any_array = interner.array(TypeId::ANY);

    let extends_tuple = interner.tuple(vec![tuple_elem(infer_h), rest_tuple_elem(any_array)]);
    let source = interner.tuple(vec![
        tuple_elem(TypeId::STRING),
        rest_tuple_elem(interner.array(TypeId::NUMBER)),
    ]);
    let cond = ConditionalType {
        check_type: source,
        extends_type: extends_tuple,
        true_type: infer_h,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    assert_eq!(
        evaluate_type(&interner, interner.conditional(cond)),
        TypeId::STRING
    );
}

#[test]
fn conditional_infer_rest_simplifies_single_rest_residual_to_array() {
    let interner = TypeInterner::new();
    let infer_a = infer_var(&interner, "A");
    let infer_b = infer_var(&interner, "B");
    let number_array = interner.array(TypeId::NUMBER);

    let extends_tuple = interner.tuple(vec![tuple_elem(infer_a), rest_tuple_elem(infer_b)]);
    let source = interner.tuple(vec![
        tuple_elem(TypeId::STRING),
        rest_tuple_elem(number_array),
    ]);
    let true_branch = interner.tuple(vec![tuple_elem(infer_a), tuple_elem(infer_b)]);
    let cond = ConditionalType {
        check_type: source,
        extends_type: extends_tuple,
        true_type: true_branch,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let expected = interner.tuple(vec![tuple_elem(TypeId::STRING), tuple_elem(number_array)]);
    assert_eq!(
        evaluate_type(&interner, interner.conditional(cond)),
        expected
    );
}

#[test]
fn conditional_infer_last_from_leading_rest_variadic_source() {
    let interner = TypeInterner::new();
    let infer_l = infer_var(&interner, "L");
    let any_array = interner.array(TypeId::ANY);

    let extends_tuple = interner.tuple(vec![rest_tuple_elem(any_array), tuple_elem(infer_l)]);
    let source = interner.tuple(vec![
        rest_tuple_elem(interner.array(TypeId::NUMBER)),
        tuple_elem(TypeId::STRING),
    ]);
    let cond = ConditionalType {
        check_type: source,
        extends_type: extends_tuple,
        true_type: infer_l,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    assert_eq!(
        evaluate_type(&interner, interner.conditional(cond)),
        TypeId::STRING
    );
}

#[test]
fn conditional_infer_head_from_multi_prefix_variadic_source() {
    let interner = TypeInterner::new();
    let infer_h = infer_var(&interner, "H");
    let any_array = interner.array(TypeId::ANY);

    let extends_tuple = interner.tuple(vec![tuple_elem(infer_h), rest_tuple_elem(any_array)]);
    let source = interner.tuple(vec![
        tuple_elem(TypeId::STRING),
        tuple_elem(TypeId::BOOLEAN),
        rest_tuple_elem(interner.array(TypeId::NUMBER)),
    ]);
    let cond = ConditionalType {
        check_type: source,
        extends_type: extends_tuple,
        true_type: infer_h,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    assert_eq!(
        evaluate_type(&interner, interner.conditional(cond)),
        TypeId::STRING
    );
}

#[test]
fn conditional_infer_head_empty_source_takes_false_branch() {
    let interner = TypeInterner::new();
    let infer_h = infer_var(&interner, "H");
    let any_array = interner.array(TypeId::ANY);

    let extends_tuple = interner.tuple(vec![tuple_elem(infer_h), rest_tuple_elem(any_array)]);
    let cond = ConditionalType {
        check_type: interner.tuple(Vec::new()),
        extends_type: extends_tuple,
        true_type: infer_h,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    assert_eq!(
        evaluate_type(&interner, interner.conditional(cond)),
        TypeId::NEVER
    );
}

// =============================================================================
// Tail inference: concrete fixed-element tuples
// =============================================================================

#[test]
fn conditional_infer_tail_of_three_element_tuple_is_two_element_tuple() {
    let interner = TypeInterner::new();
    let infer_h = infer_var(&interner, "_H");
    let infer_rest = infer_var(&interner, "Rest");

    let extends_tuple = interner.tuple(vec![tuple_elem(infer_h), rest_tuple_elem(infer_rest)]);
    let source = interner.tuple(vec![
        tuple_elem(TypeId::STRING),
        tuple_elem(TypeId::NUMBER),
        tuple_elem(TypeId::BOOLEAN),
    ]);
    let cond = ConditionalType {
        check_type: source,
        extends_type: extends_tuple,
        true_type: infer_rest,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let expected = interner.tuple(vec![
        tuple_elem(TypeId::NUMBER),
        tuple_elem(TypeId::BOOLEAN),
    ]);
    assert_eq!(
        evaluate_type(&interner, interner.conditional(cond)),
        expected,
        "Tail<[string, number, boolean]> should produce [number, boolean]"
    );
}

#[test]
fn conditional_infer_tail_of_two_element_tuple_is_one_element_tuple() {
    let interner = TypeInterner::new();
    let infer_h = infer_var(&interner, "_H");
    let infer_rest = infer_var(&interner, "Rest");

    let extends_tuple = interner.tuple(vec![tuple_elem(infer_h), rest_tuple_elem(infer_rest)]);
    let source = interner.tuple(vec![tuple_elem(TypeId::STRING), tuple_elem(TypeId::NUMBER)]);
    let cond = ConditionalType {
        check_type: source,
        extends_type: extends_tuple,
        true_type: infer_rest,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let expected = interner.tuple(vec![tuple_elem(TypeId::NUMBER)]);
    assert_eq!(
        evaluate_type(&interner, interner.conditional(cond)),
        expected,
        "Tail<[string, number]> should produce [number]"
    );
}

#[test]
fn conditional_infer_tail_of_single_element_tuple_is_empty_tuple() {
    let interner = TypeInterner::new();
    let infer_h = infer_var(&interner, "_H");
    let infer_rest = infer_var(&interner, "Rest");

    let extends_tuple = interner.tuple(vec![tuple_elem(infer_h), rest_tuple_elem(infer_rest)]);
    let source = interner.tuple(vec![tuple_elem(TypeId::STRING)]);
    let cond = ConditionalType {
        check_type: source,
        extends_type: extends_tuple,
        true_type: infer_rest,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let expected = interner.tuple(vec![]);
    assert_eq!(
        evaluate_type(&interner, interner.conditional(cond)),
        expected,
        "Tail<[string]> should produce []"
    );
}

#[test]
fn conditional_infer_prepend_inferred_tail_is_flattened_into_result_tuple() {
    // Source=[number,boolean,string], Rest=[boolean,string] after matching head.
    // True branch [string, ...Rest] must flatten to [string, boolean, string].
    let interner = TypeInterner::new();
    let infer_h = infer_var(&interner, "_H");
    let infer_rest = infer_var(&interner, "Rest");

    let extends_tuple = interner.tuple(vec![tuple_elem(infer_h), rest_tuple_elem(infer_rest)]);
    let source = interner.tuple(vec![
        tuple_elem(TypeId::NUMBER),
        tuple_elem(TypeId::BOOLEAN),
        tuple_elem(TypeId::STRING),
    ]);
    let true_branch = interner.tuple(vec![
        tuple_elem(TypeId::STRING),
        rest_tuple_elem(infer_rest),
    ]);
    let cond = ConditionalType {
        check_type: source,
        extends_type: extends_tuple,
        true_type: true_branch,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let expected = interner.tuple(vec![
        tuple_elem(TypeId::STRING),
        tuple_elem(TypeId::BOOLEAN),
        tuple_elem(TypeId::STRING),
    ]);
    assert_eq!(
        evaluate_type(&interner, interner.conditional(cond)),
        expected,
        "[string, ...Rest] where Rest=[boolean,string] should produce [string, boolean, string]"
    );
}

#[test]
fn evaluate_spread_of_concrete_tuple_flattens_inline() {
    let interner = TypeInterner::new();
    let inner = interner.tuple(vec![
        tuple_elem(TypeId::NUMBER),
        tuple_elem(TypeId::BOOLEAN),
    ]);
    let spread_tuple = interner.tuple(vec![tuple_elem(TypeId::STRING), rest_tuple_elem(inner)]);

    let expected = interner.tuple(vec![
        tuple_elem(TypeId::STRING),
        tuple_elem(TypeId::NUMBER),
        tuple_elem(TypeId::BOOLEAN),
    ]);
    assert_eq!(
        evaluate_type(&interner, spread_tuple),
        expected,
        "[string, ...[number, boolean]] should evaluate to [string, number, boolean]"
    );
}

#[test]
fn conditional_infer_tail_applied_to_previous_tail_preserves_arity() {
    // Tail<Tail<[string, number, boolean]>> must be [boolean], not [number, boolean].
    // Validates that chained infer bindings produce independent residuals.
    let interner = TypeInterner::new();
    let infer_h = infer_var(&interner, "_H");
    let infer_rest = infer_var(&interner, "Rest");
    let extends_tuple = interner.tuple(vec![tuple_elem(infer_h), rest_tuple_elem(infer_rest)]);

    let source1 = interner.tuple(vec![
        tuple_elem(TypeId::STRING),
        tuple_elem(TypeId::NUMBER),
        tuple_elem(TypeId::BOOLEAN),
    ]);
    let cond1 = ConditionalType {
        check_type: source1,
        extends_type: extends_tuple,
        true_type: infer_rest,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };
    let tail1 = evaluate_type(&interner, interner.conditional(cond1));

    let infer_h2 = infer_var(&interner, "_H2");
    let infer_rest2 = infer_var(&interner, "Rest2");
    let extends_tuple2 = interner.tuple(vec![tuple_elem(infer_h2), rest_tuple_elem(infer_rest2)]);
    let cond2 = ConditionalType {
        check_type: tail1,
        extends_type: extends_tuple2,
        true_type: infer_rest2,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };
    let tail2 = evaluate_type(&interner, interner.conditional(cond2));

    let expected = interner.tuple(vec![tuple_elem(TypeId::BOOLEAN)]);
    assert_eq!(
        tail2, expected,
        "Tail<Tail<[string, number, boolean]>> should produce [boolean]"
    );
}
