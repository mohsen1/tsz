// Deferred type-level index keys must keep `O[I]` deferred instead of
// collapsing a missing-key lookup to `undefined`.
//
// Structural rule: a "missing key → undefined" answer is only valid for a
// *concrete* key type. An index that is still a deferred type-level
// computation — an alias `Application` the resolver could not expand, an
// unresolved `Lazy` reference, or any form still carrying free type
// variables — may instantiate to a real key later, so tsc keeps the access
// deferred (`isGenericIndexType`). Witness: ts-toolbelt
// `_Sub<N1, N2> = {0: SubPositive<N1, N2>, 1: SubNegative<N1, N2>}[_IsNegative<N2>]`
// cold-start emitted a false `TS2344` "Type 'undefined' does not satisfy the
// constraint" when `_IsNegative<N2>` could not be resolved yet.

#[test]
fn test_object_index_with_unresolved_application_key_defers() {
    let interner = TypeInterner::new();

    // {gamma: string, delta: number}[UnknownAlias<Zq>] — alias unresolvable.
    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("gamma"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("delta"), TypeId::NUMBER),
    ]);
    let (_, zq_param) = test_type_param(&interner, "Zq");
    let unresolved_base = interner.lazy(DefId(987_001));
    let app_index = interner.application(unresolved_base, vec![zq_param]);

    let result = evaluate_index_access(&interner, obj, app_index);
    assert_ne!(
        result,
        TypeId::UNDEFINED,
        "deferred application index must not collapse to undefined"
    );
    assert!(
        matches!(
            interner.lookup(result),
            Some(TypeData::IndexAccess(_, _))
        ),
        "expected a deferred IndexAccess, got {:?}",
        interner.lookup(result)
    );
}

#[test]
fn test_numeric_branch_object_index_with_unresolved_application_key_defers() {
    let interner = TypeInterner::new();

    // The ts-toolbelt `_Sub` dispatch shape with renamed binders:
    // {0: P1, 1: P2}[Disp<P2>] where `Disp` is unresolvable.
    let (_, p1) = test_type_param(&interner, "Qx1");
    let (_, p2) = test_type_param(&interner, "Qx2");
    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("0"), p1),
        PropertyInfo::new(interner.intern_string("1"), p2),
    ]);
    let dispatch_base = interner.lazy(DefId(987_002));
    let dispatch_index = interner.application(dispatch_base, vec![p2]);

    let result = evaluate_index_access(&interner, obj, dispatch_index);
    assert_ne!(result, TypeId::UNDEFINED);
    assert!(matches!(
        interner.lookup(result),
        Some(TypeData::IndexAccess(_, _))
    ));
}

#[test]
fn test_object_index_with_unresolved_lazy_key_defers() {
    let interner = TypeInterner::new();

    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("epsilon"),
        TypeId::BOOLEAN,
    )]);
    let lazy_index = interner.lazy(DefId(987_003));

    let result = evaluate_index_access(&interner, obj, lazy_index);
    assert_ne!(result, TypeId::UNDEFINED);
    assert!(matches!(
        interner.lookup(result),
        Some(TypeData::IndexAccess(_, _))
    ));
}

#[test]
fn test_tuple_index_with_unresolved_application_key_defers() {
    let interner = TypeInterner::new();

    let tuple = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    }]);
    let (_, key_param) = test_type_param(&interner, "Kz");
    let app_index = interner.application(interner.lazy(DefId(987_004)), vec![key_param]);

    let result = evaluate_index_access(&interner, tuple, app_index);
    assert_ne!(result, TypeId::UNDEFINED);
}

#[test]
fn test_object_index_with_concrete_missing_key_still_undefined() {
    let interner = TypeInterner::new();

    // Negative/fallback case: a concrete literal key that misses must keep
    // producing `undefined` (the checker derives element-access diagnostics
    // from it).
    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("zeta"),
        TypeId::STRING,
    )]);
    let missing_key = interner.literal_string("eta");

    let result = evaluate_index_access(&interner, obj, missing_key);
    assert_eq!(result, TypeId::UNDEFINED);
}

#[test]
fn test_object_index_with_resolvable_application_key_still_resolves() {
    use crate::relations::subtype::TypeEnvironment;

    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    // Alias `Pick1` resolves to the literal "theta"; {theta: number}[Pick1]
    // must still evaluate through the alias to `number`.
    let theta_key = interner.literal_string("theta");
    let def_id = DefId(11);
    env.insert_def(def_id, theta_key);

    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("theta"),
        TypeId::NUMBER,
    )]);
    let lazy_index = interner.lazy(def_id);

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let index_access = interner.index_access(obj, lazy_index);
    let result = evaluator.evaluate(index_access);
    assert_eq!(result, TypeId::NUMBER);
}
