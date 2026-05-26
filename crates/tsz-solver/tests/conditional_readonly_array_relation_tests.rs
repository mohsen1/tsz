//! Conditional `extends` relation coverage for readonly array surfaces.

use super::*;
use crate::evaluation::evaluate::evaluate_type;
use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::intern::TypeInterner;
use crate::types::{ConditionalType, TupleElement, TypeParamInfo};

fn conditional_result(interner: &TypeInterner, check_type: TypeId, extends_type: TypeId) -> TypeId {
    let yes = interner.literal_string("Y");
    let no = interner.literal_string("N");
    let cond_id = interner.conditional(ConditionalType {
        check_type,
        extends_type,
        true_type: yes,
        false_type: no,
        is_distributive: false,
    });
    evaluate_type(interner, cond_id)
}

#[test]
fn generic_readonly_array_extends_mutable_array_takes_false_branch() {
    let interner = TypeInterner::new();
    let s_name = interner.intern_string("S");
    let t_name = interner.intern_string("T");
    let s_param = interner.type_param(TypeParamInfo::simple(s_name));
    let t_param = interner.type_param(TypeParamInfo::simple(t_name));
    let readonly_numbers = interner.readonly_array(TypeId::NUMBER);
    let mutable_numbers = interner.array(TypeId::NUMBER);
    let no = interner.literal_string("N");
    let cond_id = interner.conditional(ConditionalType {
        check_type: s_param,
        extends_type: t_param,
        true_type: interner.literal_string("Y"),
        false_type: no,
        is_distributive: false,
    });
    let mut subst = TypeSubstitution::new();
    subst.insert(s_name, readonly_numbers);
    subst.insert(t_name, mutable_numbers);
    let instantiated = instantiate_type(&interner, cond_id, &subst);

    assert_eq!(
        evaluate_type(&interner, instantiated),
        no,
        "R<readonly number[], number[]> should take the false branch"
    );
}

#[test]
fn generic_readonly_tuple_extends_mutable_array_takes_false_branch() {
    let interner = TypeInterner::new();
    let s_name = interner.intern_string("S");
    let t_name = interner.intern_string("T");
    let s_param = interner.type_param(TypeParamInfo::simple(s_name));
    let t_param = interner.type_param(TypeParamInfo::simple(t_name));
    let tuple = interner.tuple(vec![
        TupleElement {
            type_id: interner.literal_number(1.0),
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: interner.literal_number(2.0),
            name: None,
            optional: false,
            rest: false,
        },
    ]);
    let readonly_tuple = interner.readonly_type(tuple);
    let mutable_numbers = interner.array(TypeId::NUMBER);
    let no = interner.literal_string("N");
    let cond_id = interner.conditional(ConditionalType {
        check_type: s_param,
        extends_type: t_param,
        true_type: interner.literal_string("Y"),
        false_type: no,
        is_distributive: false,
    });
    let mut subst = TypeSubstitution::new();
    subst.insert(s_name, readonly_tuple);
    subst.insert(t_name, mutable_numbers);
    let instantiated = instantiate_type(&interner, cond_id, &subst);

    assert_eq!(
        evaluate_type(&interner, instantiated),
        no,
        "R<readonly [1, 2], number[]> should take the false branch"
    );
}

#[test]
fn readonly_target_controls_take_true_branch() {
    let interner = TypeInterner::new();
    let readonly_numbers = interner.readonly_array(TypeId::NUMBER);
    let mutable_numbers = interner.array(TypeId::NUMBER);
    let yes = interner.literal_string("Y");

    assert_eq!(
        conditional_result(&interner, readonly_numbers, readonly_numbers),
        yes,
        "readonly number[] extends readonly number[] should take the true branch"
    );
    assert_eq!(
        conditional_result(&interner, mutable_numbers, readonly_numbers),
        yes,
        "number[] extends readonly number[] should take the true branch"
    );
}

#[test]
fn direct_readonly_array_extends_mutable_array_takes_false_branch() {
    let interner = TypeInterner::new();
    let readonly_numbers = interner.readonly_array(TypeId::NUMBER);
    let mutable_numbers = interner.array(TypeId::NUMBER);
    let no = interner.literal_string("N");

    assert_eq!(
        conditional_result(&interner, readonly_numbers, mutable_numbers),
        no,
        "direct readonly number[] extends number[] should stay false"
    );
}
