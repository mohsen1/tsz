#[test]
fn test_intersection_of_all_weak_types_is_still_weak() {
    // When ALL members of an intersection are weak types, the intersection IS weak.
    // e.g., `{ a?: number } & { b?: string }` is still a weak type.
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let a = interner.intern_string("a");
    let b = interner.intern_string("b");
    let c = interner.intern_string("c");

    let weak1 = interner.object(vec![PropertyInfo::opt(a, TypeId::NUMBER)]);
    let weak2 = interner.object(vec![PropertyInfo::opt(b, TypeId::STRING)]);

    let intersection = interner.intersection2(weak1, weak2);

    // Source with no overlapping properties should be rejected (weak type violation)
    let source = interner.object(vec![PropertyInfo::new(c, TypeId::BOOLEAN)]);
    assert!(!checker.is_assignable(source, intersection));
    assert!(matches!(
        checker.explain_failure(source, intersection),
        Some(SubtypeFailureReason::NoCommonProperties { .. })
    ));
}

#[test]
fn test_intersection_weak_type_source_matching_second_member_not_violation() {
    // Source has a property that matches the SECOND weak intersection member.
    // The weak-type check must consider all members' properties, not just the first.
    // `{ b: boolean } <: { a?: number } & { b?: string }` — b is in the second member,
    // so source shares a property name with the intersection. Not a weak-type violation.
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let a = interner.intern_string("a");
    let b = interner.intern_string("b");

    let weak1 = interner.object(vec![PropertyInfo::opt(a, TypeId::NUMBER)]);
    let weak2 = interner.object(vec![PropertyInfo::opt(b, TypeId::STRING)]);

    let intersection = interner.intersection2(weak1, weak2);

    // Source with property `b` matches weak2 — not a NoCommonProperties violation.
    // (There may be a type-mismatch for b: boolean vs b?: string, but that is
    // a different diagnostic, not TS2559.)
    let source_b = interner.object(vec![PropertyInfo::new(b, TypeId::BOOLEAN)]);
    assert!(
        !matches!(
            checker.explain_failure(source_b, intersection),
            Some(SubtypeFailureReason::NoCommonProperties { .. })
        ),
        "Source with property in second member must not trigger NoCommonProperties"
    );
}

#[test]
fn test_intersection_weak_type_three_members_no_common() {
    // Three-member weak intersection: source has no property in any member.
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let a = interner.intern_string("a");
    let b = interner.intern_string("b");
    let c = interner.intern_string("c");
    let z = interner.intern_string("z");

    let w1 = interner.object(vec![PropertyInfo::opt(a, TypeId::NUMBER)]);
    let w2 = interner.object(vec![PropertyInfo::opt(b, TypeId::STRING)]);
    let w3 = interner.object(vec![PropertyInfo::opt(c, TypeId::BOOLEAN)]);

    let intersection = interner.intersection(vec![w1, w2, w3]);

    let source = interner.object(vec![PropertyInfo::new(z, TypeId::NUMBER)]);
    assert!(!checker.is_assignable(source, intersection));
    assert!(matches!(
        checker.explain_failure(source, intersection),
        Some(SubtypeFailureReason::NoCommonProperties { .. })
    ));
}

#[test]
fn test_intersection_with_non_weak_member_not_weak_intersection() {
    // An intersection where at least one member is NOT weak is not a weak intersection.
    // `string & { a?: number }` is not weak because `string` is not weak.
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let a = interner.intern_string("a");
    let z = interner.intern_string("z");

    let weak = interner.object(vec![PropertyInfo::opt(a, TypeId::NUMBER)]);
    // Use a required-property object to make the intersection non-weak.
    let non_weak = interner.object(vec![PropertyInfo::new(z, TypeId::STRING)]);

    let intersection = interner.intersection2(weak, non_weak);

    let source_unrelated = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);
    // Non-weak intersection should not trigger NoCommonProperties.
    assert!(
        !matches!(
            checker.explain_failure(source_unrelated, intersection),
            Some(SubtypeFailureReason::NoCommonProperties { .. })
        ),
        "Non-weak intersection must not trigger NoCommonProperties"
    );
}

#[test]
fn test_weak_union_with_all_weak_members() {
    // Weak union: union of only weak types
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let a = interner.intern_string("a");
    let b = interner.intern_string("b");

    let weak_a = interner.object(vec![PropertyInfo::opt(a, TypeId::STRING)]);
    let weak_b = interner.object(vec![PropertyInfo::opt(b, TypeId::NUMBER)]);

    let weak_union = interner.union(vec![weak_a, weak_b]);

    let c = interner.intern_string("c");
    let source = interner.object(vec![PropertyInfo::new(c, TypeId::BOOLEAN)]);

    // Source with no overlap should be rejected
    assert!(!checker.is_assignable(source, weak_union));
}

