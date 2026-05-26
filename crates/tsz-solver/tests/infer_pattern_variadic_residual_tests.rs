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
