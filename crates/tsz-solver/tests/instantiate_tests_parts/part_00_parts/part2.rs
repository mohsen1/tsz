#[test]
fn test_instantiate_string_intrinsic_lowercase_with_union() {
    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");

    // Create Lowercase<T>
    let type_param_t = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let lowercase = interner.intern(TypeData::StringIntrinsic {
        kind: StringIntrinsicKind::Lowercase,
        type_arg: type_param_t,
    });

    // Substitute T = "ABC" | "XYZ" -> should evaluate to "abc" | "xyz"
    let abc_lit = interner.literal_string("ABC");
    let xyz_lit = interner.literal_string("XYZ");
    let union = interner.union(vec![abc_lit, xyz_lit]);
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, union);
    let result = instantiate_type(&interner, lowercase, &subst);

    // The result should be a union of "abc" | "xyz"
    let abc_lower = interner.literal_string("abc");
    let xyz_lower = interner.literal_string("xyz");
    let expected = interner.union(vec![abc_lower, xyz_lower]);
    assert_eq!(result, expected);
}

#[test]
fn test_instantiate_string_intrinsic_capitalize() {
    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");

    // Create Capitalize<T>
    let type_param_t = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let capitalize = interner.intern(TypeData::StringIntrinsic {
        kind: StringIntrinsicKind::Capitalize,
        type_arg: type_param_t,
    });

    // Substitute T = "hello" -> should evaluate to "Hello"
    let hello_lit = interner.literal_string("hello");
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, hello_lit);
    let result = instantiate_type(&interner, capitalize, &subst);

    let expected = interner.literal_string("Hello");
    assert_eq!(result, expected);
}

#[test]
fn test_instantiate_string_intrinsic_uncapitalize() {
    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");

    // Create Uncapitalize<T>
    let type_param_t = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let uncapitalize = interner.intern(TypeData::StringIntrinsic {
        kind: StringIntrinsicKind::Uncapitalize,
        type_arg: type_param_t,
    });

    // Substitute T = "Hello" -> should evaluate to "hello"
    let hello_lit = interner.literal_string("Hello");
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, hello_lit);
    let result = instantiate_type(&interner, uncapitalize, &subst);

    let expected = interner.literal_string("hello");
    assert_eq!(result, expected);
}

#[test]
fn test_instantiate_string_intrinsic_with_template_literal() {
    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");

    // Create `get${T}` template literal
    let type_param_t = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("get")),
        TemplateSpan::Type(type_param_t),
    ]);

    // Create Uppercase<`get${T}`>
    let uppercase = interner.intern(TypeData::StringIntrinsic {
        kind: StringIntrinsicKind::Uppercase,
        type_arg: template,
    });

    // Substitute T = "Name" -> should evaluate to "GETNAME"
    let name_lit = interner.literal_string("Name");
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, name_lit);
    let result = instantiate_type(&interner, uppercase, &subst);

    let expected = interner.literal_string("GETNAME");
    assert_eq!(result, expected);
}

#[test]
fn test_instantiate_string_intrinsic_preserves_type_param() {
    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    // Create Uppercase<T>
    let type_param_t = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let uppercase = interner.intern(TypeData::StringIntrinsic {
        kind: StringIntrinsicKind::Uppercase,
        type_arg: type_param_t,
    });

    // Substitute U = "hello" (T is not substituted)
    let hello_lit = interner.literal_string("hello");
    let mut subst = TypeSubstitution::new();
    subst.insert(u_name, hello_lit);
    let result = instantiate_type(&interner, uppercase, &subst);

    // T should stay as is - result should still be StringIntrinsic<T>
    if let Some(TypeData::StringIntrinsic { kind, type_arg }) = interner.lookup(result) {
        assert_eq!(kind, StringIntrinsicKind::Uppercase);
        // type_arg should still be T
        if let Some(TypeData::TypeParameter(info)) = interner.lookup(type_arg) {
            assert_eq!(info.name, t_name);
        } else {
            panic!("Expected type parameter T in StringIntrinsic");
        }
    } else {
        panic!("Expected StringIntrinsic type");
    }
}

/// Regression test for type parameter shadowing in class methods.
///
/// When a Callable type has multiple call signatures and one signature shadows
/// a type parameter (e.g., class `B<T>` has method `bar<T>`), the visiting cache
/// in `TypeInstantiator` must not leak across signatures. Otherwise, a `TypeParameter`
/// cached as "unsubstituted" (because it was shadowed in bar's scope) would
/// incorrectly remain unsubstituted when processing foo's scope.
///
/// Repro: `class B<T, U> { foo(t: T, u: U) {}; bar<T>(t: T, u: U) {} }`
/// `new B<string, number>().foo('hello', 1)` should not error.
#[test]
fn test_callable_shadowed_type_param_no_cache_leak() {
    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let t_param = TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let u_param = TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));
    let u_type = interner.intern(TypeData::TypeParameter(u_param));

    // foo(t: T, u: U) — uses class-level T and U, no own type params
    let foo_sig = CallSignature {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("t")),
                type_id: t_type,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("u")),
                type_id: u_type,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_method: true,
    };

    // bar<T>(t: T, u: U) — shadows class T with its own T
    let bar_sig = CallSignature {
        type_params: vec![t_param],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("t")),
                type_id: t_type,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("u")),
                type_id: u_type,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_method: true,
    };

    // Callable with both signatures. bar is listed first to trigger the bug:
    // when bar is instantiated first, T gets cached as unsubstituted (shadowed).
    // Then when foo is instantiated, the stale cache would return T instead of string.
    let callable = interner.callable(CallableShape {
        call_signatures: vec![bar_sig, foo_sig],
        construct_signatures: vec![],
        properties: vec![],
        ..Default::default()
    });

    // Substitute T=string, U=number (as if `new B<string, number>()`)
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, TypeId::STRING);
    subst.insert(u_name, TypeId::NUMBER);
    let result = instantiate_type(&interner, callable, &subst);

    // Verify the result
    if let Some(TypeData::Callable(shape_id)) = interner.lookup(result) {
        let shape = interner.callable_shape(shape_id);
        assert_eq!(shape.call_signatures.len(), 2);

        // bar's signature (index 0): T is shadowed, so params should be (T, number)
        let bar_result = &shape.call_signatures[0];
        assert_eq!(bar_result.type_params.len(), 1); // still has own <T>
        assert_eq!(bar_result.params[0].type_id, t_type); // T stays as TypeParameter
        assert_eq!(bar_result.params[1].type_id, TypeId::NUMBER); // U → number

        // foo's signature (index 1): T is NOT shadowed, so params should be (string, number)
        let foo_result = &shape.call_signatures[1];
        assert_eq!(foo_result.type_params.len(), 0); // no own type params
        assert_eq!(
            foo_result.params[0].type_id,
            TypeId::STRING,
            "foo's T param should be substituted to string, not left as TypeParameter"
        );
        assert_eq!(foo_result.params[1].type_id, TypeId::NUMBER); // U → number
    } else {
        panic!("Expected callable type, got {:?}", interner.lookup(result));
    }
}
