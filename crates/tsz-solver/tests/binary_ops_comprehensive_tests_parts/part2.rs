#[test]
fn test_optional_type_param_or_object_does_not_reduce_for_primitive_fallback() {
    // `(D | undefined) || "hello"` should NOT produce a type assignable to `object`
    // because `"hello"` (string literal) is a primitive.
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);

    let d = make_unconstrained_type_param(&interner, "D");
    let d_or_undef = interner.union2(d, TypeId::UNDEFINED);

    let result = eval.evaluate(d_or_undef, TypeId::STRING, "||");
    let result_id = match result {
        BinaryOpResult::Success(t) => t,
        _ => panic!("Expected Success"),
    };

    use crate::relations::subtype::SubtypeChecker;
    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        !checker.is_subtype_of(result_id, TypeId::OBJECT),
        "Expected (D | undefined) || string to NOT be assignable to `object` \
         (string is a primitive), but got {result_id:?} which appears to be assignable."
    );
}

#[test]
fn test_string_constrained_type_param_or_empty_object_not_assignable_to_object() {
    // `(D extends string | undefined) || {}` should NOT be assignable to `object`
    // because D is constrained to string (a primitive), so `D | {}` contains
    // a potential primitive even after the || reduction.
    //
    // (TypeScript itself would error here: D extends string means D could
    // be "hello", and "hello" is not an `object`.)
    let interner = TypeInterner::new();
    let eval = BinaryOpEvaluator::new(&interner);

    let d_str = make_string_constrained_type_param(&interner, "D");
    let d_or_undef = interner.union2(d_str, TypeId::UNDEFINED);
    let empty_obj = interner.object(vec![]);

    let result = eval.evaluate(d_or_undef, empty_obj, "||");
    let result_id = match result {
        BinaryOpResult::Success(t) => t,
        _ => panic!("Expected Success"),
    };

    use crate::relations::subtype::SubtypeChecker;
    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        !checker.is_subtype_of(result_id, TypeId::OBJECT),
        "Expected (D extends string | undefined) || {{}} to NOT be assignable to `object`, \
         but got {result_id:?} which appears to be assignable."
    );
}
