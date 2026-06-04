#[test]
fn test_required_t_subtype_of_partial_t() {
    // Required<T> → Partial<T> should succeed: narrower to wider
    let interner = TypeInterner::new();
    let t_param = interner.intern(TypeData::TypeParameter(crate::TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    let required_t = make_homomorphic_mapped(
        &interner,
        t_param,
        Some(crate::MappedModifier::Remove),
        None,
    );
    let partial_t =
        make_homomorphic_mapped(&interner, t_param, Some(crate::MappedModifier::Add), None);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.check_subtype(required_t, partial_t).is_true(),
        "Required<T> SHOULD be subtype of Partial<T> (narrower to wider)"
    );
}

#[test]
fn test_partial_t_not_subtype_of_required_t() {
    // Partial<T> → Required<T> should fail: wider to narrower
    let interner = TypeInterner::new();
    let t_param = interner.intern(TypeData::TypeParameter(crate::TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    let partial_t =
        make_homomorphic_mapped(&interner, t_param, Some(crate::MappedModifier::Add), None);
    let required_t = make_homomorphic_mapped(
        &interner,
        t_param,
        Some(crate::MappedModifier::Remove),
        None,
    );

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        !checker.check_subtype(partial_t, required_t).is_true(),
        "Partial<T> should NOT be subtype of Required<T> (wider to narrower)"
    );
}

/// Helper to create a homomorphic mapped type with a filtering as-clause:
/// { [K in keyof source as source[K] extends `check_type` ? K : never]<modifiers>: source[K] }
fn make_filtering_mapped(
    interner: &TypeInterner,
    source: TypeId,
    check_type: TypeId,
    optional: Option<crate::MappedModifier>,
    readonly: Option<crate::MappedModifier>,
) -> TypeId {
    let k_name = interner.intern_string("K");
    let k_param = interner.intern(TypeData::TypeParameter(crate::TypeParamInfo {
        name: k_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let template = interner.intern(TypeData::IndexAccess(source, k_param));
    let constraint = interner.intern(TypeData::KeyOf(source));

    // Build the conditional: source[K] extends check_type ? K : never
    let check = interner.intern(TypeData::IndexAccess(source, k_param));
    let name_type = interner.conditional(crate::ConditionalType {
        check_type: check,
        extends_type: check_type,
        true_type: k_param,
        false_type: TypeId::NEVER,
        is_distributive: false,
    });

    interner.mapped(crate::MappedType {
        type_param: crate::TypeParamInfo {
            name: k_name,
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint,
        name_type: Some(name_type),
        template,
        optional_modifier: optional,
        readonly_modifier: readonly,
    })
}

#[test]
fn test_t_subtype_of_filter_t_no_modifier() {
    // T → Filter<T> (filtering as-clause, no modifier change) should succeed.
    // Filter<T> = { [K in keyof T as T[K] extends Function ? K : never]: T[K] }
    // All keys in Filter<T> are also keys of T with the same types.
    let interner = TypeInterner::new();
    let t_param = interner.intern(TypeData::TypeParameter(crate::TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    }));
    let function_type = interner.intern(TypeData::TypeParameter(crate::TypeParamInfo {
        name: interner.intern_string("Function"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    let filter_t = make_filtering_mapped(&interner, t_param, function_type, None, None);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.check_subtype(t_param, filter_t).is_true(),
        "T SHOULD be subtype of Filter<T> (filtering as-clause preserves keys)"
    );
}

#[test]
fn test_t_subtype_of_filter_t_with_optional() {
    // T → FilterInclOpt<T> (filtering + add optional) should succeed.
    // Adding optional makes the target wider, so T is still assignable.
    let interner = TypeInterner::new();
    let t_param = interner.intern(TypeData::TypeParameter(crate::TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    }));
    let function_type = interner.intern(TypeData::TypeParameter(crate::TypeParamInfo {
        name: interner.intern_string("Function"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    let filter_opt_t = make_filtering_mapped(
        &interner,
        t_param,
        function_type,
        Some(crate::MappedModifier::Add),
        None,
    );

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.check_subtype(t_param, filter_opt_t).is_true(),
        "T SHOULD be subtype of FilterInclOpt<T> (filtering + optional widens)"
    );
}

#[test]
fn test_t_not_subtype_of_filter_t_remove_optional() {
    // T → FilterExclOpt<T> (filtering + remove optional) should FAIL.
    // Removing optional means required properties that T might have as optional.
    let interner = TypeInterner::new();
    let t_param = interner.intern(TypeData::TypeParameter(crate::TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    }));
    let function_type = interner.intern(TypeData::TypeParameter(crate::TypeParamInfo {
        name: interner.intern_string("Function"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    let filter_required_t = make_filtering_mapped(
        &interner,
        t_param,
        function_type,
        Some(crate::MappedModifier::Remove),
        None,
    );

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        !checker.check_subtype(t_param, filter_required_t).is_true(),
        "T should NOT be subtype of FilterExclOpt<T> (-? makes target narrower)"
    );
}

/// Helper to create a homomorphic mapped type with a custom template:
/// { [K in keyof source]<modifiers>: template }
fn make_homomorphic_mapped_with_template(
    interner: &TypeInterner,
    source: TypeId,
    template: TypeId,
    optional: Option<crate::MappedModifier>,
    readonly: Option<crate::MappedModifier>,
) -> TypeId {
    let k_name = interner.intern_string("K");
    let constraint = interner.intern(TypeData::KeyOf(source));
    interner.mapped(crate::MappedType {
        type_param: crate::TypeParamInfo {
            name: k_name,
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint,
        name_type: None,
        template,
        optional_modifier: optional,
        readonly_modifier: readonly,
    })
}

#[test]
fn test_type_param_assignable_to_widened_template_mapped() {
    // type MyMap<T> = { [P in keyof T]: T[keyof T] }
    // U <: MyMap<U> should be TRUE
    //
    // T[keyof T] is the union of all value types, so for each property P,
    // T[P] is always assignable to T[keyof T] (a member of a union).
    let interner = TypeInterner::new();
    let u_name = interner.intern_string("U");
    let u_param = interner.intern(TypeData::TypeParameter(crate::TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Template: U[keyof U]
    let keyof_u = interner.intern(TypeData::KeyOf(u_param));
    let template = interner.intern(TypeData::IndexAccess(u_param, keyof_u));

    // { [P in keyof U]: U[keyof U] }
    let mapped = make_homomorphic_mapped_with_template(&interner, u_param, template, None, None);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.check_subtype(u_param, mapped).is_true(),
        "U should be assignable to {{ [P in keyof U]: U[keyof U] }}"
    );
}

#[test]
fn test_type_param_not_assignable_to_string_template_mapped() {
    // type StringMap<T> = { [P in keyof T]: string }
    // U <: StringMap<U> should be FALSE (U's values aren't necessarily string)
    let interner = TypeInterner::new();
    let u_name = interner.intern_string("U");
    let u_param = interner.intern(TypeData::TypeParameter(crate::TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // { [P in keyof U]: string }
    let mapped =
        make_homomorphic_mapped_with_template(&interner, u_param, TypeId::STRING, None, None);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        !checker.check_subtype(u_param, mapped).is_true(),
        "U should NOT be assignable to {{ [P in keyof U]: string }}"
    );
}

#[test]
fn test_type_param_assignable_to_identity_mapped() {
    // type Identity<T> = { [P in keyof T]: T[P] }
    // U <: Identity<U> should be TRUE (existing behavior, now also handled by general path)
    let interner = TypeInterner::new();
    let u_param = interner.intern(TypeData::TypeParameter(crate::TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    let identity_u = make_homomorphic_mapped(&interner, u_param, None, None);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.check_subtype(u_param, identity_u).is_true(),
        "U should be assignable to {{ [K in keyof U]: U[K] }}"
    );
}

#[test]
fn test_type_param_not_assignable_to_required_mapped() {
    // U <: Required<U> should be FALSE (Required removes optionality)
    let interner = TypeInterner::new();
    let u_param = interner.intern(TypeData::TypeParameter(crate::TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    let required_u = make_homomorphic_mapped(
        &interner,
        u_param,
        Some(crate::MappedModifier::Remove),
        None,
    );

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        !checker.check_subtype(u_param, required_u).is_true(),
        "U should NOT be assignable to Required<U> (-? removes optionality)"
    );
}

#[test]
fn test_evaluated_callable_assignable_to_all_any_application_via_display_alias() {
    // Regression: evaluated Callable (e.g. ICEP<unknown,unknown> stored as TypeData::Callable)
    // must be assignable to the same generic Application with all-any args (e.g. ICEP<any,any>).
    // Without the display_alias fallback in cache.rs, application_id() returns None for the
    // evaluated form and the variance fast path is skipped, causing structural expansion of
    // self-referential types that incorrectly fails. (circularlySimplifyingConditionalTypesNoCrash.ts)
    use crate::construction::TypeInterner;
    use crate::relations::subtype::SubtypeChecker;
    use crate::{CallSignature, CallableShape};

    let interner = TypeInterner::new();
    let base_lazy = interner.lazy(DefId(99));

    // Build a minimal callable: (x: unknown): unknown
    let param_atom = interner.intern_string("x");
    let sig = CallSignature {
        type_params: vec![],
        params: vec![crate::ParamInfo {
            name: Some(param_atom),
            type_id: TypeId::UNKNOWN,
            optional: false,
            rest: false,
        }],
        return_type: TypeId::UNKNOWN,
        this_type: None,
        type_predicate: None,
        is_method: false,
    };
    let evaluated_callable = interner.callable(CallableShape {
        call_signatures: vec![sig],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
        symbol: None,
        is_abstract: false,
    });

    // source_app_unknown = GenericFn<unknown> (the Application the callable was evaluated from)
    let source_app_unknown = interner.application(base_lazy, vec![TypeId::UNKNOWN]);
    // target_app_any = GenericFn<any>
    let target_app_any = interner.application(base_lazy, vec![TypeId::ANY]);

    // Tag the evaluated callable with the display alias so the variance fast path can recover it.
    interner.store_display_alias(evaluated_callable, source_app_unknown);

    let mut checker = SubtypeChecker::new(&interner);

    // KEY assertion: evaluated Callable <: GenericFn<any> must be True
    assert!(
        checker
            .check_subtype(evaluated_callable, target_app_any)
            .is_true(),
        "evaluated Callable tagged with display_alias GenericFn<unknown> should be assignable to GenericFn<any>"
    );

    // Sanity: plain Application <: Application with all-any also works
    assert!(
        checker
            .check_subtype(source_app_unknown, target_app_any)
            .is_true(),
        "GenericFn<unknown> should be assignable to GenericFn<any>"
    );
    assert!(
        checker
            .check_subtype(target_app_any, source_app_unknown)
            .is_true(),
        "GenericFn<any> should be assignable to GenericFn<unknown>"
    );

    // Unrelated base must not be affected
    let other_base = interner.lazy(DefId(100));
    let other_app = interner.application(other_base, vec![TypeId::ANY]);
    assert!(
        !checker
            .check_subtype(evaluated_callable, other_app)
            .is_true(),
        "callable should NOT be assignable to a different generic OtherFn<any>"
    );
}
