// ===================================================================
// The `const` modifier must not fragment canonical type-parameter identity
// (#13609 — the same family as the dropped `default` and the `Infer` arm)
// ===================================================================

// `tsc` identifies a type parameter by itself (its name and the *shape* of its
// constraint), never by the `const` modifier. `const` (`<const R>`) is an
// inference-site modifier that preserves literal types at call sites
// (`compareTypeParametersIdentical` compares constraints only) — it is erased
// from the parameter's type identity, exactly like `default`. `TypeParamInfo`
// derives `Eq`/`Hash` over `is_const`, so a free reference to a `const`
// parameter and one to its non-`const` twin intern to distinct `TypeId`s; both
// must canonicalize to one identity or the relation's reflexive/identity fast
// path fragments.
#[test]
fn canonicalize_free_type_param_ignores_const_modifier() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    let name = interner.intern_string("R");
    let constraint = Some(TypeId::STRING);
    let make = |is_const| {
        interner.type_param(TypeParamInfo {
            name,
            constraint,
            default: None,
            is_const,
            origin: crate::types::TypeParamOrigin::User,
        })
    };

    let r_const = make(true);
    let r_plain = make(false);

    assert_ne!(
        r_const, r_plain,
        "precondition: interning keeps the `const` modifier distinct"
    );

    let mut c1 = Canonicalizer::new(&interner, &env);
    let mut c2 = Canonicalizer::new(&interner, &env);
    assert_eq!(
        c1.canonicalize(r_const),
        c2.canonicalize(r_plain),
        "the `const` modifier must not fragment canonical identity"
    );

    // The `const` and `default` drops compose: a `const` param with a captured
    // default still collapses onto its bare twin.
    let r_const_default = interner.type_param(TypeParamInfo {
        name,
        constraint,
        default: Some(TypeId::NUMBER),
        is_const: true,
        origin: crate::types::TypeParamOrigin::User,
    });
    let mut c3 = Canonicalizer::new(&interner, &env);
    let mut c4 = Canonicalizer::new(&interner, &env);
    assert_eq!(
        c3.canonicalize(r_const_default),
        c4.canonicalize(r_plain),
        "dropping `const` and `default` compose to one identity"
    );
}

// The same identity rule applies when the type parameter is declared in a
// signature's type-parameter list: two generic function types that differ only
// in a type parameter's `const` modifier must canonicalize to one identity, so
// the relation's reflexive short-circuit holds for `<const R extends X>() => R`
// against `<R extends X>() => R`.
#[test]
fn canonicalize_function_type_param_list_ignores_const_modifier() {
    use crate::types::{FunctionShape, ParamInfo};
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    let r = interner.intern_string("R");
    let make = |is_const: bool| {
        let body = interner.type_param(TypeParamInfo {
            name: r,
            constraint: Some(TypeId::STRING),
            default: None,
            is_const,
            origin: crate::types::TypeParamOrigin::User,
        });
        interner.function(FunctionShape {
            type_params: vec![TypeParamInfo {
                name: r,
                constraint: Some(TypeId::STRING),
                default: None,
                is_const,
                origin: crate::types::TypeParamOrigin::User,
            }],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: body,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: body,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        })
    };

    let const_fn = make(true);
    let plain_fn = make(false);
    assert_ne!(
        const_fn, plain_fn,
        "precondition: interning keeps the `const` modifier distinct"
    );

    let mut c1 = Canonicalizer::new(&interner, &env);
    let mut c2 = Canonicalizer::new(&interner, &env);
    assert_eq!(
        c1.canonicalize(const_fn),
        c2.canonicalize(plain_fn),
        "the `const` modifier in a signature list must not fragment identity"
    );
}

// The `Infer` arm follows the same rule: `infer R` differing only in the
// `const` modifier must canonicalize identically, while genuine distinctions
// (name, constraint) stay distinct. Anti-hardcoding: the rule keys on the
// structural `is_const` flag, not on any binder name.
#[test]
fn canonicalize_infer_param_ignores_const_modifier() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    let name = interner.intern_string("R");
    let constraint = Some(TypeId::STRING);
    let make_infer = |is_const| {
        interner.infer(TypeParamInfo {
            name,
            constraint,
            default: None,
            is_const,
            origin: crate::types::TypeParamOrigin::User,
        })
    };

    let infer_const = make_infer(true);
    let infer_plain = make_infer(false);
    assert_ne!(
        infer_const, infer_plain,
        "precondition: interning keeps the `const` modifier distinct"
    );

    let mut c1 = Canonicalizer::new(&interner, &env);
    let mut c2 = Canonicalizer::new(&interner, &env);
    assert_eq!(
        c1.canonicalize(infer_const),
        c2.canonicalize(infer_plain),
        "an infer parameter's `const` modifier must not fragment canonical identity"
    );

    // Negative control: dropping `const` must not merge genuinely different
    // parameters (different constraint shape stays distinct).
    let infer_const_num = interner.infer(TypeParamInfo {
        name,
        constraint: Some(TypeId::NUMBER),
        default: None,
        is_const: true,
        origin: crate::types::TypeParamOrigin::User,
    });
    let mut c3 = Canonicalizer::new(&interner, &env);
    let mut c4 = Canonicalizer::new(&interner, &env);
    assert_ne!(
        c3.canonicalize(infer_plain),
        c4.canonicalize(infer_const_num),
        "a genuinely different constraint stays distinct after dropping `const`"
    );
}

