//! Regression tests for conditional `infer` patterns that read a numeric-index
//! property off a tuple (`T extends { 0: infer A } ? A : F`).
//!
//! These exercise `evaluate_conditional` through the bare `TypeInterner`, i.e.
//! the pure-evaluation path with **no** `QueryDatabase` attached — the same path
//! type-alias evaluation uses. Before the fix the infer-property resolver had no
//! tuple arm in that path, so every such conditional collapsed to its false
//! branch (or deferred) instead of matching the tuple element. tsc resolves the
//! numeric-index property against the tuple's apparent type, so the structural
//! rule is:
//!
//! When the extends pattern is `{ <n>: infer A }` and the check type is a tuple,
//! tsc reads the element at fixed index `<n>`; a guaranteed (non-optional) slot
//! binds `A`, an optional or absent slot fails a required pattern position.

use tsz_solver::computation::evaluate_conditional;
use tsz_solver::construction::TypeInterner;
use tsz_solver::type_handles::{
    ConditionalType, PropertyInfo, TupleElement, TypeId, TypeParamInfo,
};

fn infer_param(interner: &TypeInterner, name: &str) -> TypeId {
    interner.infer(TypeParamInfo {
        name: interner.intern_string(name),
        constraint: None,
        default: None,
        is_const: false,
        origin: tsz_solver::TypeParamOrigin::User,
    })
}

/// Build the extends pattern `{ <key>: infer <infer_name> }`.
fn numeric_key_infer_pattern(
    interner: &TypeInterner,
    key: &str,
    infer_name: &str,
    optional: bool,
) -> (TypeId, TypeId) {
    let inferred = infer_param(interner, infer_name);
    let mut prop = PropertyInfo::new(interner.intern_string(key), inferred);
    prop.optional = optional;
    let pattern = interner.object(vec![prop]);
    (pattern, inferred)
}

/// `T extends { 0: infer A } ? A : false_type`, evaluated with no query database.
fn eval_numeric_key_conditional(
    interner: &TypeInterner,
    check_type: TypeId,
    key: &str,
    optional: bool,
    false_type: TypeId,
) -> TypeId {
    let (extends_type, inferred) = numeric_key_infer_pattern(interner, key, "Elem", optional);
    evaluate_conditional(
        interner,
        &ConditionalType {
            check_type,
            extends_type,
            true_type: inferred,
            false_type,
            is_distributive: false,
        },
    )
}

#[test]
fn numeric_key_infer_binds_fixed_tuple_element() {
    // `[9, 8] extends { 0: infer A } ? A : 'no'` → A = 9.
    let interner = TypeInterner::new();
    let nine = interner.literal_number(9.0);
    let eight = interner.literal_number(8.0);
    let tuple = interner.tuple(vec![TupleElement::fixed(nine), TupleElement::fixed(eight)]);
    let no = interner.literal_string("no");

    let result = eval_numeric_key_conditional(&interner, tuple, "0", false, no);
    assert_eq!(
        result,
        nine,
        "index 0 of [9, 8] should bind the first element, got {:?}",
        interner.lookup(result)
    );

    // Index 1 binds the second element.
    let result1 = eval_numeric_key_conditional(&interner, tuple, "1", false, no);
    assert_eq!(
        result1, eight,
        "index 1 of [9, 8] should bind the second element"
    );
}

#[test]
fn numeric_key_infer_binds_through_readonly_wrapper() {
    // `readonly [9, 8] extends { 0: infer A } ? A : 'no'` → A = 9.
    let interner = TypeInterner::new();
    let nine = interner.literal_number(9.0);
    let eight = interner.literal_number(8.0);
    let tuple = interner.tuple(vec![TupleElement::fixed(nine), TupleElement::fixed(eight)]);
    let readonly_tuple = interner.readonly_type(tuple);
    let no = interner.literal_string("no");

    let result = eval_numeric_key_conditional(&interner, readonly_tuple, "0", false, no);
    assert_eq!(
        result,
        nine,
        "readonly tuples must unwrap so index 0 still binds, got {:?}",
        interner.lookup(result)
    );
}

#[test]
fn required_numeric_key_rejects_optional_slot() {
    // `[number?] extends { 0: infer A } ? A : 'no'` → false branch ('no'),
    // because an optional slot is not a guaranteed property.
    let interner = TypeInterner::new();
    let number = TypeId::NUMBER;
    let mut optional_elem = TupleElement::fixed(number);
    optional_elem.optional = true;
    let tuple = interner.tuple(vec![optional_elem]);
    let no = interner.literal_string("no");

    let result = eval_numeric_key_conditional(&interner, tuple, "0", false, no);
    assert_eq!(
        result, no,
        "a required numeric-key pattern must reject an optional tuple slot"
    );
}

#[test]
fn required_numeric_key_rejects_out_of_range_index() {
    // `[42] extends { 1: infer A } ? A : 'no'` → false branch ('no').
    let interner = TypeInterner::new();
    let forty_two = interner.literal_number(42.0);
    let tuple = interner.tuple(vec![TupleElement::fixed(forty_two)]);
    let no = interner.literal_string("no");

    let result = eval_numeric_key_conditional(&interner, tuple, "1", false, no);
    assert_eq!(
        result, no,
        "index 1 of [42] is absent and must take the false branch"
    );
}

#[test]
fn numeric_key_infer_descends_into_fixed_rest_spread() {
    // `[boolean, ...[number, string]] extends { 1: infer A } ? A : 'no'` → number.
    let interner = TypeInterner::new();
    let number = TypeId::NUMBER;
    let string = TypeId::STRING;
    let inner = interner.tuple(vec![
        TupleElement::fixed(number),
        TupleElement::fixed(string),
    ]);
    let tuple = interner.tuple(vec![
        TupleElement::fixed(TypeId::BOOLEAN),
        TupleElement::rest(inner),
    ]);
    let no = interner.literal_string("no");

    let result = eval_numeric_key_conditional(&interner, tuple, "1", false, no);
    assert_eq!(
        result,
        number,
        "index 1 should descend into the fixed rest spread and bind number, got {:?}",
        interner.lookup(result)
    );
}

#[test]
fn distributive_numeric_key_infer_preserves_per_variant() {
    // `([1, 2] | ['x']) extends { 0: infer A } ? A : never` distributes to
    // `1 | 'x'`. Built as a distributive conditional over the union.
    let interner = TypeInterner::new();
    let one = interner.literal_number(1.0);
    let two = interner.literal_number(2.0);
    let x = interner.literal_string("x");
    let t12 = interner.tuple(vec![TupleElement::fixed(one), TupleElement::fixed(two)]);
    let tx = interner.tuple(vec![TupleElement::fixed(x)]);
    let union = interner.union(vec![t12, tx]);

    let (extends_type, inferred) = numeric_key_infer_pattern(&interner, "0", "Elem", false);
    let result = evaluate_conditional(
        &interner,
        &ConditionalType {
            check_type: union,
            extends_type,
            true_type: inferred,
            false_type: TypeId::NEVER,
            is_distributive: true,
        },
    );

    let expected = interner.union(vec![one, x]);
    assert_eq!(
        result,
        expected,
        "distribution should yield 1 | 'x', got {:?}",
        interner.lookup(result)
    );
}
