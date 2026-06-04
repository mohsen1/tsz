#[test]
fn test_composed_extract_deferred_when_check_is_conditional() {
    // Extract<Extract<T, Foo>, Bar> should be deferred (not eagerly resolved to never).
    //
    // Inner: T extends Foo ? T : never → deferred (T is type param)
    // Outer: Inner extends Bar ? Inner : never → should also defer
    //
    // Previously, the outer was eagerly resolved to the false branch (never) because
    // the evaluator didn't recognize that a Conditional check_type containing type
    // params should be deferred.
    let interner = TypeInterner::new();

    let t_param = interner.type_param(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    });

    let foo = interner.object(vec![PropertyInfo::new(
        interner.intern_string("foo"),
        TypeId::STRING,
    )]);

    let bar = interner.object(vec![PropertyInfo::new(
        interner.intern_string("bar"),
        TypeId::STRING,
    )]);

    // Inner conditional: T extends Foo ? T : never (Extract<T, Foo>)
    let inner_cond = interner.conditional(ConditionalType {
        check_type: t_param,
        extends_type: foo,
        true_type: t_param,
        false_type: TypeId::NEVER,
        is_distributive: true,
    });

    // Outer conditional: Inner extends Bar ? Inner : never (Extract<Inner, Bar>)
    let outer_cond = interner.conditional(ConditionalType {
        check_type: inner_cond,
        extends_type: bar,
        true_type: inner_cond,
        false_type: TypeId::NEVER,
        is_distributive: true,
    });

    let result = evaluate_type(&interner, outer_cond);

    // The result should NOT be NEVER — it should be a deferred conditional
    assert_ne!(
        result,
        TypeId::NEVER,
        "Extract<Extract<T, Foo>, Bar> should be deferred, not resolved to never"
    );
    // It should be a Conditional type
    assert!(
        matches!(interner.lookup(result), Some(TypeData::Conditional(_))),
        "Result should be a deferred conditional type"
    );
}

#[test]
fn test_composed_extract_not_assignable_to_missing_property() {
    // Extract<Extract<T, Foo>, Bar> should NOT be assignable to { foo: string; bat: string }
    // because the constraint T & Foo & Bar doesn't have 'bat'.
    let interner = TypeInterner::new();

    let t_param = interner.type_param(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    });

    let foo = interner.object(vec![PropertyInfo::new(
        interner.intern_string("foo"),
        TypeId::STRING,
    )]);

    let bar = interner.object(vec![PropertyInfo::new(
        interner.intern_string("bar"),
        TypeId::STRING,
    )]);

    let foo_bat = interner.object(vec![
        PropertyInfo::new(interner.intern_string("foo"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("bat"), TypeId::STRING),
    ]);

    // Build Extract<Extract<T, Foo>, Bar>
    let inner_cond = interner.conditional(ConditionalType {
        check_type: t_param,
        extends_type: foo,
        true_type: t_param,
        false_type: TypeId::NEVER,
        is_distributive: true,
    });

    let outer_cond = interner.conditional(ConditionalType {
        check_type: inner_cond,
        extends_type: bar,
        true_type: inner_cond,
        false_type: TypeId::NEVER,
        is_distributive: true,
    });

    let evaluated = evaluate_type(&interner, outer_cond);
    let mut checker = SubtypeChecker::new(&interner);
    let result = checker.is_subtype_of(evaluated, foo_bat);

    assert!(
        !result,
        "Extract<Extract<T, Foo>, Bar> should NOT be assignable to {{ foo, bat }}"
    );
}

#[test]
fn test_composed_extract_assignable_to_matching_properties() {
    // Extract<Extract<T, Foo>, Bar> SHOULD be assignable to { foo: string; bar: string }
    // because the constraint T & Foo & Bar has both 'foo' and 'bar'.
    let interner = TypeInterner::new();

    let t_param = interner.type_param(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    });

    let foo = interner.object(vec![PropertyInfo::new(
        interner.intern_string("foo"),
        TypeId::STRING,
    )]);

    let bar = interner.object(vec![PropertyInfo::new(
        interner.intern_string("bar"),
        TypeId::STRING,
    )]);

    let foo_bar = interner.object(vec![
        PropertyInfo::new(interner.intern_string("foo"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("bar"), TypeId::STRING),
    ]);

    // Build Extract<Extract<T, Foo>, Bar>
    let inner_cond = interner.conditional(ConditionalType {
        check_type: t_param,
        extends_type: foo,
        true_type: t_param,
        false_type: TypeId::NEVER,
        is_distributive: true,
    });

    let outer_cond = interner.conditional(ConditionalType {
        check_type: inner_cond,
        extends_type: bar,
        true_type: inner_cond,
        false_type: TypeId::NEVER,
        is_distributive: true,
    });

    let evaluated = evaluate_type(&interner, outer_cond);
    let mut checker = SubtypeChecker::new(&interner);
    let result = checker.is_subtype_of(evaluated, foo_bar);

    assert!(
        result,
        "Extract<Extract<T, Foo>, Bar> SHOULD be assignable to {{ foo, bar }}"
    );
}