// ===================================================================
// Value-name axis: parameter names / tuple labels / index-key names /
// predicate target identifiers are cosmetic and must not fragment
// canonical structural identity (#13609, value-name analogue of #14096).
// ===================================================================

#[test]
fn canonicalize_function_value_param_names_alpha_equivalent() {
    use crate::types::{FunctionShape, ParamInfo};
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    // `(<name>: string) => number` — only the value-parameter name varies.
    let make = |name: &str| {
        interner.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.intern_string(name)),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::NUMBER,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        })
    };

    let mut c1 = Canonicalizer::new(&interner, &env);
    let mut c2 = Canonicalizer::new(&interner, &env);
    assert_eq!(
        c1.canonicalize(make("a")),
        c2.canonicalize(make("verbose_name")),
        "functions differing only in a value-parameter name are the same type \
         and must share one canonical form"
    );
}

#[test]
fn canonicalize_function_param_type_and_arity_stay_distinct() {
    // Negative control: dropping the name must not merge functions whose
    // parameter *types*, arity, or optional/rest flags differ.
    use crate::types::{FunctionShape, ParamInfo};
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    let one = |ty: TypeId, optional: bool, rest: bool| {
        interner.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("p")),
                type_id: ty,
                optional,
                rest,
            }],
            this_type: None,
            return_type: TypeId::NUMBER,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        })
    };

    let string_param = one(TypeId::STRING, false, false);
    let number_param = one(TypeId::NUMBER, false, false);
    let optional_param = one(TypeId::STRING, true, false);

    let mut c1 = Canonicalizer::new(&interner, &env);
    let mut c2 = Canonicalizer::new(&interner, &env);
    let mut c3 = Canonicalizer::new(&interner, &env);
    let cs = c1.canonicalize(string_param);
    let cn = c2.canonicalize(number_param);
    let co = c3.canonicalize(optional_param);
    assert_ne!(cs, cn, "different parameter types must stay distinct");
    assert_ne!(cs, co, "optional vs required must stay distinct");
}

#[test]
fn canonicalize_tuple_labels_alpha_equivalent() {
    use crate::types::TupleElement;
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    // `[<label>: number, number]` vs an unlabeled `[number, number]`.
    let labeled = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: Some(interner.intern_string("first")),
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: Some(interner.intern_string("second")),
            optional: false,
            rest: false,
        },
    ]);
    let unlabeled = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let mut c1 = Canonicalizer::new(&interner, &env);
    let mut c2 = Canonicalizer::new(&interner, &env);
    assert_eq!(
        c1.canonicalize(labeled),
        c2.canonicalize(unlabeled),
        "tuple labels are cosmetic and must not fragment canonical identity"
    );
}

#[test]
fn canonicalize_tuple_optional_and_element_type_stay_distinct() {
    // Negative control: labels drop, but optionality and element type are
    // identity-bearing.
    use crate::types::TupleElement;
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    let elem = |ty: TypeId, optional: bool| {
        interner.tuple(vec![TupleElement {
            type_id: ty,
            name: Some(interner.intern_string("x")),
            optional,
            rest: false,
        }])
    };

    let mut c1 = Canonicalizer::new(&interner, &env);
    let mut c2 = Canonicalizer::new(&interner, &env);
    let mut c3 = Canonicalizer::new(&interner, &env);
    let req_num = c1.canonicalize(elem(TypeId::NUMBER, false));
    let opt_num = c2.canonicalize(elem(TypeId::NUMBER, true));
    let req_str = c3.canonicalize(elem(TypeId::STRING, false));
    assert_ne!(req_num, opt_num, "optional element must stay distinct");
    assert_ne!(
        req_num, req_str,
        "different element type must stay distinct"
    );
}