#[test]
fn test_weak_union_with_non_weak_member_not_weak() {
    // Union with at least one non-weak member is not a weak union
    // Normal union typing applies: source must match at least one member
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let a = interner.intern_string("a");
    let b = interner.intern_string("b");

    let weak_type = interner.object(vec![PropertyInfo::opt(a, TypeId::STRING)]);
    let non_weak_type = interner.object(vec![PropertyInfo {
        name: b,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false, // Required property - NOT weak
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    }]);

    let union = interner.union(vec![weak_type, non_weak_type]);

    // Source that matches the non-weak member
    let source_matching_non_weak = interner.object(vec![PropertyInfo {
        name: b,
        type_id: TypeId::NUMBER, // Matches the required property
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    }]);

    // Should be accepted since source matches the non-weak member
    assert!(
        checker.is_assignable(source_matching_non_weak, union),
        "Source matching non-weak member should be assignable to union"
    );

    // Source that doesn't match any member should be rejected
    let c = interner.intern_string("c");
    let source_no_match = interner.object(vec![PropertyInfo::new(c, TypeId::BOOLEAN)]);

    // Should be rejected since source doesn't match any union member
    assert!(
        !checker.is_assignable(source_no_match, union),
        "Source not matching any union member should not be assignable"
    );
}

#[test]
fn test_global_object_type_exempt_from_weak_type_check() {
    // The global Object type (with its standard properties like constructor,
    // toString, hasOwnProperty, etc.) should be exempt from weak type checks.
    // This matches TypeScript behavior: Object is treated like {} for weak type
    // purposes. See TypeScript PR #16047.
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // Create Object-like type with the standard 7 properties
    let constructor = interner.intern_string("constructor");
    let to_string = interner.intern_string("toString");
    let to_locale_string = interner.intern_string("toLocaleString");
    let value_of = interner.intern_string("valueOf");
    let has_own_property = interner.intern_string("hasOwnProperty");
    let is_prototype_of = interner.intern_string("isPrototypeOf");
    let property_is_enumerable = interner.intern_string("propertyIsEnumerable");

    let object_type = interner.object(vec![
        PropertyInfo::new(constructor, TypeId::ANY),
        PropertyInfo::new(to_string, TypeId::ANY),
        PropertyInfo::new(to_locale_string, TypeId::ANY),
        PropertyInfo::new(value_of, TypeId::ANY),
        PropertyInfo::new(has_own_property, TypeId::ANY),
        PropertyInfo::new(is_prototype_of, TypeId::ANY),
        PropertyInfo::new(property_is_enumerable, TypeId::ANY),
    ]);

    // Weak target (all optional properties, no overlap with Object)
    let wings = interner.intern_string("wings");
    let legs = interner.intern_string("legs");
    let weak_target = interner.object(vec![
        PropertyInfo::opt(wings, TypeId::BOOLEAN),
        PropertyInfo::opt(legs, TypeId::NUMBER),
    ]);

    // Object should be assignable to weak type (exempt from weak type check)
    assert!(
        checker.is_assignable(object_type, weak_target),
        "Global Object type should be exempt from weak type check"
    );

    // But a non-Object source with no overlap should still be rejected
    let name = interner.intern_string("name");
    let non_object = interner.object(vec![PropertyInfo::new(name, TypeId::STRING)]);
    assert!(
        !checker.is_assignable(non_object, weak_target),
        "Non-Object source with no common properties should be rejected"
    );
}

#[test]
fn test_exact_optional_property_types_distinguishes_undefined_from_missing() {
    // With exact_optional_property_types=true, optional properties distinguish
    // between "missing" and "undefined"
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    checker.set_exact_optional_property_types(true);

    let x = interner.intern_string("x");

    // { x?: number }
    let optional_number = interner.object(vec![PropertyInfo::opt(x, TypeId::NUMBER)]);

    // { x: number | undefined }
    let number_or_undefined = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
    let _explicit_undefined = interner.object(vec![PropertyInfo::new(x, number_or_undefined)]);

    // With exact mode, these are NOT the same
    // { x?: number } is NOT assignable to { x: number | undefined }
    assert!(
        !checker.is_assignable(optional_number, _explicit_undefined),
        "Optional property should not be assignable to explicit undefined union in exact mode"
    );
    // { x: number | undefined } is NOT assignable to { x?: number }
    assert!(
        !checker.is_assignable(_explicit_undefined, optional_number),
        "Explicit undefined union should not be assignable to optional property in exact mode"
    );
}