#[test]
fn canonicalize_index_signature_key_name_alpha_equivalent() {
    use crate::types::{IndexSignature, ObjectShape};
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    // `{ [<key>: string]: number }` — only the cosmetic key name varies.
    let make = |key_name: &str| {
        interner.object_with_index(ObjectShape {
            properties: vec![],
            string_index: Some(IndexSignature {
                key_type: TypeId::STRING,
                value_type: TypeId::NUMBER,
                readonly: false,
                param_name: Some(interner.intern_string(key_name)),
            }),
            number_index: None,
            symbol_index: None,
            symbol: None,
            flags: Default::default(),
        })
    };

    let mut c1 = Canonicalizer::new(&interner, &env);
    let mut c2 = Canonicalizer::new(&interner, &env);
    assert_eq!(
        c1.canonicalize(make("k")),
        c2.canonicalize(make("key")),
        "index-signature key names are cosmetic and must not fragment identity"
    );
}

#[test]
fn canonicalize_index_signature_readonly_and_value_stay_distinct() {
    // Negative control: key name drops, but readonly and value type are
    // identity-bearing.
    use crate::types::{IndexSignature, ObjectShape};
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    let make = |value: TypeId, readonly: bool| {
        interner.object_with_index(ObjectShape {
            properties: vec![],
            string_index: Some(IndexSignature {
                key_type: TypeId::STRING,
                value_type: value,
                readonly,
                param_name: Some(interner.intern_string("k")),
            }),
            number_index: None,
            symbol_index: None,
            symbol: None,
            flags: Default::default(),
        })
    };

    let mut c1 = Canonicalizer::new(&interner, &env);
    let mut c2 = Canonicalizer::new(&interner, &env);
    let mut c3 = Canonicalizer::new(&interner, &env);
    let mutable_num = c1.canonicalize(make(TypeId::NUMBER, false));
    let readonly_num = c2.canonicalize(make(TypeId::NUMBER, true));
    let mutable_str = c3.canonicalize(make(TypeId::STRING, false));
    assert_ne!(mutable_num, readonly_num, "readonly must stay distinct");
    assert_ne!(mutable_num, mutable_str, "value type must stay distinct");
}

#[test]
fn canonicalize_predicate_identifier_target_alpha_equivalent() {
    use crate::types::{FunctionShape, ParamInfo, TypePredicate, TypePredicateTarget};
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    // `(<name>: unknown): <name> is string` — both the value-parameter name and
    // the predicate target identifier vary, but `parameter_index` is the same.
    let make = |name: &str| {
        interner.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.intern_string(name)),
                type_id: TypeId::UNKNOWN,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::BOOLEAN,
            type_predicate: Some(TypePredicate {
                asserts: false,
                target: TypePredicateTarget::Identifier(interner.intern_string(name)),
                type_id: Some(TypeId::STRING),
                parameter_index: Some(0),
            }),
            is_constructor: false,
            is_method: false,
        })
    };

    let mut c1 = Canonicalizer::new(&interner, &env);
    let mut c2 = Canonicalizer::new(&interner, &env);
    assert_eq!(
        c1.canonicalize(make("x")),
        c2.canonicalize(make("value")),
        "predicate functions differing only in the referenced parameter name \
         are the same type and must share one canonical form"
    );
}

#[test]
fn canonicalize_predicate_asserts_and_narrowed_type_stay_distinct() {
    // Negative control: the identifier name drops, but `asserts`, the narrowed
    // type, and the `This`/`Identifier` discriminant are identity-bearing.
    use crate::types::{FunctionShape, ParamInfo, TypePredicate, TypePredicateTarget};
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    let make = |asserts: bool, narrowed: TypeId, target: TypePredicateTarget| {
        interner.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: TypeId::UNKNOWN,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::BOOLEAN,
            type_predicate: Some(TypePredicate {
                asserts,
                target,
                type_id: Some(narrowed),
                parameter_index: Some(0),
            }),
            is_constructor: false,
            is_method: false,
        })
    };

    let ident = || TypePredicateTarget::Identifier(interner.intern_string("x"));
    let base = make(false, TypeId::STRING, ident());

    let mut c1 = Canonicalizer::new(&interner, &env);
    let mut c2 = Canonicalizer::new(&interner, &env);
    let mut c3 = Canonicalizer::new(&interner, &env);
    let mut c4 = Canonicalizer::new(&interner, &env);
    let cbase = c1.canonicalize(base);
    let casserts = c2.canonicalize(make(true, TypeId::STRING, ident()));
    let cnarrow = c3.canonicalize(make(false, TypeId::NUMBER, ident()));
    let cthis = c4.canonicalize(make(false, TypeId::STRING, TypePredicateTarget::This));
    assert_ne!(cbase, casserts, "`asserts` must stay distinct");
    assert_ne!(cbase, cnarrow, "narrowed type must stay distinct");
    assert_ne!(
        cbase, cthis,
        "`this` vs identifier target must stay distinct"
    );
}
