use super::*;
use crate::construction::TypeInterner;
use crate::def::{DefinitionInfo, DefinitionStore};
use crate::types::{
    CallSignature, CallableShape, MappedType, ParamInfo, PropertyInfo, TypeId, TypeParamInfo,
};

fn make_callable_with_construct_sig(
    interner: &TypeInterner,
    return_type: TypeId,
    type_params: Vec<TypeParamInfo>,
) -> TypeId {
    let shape = CallableShape {
        call_signatures: vec![],
        construct_signatures: vec![CallSignature {
            type_params,
            params: vec![ParamInfo::unnamed(TypeId::STRING)],
            this_type: None,
            return_type,
            type_predicate: None,
            is_method: false,
        }],
        properties: vec![],
        string_index: None,
        number_index: None,
        symbol: None,
        is_abstract: false,
    };
    interner.callable(shape)
}

fn make_callable_with_call_sig(interner: &TypeInterner, return_type: TypeId) -> TypeId {
    let shape = CallableShape {
        call_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![ParamInfo::unnamed(TypeId::NUMBER)],
            this_type: None,
            return_type,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
        symbol: None,
        is_abstract: false,
    };
    interner.callable(shape)
}

#[test]
fn get_construct_signatures_direct_callable() {
    let interner = TypeInterner::new();
    let callable = make_callable_with_construct_sig(&interner, TypeId::STRING, vec![]);
    let sigs = get_construct_signatures(&interner, callable);
    assert!(sigs.is_some());
    assert_eq!(sigs.unwrap().len(), 1);
}

#[test]
fn get_construct_signatures_intersection_collects_from_members() {
    let interner = TypeInterner::new();
    // Create two callables with construct signatures
    let ctor1 = make_callable_with_construct_sig(&interner, TypeId::STRING, vec![]);
    let ctor2 = make_callable_with_construct_sig(&interner, TypeId::NUMBER, vec![]);
    // Create intersection: ctor1 & ctor2
    let intersection = interner.intersection2(ctor1, ctor2);
    let sigs = get_construct_signatures(&interner, intersection);
    assert!(sigs.is_some());
    let sigs = sigs.unwrap();
    assert_eq!(
        sigs.len(),
        2,
        "Should collect construct sigs from both members"
    );
}

#[test]
fn get_construct_signatures_intersection_with_non_callable_member() {
    let interner = TypeInterner::new();
    // Create intersection: Constructor & { prop: string }
    let ctor = make_callable_with_construct_sig(&interner, TypeId::STRING, vec![]);
    let obj = interner.object(vec![]); // plain object, no construct sigs
    let intersection = interner.intersection2(ctor, obj);
    let sigs = get_construct_signatures(&interner, intersection);
    assert!(sigs.is_some());
    assert_eq!(
        sigs.unwrap().len(),
        1,
        "Should find construct sig from callable member"
    );
}

#[test]
fn get_construct_signatures_intersection_no_construct_sigs() {
    let interner = TypeInterner::new();
    // Intersection of non-callable types
    let intersection = interner.intersection2(TypeId::STRING, TypeId::NUMBER);
    let sigs = get_construct_signatures(&interner, intersection);
    assert!(sigs.is_none());
}

#[test]
fn get_call_signatures_intersection_collects_from_members() {
    let interner = TypeInterner::new();
    let fn1 = make_callable_with_call_sig(&interner, TypeId::STRING);
    let fn2 = make_callable_with_call_sig(&interner, TypeId::NUMBER);
    let intersection = interner.intersection2(fn1, fn2);
    let sigs = get_call_signatures(&interner, intersection);
    assert!(sigs.is_some());
    let sigs = sigs.unwrap();
    assert_eq!(sigs.len(), 2, "Should collect call sigs from both members");
}

#[test]
fn get_call_signatures_intersection_no_call_sigs() {
    let interner = TypeInterner::new();
    let intersection = interner.intersection2(TypeId::STRING, TypeId::NUMBER);
    let sigs = get_call_signatures(&interner, intersection);
    assert!(sigs.is_none());
}

#[test]
fn contains_never_index_access_surface_matches_direct_index_access() {
    let interner = TypeInterner::new();
    let def_store = DefinitionStore::new();
    let direct = interner.index_access(TypeId::NEVER, TypeId::STRING);
    let other = interner.index_access(TypeId::OBJECT, TypeId::STRING);

    assert!(contains_never_index_access_surface(
        &interner, &def_store, direct, 8,
    ));
    assert!(!contains_never_index_access_surface(
        &interner, &def_store, other, 8,
    ));
}

#[test]
fn contains_never_index_access_surface_follows_display_alias() {
    let interner = TypeInterner::new();
    let def_store = DefinitionStore::new();
    let structural = interner.object(vec![]);
    let display_alias = interner.index_access(TypeId::NEVER, TypeId::STRING);
    interner.store_display_alias(structural, display_alias);

    assert!(contains_never_index_access_surface(
        &interner, &def_store, structural, 8,
    ));
}

#[test]
fn contains_never_index_access_surface_follows_alias_application_body() {
    let interner = TypeInterner::new();
    let def_store = DefinitionStore::new();
    let body = interner.index_access(TypeId::NEVER, TypeId::STRING);
    let alias_id = def_store.register(DefinitionInfo::type_alias(
        interner.intern_string("Boxed"),
        vec![],
        body,
    ));
    let application = interner.application(interner.lazy(alias_id), vec![]);

    assert!(contains_never_index_access_surface(
        &interner,
        &def_store,
        application,
        8,
    ));
}

#[test]
fn construct_sig_with_application_return_type_is_extractable() {
    // Simulates the JSX class component scenario where:
    // interface ComponentClass<P> { new(props: P): Component<P, any>; }
    // interface TestClass extends ComponentClass<{reqd: any}> {}
    //
    // The construct signature return type is Application(Component, [props, any])
    // which needs evaluation. The checker should evaluate it before bailing out.
    let interner = TypeInterner::new();

    // Create an Application type (simulating Component<{reqd: any}, any>)
    let inner_obj = interner.object(vec![]);
    let app_type = interner.application(inner_obj, vec![TypeId::STRING, TypeId::ANY]);

    // Create a callable with construct sig returning the Application type
    let callable = make_callable_with_construct_sig(&interner, app_type, vec![]);

    // Verify we CAN extract construct signatures
    let sigs = get_construct_signatures(&interner, callable);
    assert!(sigs.is_some(), "Should extract construct signatures");
    let sigs = sigs.unwrap();
    assert_eq!(sigs.len(), 1);

    // The return type IS an Application (needs evaluation)
    let return_type = sigs[0].return_type;
    assert!(
        crate::type_queries::needs_evaluation_for_merge(&interner, return_type),
        "Application return type needs evaluation"
    );

    // But the type itself does NOT contain type parameters
    // (all args are concrete: STRING, ANY)
    assert!(
        !crate::contains_type_parameters(&interner, return_type),
        "Concrete application should not contain type parameters"
    );
}

#[test]
fn test_union_has_direct_type_parameter() {
    let interner = crate::intern::TypeInterner::new();

    // Single type parameter
    let tp = interner.type_param(crate::types::TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });
    // Not a union — returns false
    assert!(!union_has_direct_type_parameter(&interner, tp));

    // Union containing a type parameter
    let union_with_tp = interner.union2(TypeId::STRING, tp);
    assert!(union_has_direct_type_parameter(&interner, union_with_tp));

    // Union without type parameters
    let plain_union = interner.union2(TypeId::STRING, TypeId::NUMBER);
    assert!(!union_has_direct_type_parameter(&interner, plain_union));

    // Non-union type
    assert!(!union_has_direct_type_parameter(&interner, TypeId::STRING));
}

#[test]
fn test_collect_callable_property_types() {
    let interner = crate::intern::TypeInterner::new();
    use crate::types::{FunctionShape, PropertyInfo, Visibility};

    // Create a function type (callable property)
    let fn_type = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Create an object with one callable and one non-callable property
    let obj = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("callback"),
            type_id: fn_type,
            write_type: fn_type,
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
            non_widening: false,
        },
        PropertyInfo {
            name: interner.intern_string("value"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 1,
            is_string_named: false,
            is_symbol_named: false,
            single_quoted_name: false,
            non_widening: false,
        },
    ]);

    let result = collect_callable_property_types(&interner, obj);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0], fn_type);

    // Non-object type returns empty
    assert!(collect_callable_property_types(&interner, TypeId::STRING).is_empty());
}

#[test]
fn test_construct_return_type_for_type() {
    let interner = crate::intern::TypeInterner::new();
    use crate::types::{CallSignature, CallableShape, FunctionShape};

    // Function constructor
    let fn_ctor = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });
    assert_eq!(
        construct_return_type_for_type(&interner, fn_ctor),
        Some(TypeId::STRING)
    );

    // Non-constructor function → None
    let fn_regular = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    assert_eq!(construct_return_type_for_type(&interner, fn_regular), None);

    // Callable with construct signature
    let callable = interner.callable(CallableShape {
        call_signatures: vec![],
        construct_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: TypeId::BOOLEAN,
            type_predicate: None,
            is_method: false,
        }],
        properties: vec![],
        string_index: None,
        number_index: None,
        symbol: None,
        is_abstract: false,
    });
    assert_eq!(
        construct_return_type_for_type(&interner, callable),
        Some(TypeId::BOOLEAN)
    );

    // Non-constructable type → None
    assert_eq!(
        construct_return_type_for_type(&interner, TypeId::STRING),
        None
    );
}

#[test]
fn construct_return_type_for_intersection_ignores_static_augmentation_members() {
    let interner = crate::intern::TypeInterner::new();

    let ctor = make_callable_with_construct_sig(&interner, TypeId::STRING, vec![]);
    let augmentation = interner.object(vec![PropertyInfo::new(
        interner.intern_string("enhanced"),
        TypeId::UNKNOWN,
    )]);
    let enhanced_ctor = interner.intersection2(ctor, augmentation);

    assert_eq!(
        construct_return_type_for_type(&interner, enhanced_ctor),
        Some(TypeId::STRING)
    );
}

#[test]
fn test_is_constructor_like_type() {
    let interner = crate::intern::TypeInterner::new();
    use crate::types::{CallSignature, CallableShape, FunctionShape};

    // Constructor function
    let fn_ctor = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });
    assert!(is_constructor_like_type(&interner, fn_ctor));

    // Regular function — not constructor-like
    let fn_regular = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    assert!(!is_constructor_like_type(&interner, fn_regular));

    // Callable with construct signature
    let callable_ctor = interner.callable(CallableShape {
        call_signatures: vec![],
        construct_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: TypeId::OBJECT,
            type_predicate: None,
            is_method: false,
        }],
        properties: vec![],
        string_index: None,
        number_index: None,
        symbol: None,
        is_abstract: false,
    });
    assert!(is_constructor_like_type(&interner, callable_ctor));

    // Union containing a constructor — should be constructor-like
    let union_with_ctor = interner.union2(TypeId::STRING, fn_ctor);
    assert!(is_constructor_like_type(&interner, union_with_ctor));

    // Plain type — not constructor-like
    assert!(!is_constructor_like_type(&interner, TypeId::STRING));
}

#[test]
fn test_extract_type_params_for_call() {
    let interner = crate::intern::TypeInterner::new();
    use crate::types::{FunctionShape, TypeParamInfo};

    let tp_t = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    };

    // Function with 1 type param
    let fn_generic = interner.function(FunctionShape {
        type_params: vec![tp_t],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let result = extract_type_params_for_call(&interner, fn_generic, 1);
    assert!(result.is_some());
    assert_eq!(result.unwrap().len(), 1);

    // Non-callable type → None
    assert!(extract_type_params_for_call(&interner, TypeId::STRING, 0).is_none());
}

#[test]
fn test_get_callable_shape_for_type() {
    let interner = crate::intern::TypeInterner::new();
    use crate::types::FunctionShape;

    // Function → wrapped as single-sig callable
    let fn_type = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let shape = get_callable_shape_for_type(&interner, fn_type);
    assert!(shape.is_some());
    let shape = shape.unwrap();
    assert_eq!(shape.call_signatures.len(), 1);
    assert_eq!(shape.call_signatures[0].return_type, TypeId::STRING);

    // Non-callable → None
    assert!(get_callable_shape_for_type(&interner, TypeId::NUMBER).is_none());
}

#[test]
fn test_get_overload_call_signatures() {
    let interner = crate::intern::TypeInterner::new();
    use crate::types::{CallSignature, CallableShape};

    // Callable with 2 overloads → Some
    let overloaded = interner.callable(CallableShape {
        call_signatures: vec![
            CallSignature {
                type_params: vec![],
                params: vec![],
                this_type: None,
                return_type: TypeId::STRING,
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
                type_params: vec![],
                params: vec![],
                this_type: None,
                return_type: TypeId::NUMBER,
                type_predicate: None,
                is_method: false,
            },
        ],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
        symbol: None,
        is_abstract: false,
    });
    let sigs = get_overload_call_signatures(&interner, overloaded);
    assert!(sigs.is_some());
    assert_eq!(sigs.unwrap().len(), 2);

    // Callable with 1 signature → None (not overloaded)
    let single = interner.callable(CallableShape {
        call_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
        symbol: None,
        is_abstract: false,
    });
    assert!(get_overload_call_signatures(&interner, single).is_none());

    // Non-callable → None
    assert!(get_overload_call_signatures(&interner, TypeId::STRING).is_none());
}

#[test]
fn test_get_object_symbol() {
    let interner = crate::intern::TypeInterner::new();
    use crate::types::{ObjectFlags, ObjectShape, PropertyInfo, Visibility};

    let sym = tsz_binder::SymbolId(42);

    // Object with symbol — use object_with_index to comply with intern quarantine
    let obj_with_sym = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo {
            name: interner.intern_string("x"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
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
            non_widening: false,
        }],
        string_index: None,
        number_index: None,
        symbol_index: None,
        symbol: Some(sym),
    });
    assert_eq!(get_object_symbol(&interner, obj_with_sym), Some(sym));

    // Non-object → None
    assert_eq!(get_object_symbol(&interner, TypeId::STRING), None);
}

#[test]
fn test_get_raw_property_type() {
    let interner = crate::intern::TypeInterner::new();
    use crate::types::{PropertyInfo, Visibility};

    let name_x = interner.intern_string("x");
    let name_y = interner.intern_string("y");

    let obj = interner.object(vec![
        PropertyInfo {
            name: name_x,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
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
            non_widening: false,
        },
        PropertyInfo {
            name: name_y,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 1,
            is_string_named: false,
            is_symbol_named: false,
            single_quoted_name: false,
            non_widening: false,
        },
    ]);

    assert_eq!(
        get_raw_property_type(&interner, obj, name_x),
        Some(TypeId::STRING)
    );
    assert_eq!(
        get_raw_property_type(&interner, obj, name_y),
        Some(TypeId::NUMBER)
    );

    // Non-existent property
    let name_z = interner.intern_string("z");
    assert_eq!(get_raw_property_type(&interner, obj, name_z), None);

    // Non-object type
    assert_eq!(
        get_raw_property_type(&interner, TypeId::STRING, name_x),
        None
    );
}

#[test]
fn test_intersect_constructor_returns() {
    let interner = crate::intern::TypeInterner::new();
    use crate::types::FunctionShape;

    // Function constructor — return type gets intersected
    let fn_ctor = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::OBJECT,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });
    let result = intersect_constructor_returns(&interner, fn_ctor, TypeId::STRING);
    assert_ne!(result, fn_ctor); // Should produce a new type
    // The result should be a Function with intersected return type
    if let Some(shape_id) = crate::visitor::function_shape_id(&interner, result) {
        let shape = interner.function_shape(shape_id);
        assert!(shape.is_constructor);
        // return type should be object & string (intersection)
        assert_ne!(shape.return_type, TypeId::OBJECT);
    } else {
        panic!("Expected Function type");
    }

    // Non-constructor function — unchanged
    let fn_regular = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    assert_eq!(
        intersect_constructor_returns(&interner, fn_regular, TypeId::STRING),
        fn_regular
    );

    // Non-callable — unchanged
    assert_eq!(
        intersect_constructor_returns(&interner, TypeId::STRING, TypeId::NUMBER),
        TypeId::STRING
    );
}

#[test]
fn classify_body_for_arg_preservation_non_conditional() {
    let interner = TypeInterner::new();

    // Non-conditional body → EvaluateAll
    assert_eq!(
        classify_body_for_arg_preservation(&interner, TypeId::STRING),
        BodyArgPreservation::EvaluateAll,
    );
    assert_eq!(
        classify_body_for_arg_preservation(&interner, TypeId::NUMBER),
        BodyArgPreservation::EvaluateAll,
    );
}

#[test]
fn classify_body_for_arg_preservation_conditional_with_infer() {
    let interner = TypeInterner::new();

    let infer_u = interner.type_param(TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });
    let infer_type = interner.infer(TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });

    // Conditional with infer in extends: T extends infer U ? T : never
    let cond_with_infer = interner.conditional(crate::types::ConditionalType {
        check_type: infer_u,
        extends_type: infer_type,
        true_type: infer_u,
        false_type: TypeId::NEVER,
        is_distributive: true,
    });
    assert_eq!(
        classify_body_for_arg_preservation(&interner, cond_with_infer),
        BodyArgPreservation::ConditionalInfer,
    );

    // Conditional without infer: T extends string ? T : never
    let cond_no_infer = interner.conditional(crate::types::ConditionalType {
        check_type: infer_u,
        extends_type: TypeId::STRING,
        true_type: infer_u,
        false_type: TypeId::NEVER,
        is_distributive: true,
    });
    assert_eq!(
        classify_body_for_arg_preservation(&interner, cond_no_infer),
        BodyArgPreservation::EvaluateAll,
    );
}

#[test]
fn classify_body_for_arg_preservation_conditional_application_infer() {
    let interner = TypeInterner::new();

    let param_t = interner.type_param(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });
    let infer_v = interner.infer(TypeParamInfo {
        name: interner.intern_string("V"),
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });

    // Application(Lazy(42), [T, infer V]) — represents Synthetic<T, infer V>
    let base = interner.lazy(crate::def::DefId(42));
    let app_with_infer = interner.application(base, vec![param_t, infer_v]);

    // Conditional: U extends Synthetic<T, infer V> ? V : never
    let cond_app_infer = interner.conditional(crate::types::ConditionalType {
        check_type: param_t,
        extends_type: app_with_infer,
        true_type: infer_v,
        false_type: TypeId::NEVER,
        is_distributive: true,
    });
    assert_eq!(
        classify_body_for_arg_preservation(&interner, cond_app_infer),
        BodyArgPreservation::ConditionalApplicationInfer,
    );
}

// =========================================================================
// is_type_deeply_any
// =========================================================================

#[test]
fn deeply_any_for_any() {
    let interner = TypeInterner::new();
    assert!(is_type_deeply_any(&interner, TypeId::ANY));
}

#[test]
fn deeply_any_for_non_any_primitives() {
    let interner = TypeInterner::new();
    assert!(!is_type_deeply_any(&interner, TypeId::STRING));
    assert!(!is_type_deeply_any(&interner, TypeId::NUMBER));
    assert!(!is_type_deeply_any(&interner, TypeId::NEVER));
    assert!(!is_type_deeply_any(&interner, TypeId::UNKNOWN));
}

#[test]
fn deeply_any_for_array_of_any() {
    let interner = TypeInterner::new();
    let arr = interner.array(TypeId::ANY);
    assert!(is_type_deeply_any(&interner, arr));
}

#[test]
fn deeply_any_for_array_of_string() {
    let interner = TypeInterner::new();
    let arr = interner.array(TypeId::STRING);
    assert!(!is_type_deeply_any(&interner, arr));
}

#[test]
fn deeply_any_for_tuple_of_any() {
    let interner = TypeInterner::new();
    let tuple = interner.tuple(vec![
        crate::types::TupleElement {
            type_id: TypeId::ANY,
            optional: false,
            rest: false,
            name: None,
        },
        crate::types::TupleElement {
            type_id: TypeId::ANY,
            optional: false,
            rest: false,
            name: None,
        },
    ]);
    assert!(is_type_deeply_any(&interner, tuple));
}

#[test]
fn deeply_any_for_tuple_with_non_any_member() {
    let interner = TypeInterner::new();
    let tuple = interner.tuple(vec![
        crate::types::TupleElement {
            type_id: TypeId::ANY,
            optional: false,
            rest: false,
            name: None,
        },
        crate::types::TupleElement {
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
            name: None,
        },
    ]);
    assert!(!is_type_deeply_any(&interner, tuple));
}

#[test]
fn deeply_any_for_union_of_any() {
    let interner = TypeInterner::new();
    // Manually create a union with all-any members
    let union = interner.union2(TypeId::ANY, TypeId::ANY);
    assert!(is_type_deeply_any(&interner, union));
}

#[test]
fn deeply_any_for_union_with_non_any() {
    let interner = TypeInterner::new();
    // union2(ANY, STRING) normalizes to ANY (tsc semantics: any | T = any),
    // so the result IS deeply any. This verifies the normalization is correct.
    let union = interner.union2(TypeId::ANY, TypeId::STRING);
    assert!(is_type_deeply_any(&interner, union));
}

#[test]
fn deeply_any_for_nested_array_of_any() {
    let interner = TypeInterner::new();
    let inner = interner.array(TypeId::ANY);
    let outer = interner.array(inner);
    assert!(is_type_deeply_any(&interner, outer));
}

fn make_array_constrained_param(interner: &TypeInterner, name: &str, element: TypeId) -> TypeId {
    let constraint = interner.array(element);
    interner.type_param(TypeParamInfo {
        name: interner.intern_string(name),
        constraint: Some(constraint),
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    })
}

fn rest_elem(type_id: TypeId) -> crate::types::TupleElement {
    crate::types::TupleElement {
        type_id,
        optional: false,
        rest: true,
        name: None,
    }
}

fn fixed_elem(type_id: TypeId) -> crate::types::TupleElement {
    crate::types::TupleElement {
        type_id,
        optional: false,
        rest: false,
        name: None,
    }
}

#[test]
fn rest_spread_element_of_plain_array_is_its_element() {
    let interner = TypeInterner::new();
    let arr = interner.array(TypeId::STRING);
    assert_eq!(rest_spread_element_type(&interner, arr), TypeId::STRING);
}

#[test]
fn rest_spread_element_of_array_constrained_param_is_constraint_element() {
    // `...End` where `End extends string[]` contributes `string`, not `End`/`string[]`.
    // Binder name is varied to prove the rule is structural, not name-driven.
    let interner = TypeInterner::new();
    for name in ["End", "Rest", "TItems", "_p0"] {
        let param = make_array_constrained_param(&interner, name, TypeId::STRING);
        assert_eq!(
            rest_spread_element_type(&interner, param),
            TypeId::STRING,
            "spread of {name} extends string[] should contribute string"
        );
    }
}

#[test]
fn rest_spread_element_of_tuple_constrained_param_unions_elements() {
    // `...End` where `End extends [number, boolean]` contributes `number | boolean`.
    let interner = TypeInterner::new();
    let constraint = interner.tuple(vec![
        fixed_elem(TypeId::NUMBER),
        fixed_elem(TypeId::BOOLEAN),
    ]);
    let param = interner.type_param(TypeParamInfo {
        name: interner.intern_string("Pair"),
        constraint: Some(constraint),
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::BOOLEAN]);
    assert_eq!(rest_spread_element_type(&interner, param), expected);
}

#[test]
fn rest_spread_element_of_nested_tuple_recurses_into_rest() {
    // `...[A, ...B[]]` contributes `A | B`: the nested rest is unwrapped, not left
    // as the inner array type.
    let interner = TypeInterner::new();
    let inner_array = interner.array(TypeId::NUMBER);
    let tuple = interner.tuple(vec![fixed_elem(TypeId::STRING), rest_elem(inner_array)]);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(rest_spread_element_type(&interner, tuple), expected);
}

#[test]
fn rest_spread_element_of_array_constrained_param_via_nested_rest() {
    // The original bug witness: a single variadic spread `[...End]` of an
    // array-constrained parameter, indexed for its element type, yields `string`.
    let interner = TypeInterner::new();
    let end = make_array_constrained_param(&interner, "End", TypeId::STRING);
    let tuple = interner.tuple(vec![rest_elem(end)]);
    assert_eq!(rest_spread_element_type(&interner, tuple), TypeId::STRING);
}

#[test]
fn rest_spread_element_of_non_array_like_is_unchanged() {
    let interner = TypeInterner::new();
    assert_eq!(
        rest_spread_element_type(&interner, TypeId::STRING),
        TypeId::STRING
    );
}

// =========================================================================
// contains_application_in_structure
// =========================================================================

#[test]
fn contains_application_direct() {
    let interner = TypeInterner::new();
    let base = interner.lazy(crate::def::DefId(1));
    let app = interner.application(base, vec![TypeId::STRING]);
    assert!(contains_application_in_structure(&interner, app));
}

#[test]
fn contains_application_not_present() {
    let interner = TypeInterner::new();
    assert!(!contains_application_in_structure(
        &interner,
        TypeId::STRING
    ));
    assert!(!contains_application_in_structure(&interner, TypeId::ANY));
}

#[test]
fn contains_application_in_union() {
    let interner = TypeInterner::new();
    let base = interner.lazy(crate::def::DefId(1));
    let app = interner.application(base, vec![TypeId::STRING]);
    let union = interner.union2(TypeId::NUMBER, app);
    assert!(contains_application_in_structure(&interner, union));
}

#[test]
fn contains_application_in_intersection() {
    let interner = TypeInterner::new();
    let base = interner.lazy(crate::def::DefId(1));
    let app = interner.application(base, vec![TypeId::STRING]);
    let intersection = interner.intersection(vec![TypeId::NUMBER, app]);
    assert!(contains_application_in_structure(&interner, intersection));
}

#[test]
fn contains_application_in_readonly() {
    let interner = TypeInterner::new();
    let base = interner.lazy(crate::def::DefId(1));
    let app = interner.application(base, vec![TypeId::STRING]);
    let readonly = interner.readonly_type(app);
    assert!(contains_application_in_structure(&interner, readonly));
}

#[test]
fn contains_application_union_without_app() {
    let interner = TypeInterner::new();
    let union = interner.union2(TypeId::STRING, TypeId::NUMBER);
    assert!(!contains_application_in_structure(&interner, union));
}

#[test]
fn contains_application_in_structure_ignores_index_access_operands() {
    let interner = TypeInterner::new();
    let base = interner.lazy(crate::def::DefId(1));
    let app = interner.application(base, vec![TypeId::STRING]);
    let indexed = interner.index_access(app, TypeId::STRING);
    assert!(!contains_application_in_structure(&interner, indexed));
    assert!(contains_application_in_constraint_resolution_path(
        &interner, indexed
    ));
}

#[test]
fn contains_application_in_structure_ignores_mapped_constraints() {
    let interner = TypeInterner::new();
    let name = interner.intern_string("K");
    let base = interner.lazy(crate::def::DefId(1));
    let app = interner.application(base, vec![TypeId::STRING]);
    let mapped = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name,
            constraint: Some(app),
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        },
        constraint: app,
        name_type: None,
        template: TypeId::NUMBER,
        readonly_modifier: None,
        optional_modifier: None,
    });
    assert!(!contains_application_in_structure(&interner, mapped));
    assert!(contains_application_in_constraint_resolution_path(
        &interner, mapped
    ));
}

// =========================================================================
// contains_type_parameters_except_name_db
// =========================================================================

#[test]
fn contains_type_parameters_except_name_ignores_iter_var_constraint() {
    use crate::types::ConditionalType;

    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");
    let k_name = interner.intern_string("K");

    // T (free) and K whose constraint references T.
    let t_param = interner.type_param(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });
    let keyof_t = interner.keyof(t_param);
    let k_param = interner.type_param(TypeParamInfo {
        name: k_name,
        constraint: Some(keyof_t),
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });

    // Sanity: a bare K should not look free relative to itself, even though
    // its baked-in constraint walks back to T.
    assert!(!contains_type_parameters_except_name_db(
        &interner, k_param, k_name,
    ));

    // `{} extends Pick<Obj, K> ? K : never` — but with `Pick` modelled as a
    // Lazy alias and `Obj` as a concrete object — must report no free
    // parameters when the iteration variable `K` is excluded.
    let obj_prop = crate::types::PropertyInfo {
        name: interner.intern_string("a"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: crate::types::Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
        non_widening: false,
    };
    let obj = interner.object(vec![obj_prop]);
    let pick_base = interner.lazy(crate::def::DefId(7));
    let pick_app = interner.application(pick_base, vec![obj, k_param]);
    let empty_obj = interner.object(vec![]);
    let cond = interner.conditional(ConditionalType {
        check_type: empty_obj,
        extends_type: pick_app,
        true_type: k_param,
        false_type: TypeId::NEVER,
        is_distributive: false,
    });
    assert!(
        !contains_type_parameters_except_name_db(&interner, cond, k_name),
        "K's stale `keyof T` constraint must not count as a free reference"
    );

    // A genuinely free `U` in the same position must still be detected.
    let u_name = interner.intern_string("U");
    let u_param = interner.type_param(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });
    let cond_with_u = interner.conditional(ConditionalType {
        check_type: u_param,
        extends_type: pick_app,
        true_type: k_param,
        false_type: TypeId::NEVER,
        is_distributive: false,
    });
    assert!(contains_type_parameters_except_name_db(
        &interner,
        cond_with_u,
        k_name,
    ));

    // Renamed iteration variable: same structure with `P` instead of `K`
    // must behave identically — the rule is structural, not name-based.
    let p_name = interner.intern_string("P");
    let keyof_t_for_p = interner.keyof(t_param);
    let p_param = interner.type_param(TypeParamInfo {
        name: p_name,
        constraint: Some(keyof_t_for_p),
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });
    let pick_app_p = interner.application(pick_base, vec![obj, p_param]);
    let cond_with_p = interner.conditional(ConditionalType {
        check_type: empty_obj,
        extends_type: pick_app_p,
        true_type: p_param,
        false_type: TypeId::NEVER,
        is_distributive: false,
    });
    assert!(!contains_type_parameters_except_name_db(
        &interner,
        cond_with_p,
        p_name,
    ));
}

#[test]
fn contains_type_parameters_except_name_ignores_nested_mapped_param_metadata() {
    // When a `Mapped` appears inside the type being checked, the visitor must
    // not descend into the nested mapped's `type_param.constraint`/`default`
    // either — same rule, recursive case. Without this guard,
    // `for_each_child_by_id`'s default Mapped child enumeration would surface
    // the outer alias parameter `T` through the inner mapped's iter-var
    // metadata.
    use crate::types::{MappedType, PropertyInfo, Visibility};

    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");
    let k_name = interner.intern_string("K");
    let inner_iter_name = interner.intern_string("InnerKey");
    let a_name = interner.intern_string("a");

    let t_param = interner.type_param(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });
    let keyof_t = interner.keyof(t_param);
    let inner_iter = TypeParamInfo {
        name: inner_iter_name,
        constraint: Some(keyof_t),
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    };
    let inner_obj = interner.object(vec![PropertyInfo {
        name: a_name,
        type_id: TypeId::NUMBER,
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
        non_widening: false,
    }]);
    let nested_mapped = interner.mapped(MappedType {
        type_param: inner_iter,
        constraint: inner_obj,
        name_type: None,
        template: TypeId::STRING,
        readonly_modifier: None,
        optional_modifier: None,
    });

    assert!(
        !contains_type_parameters_except_name_db(&interner, nested_mapped, k_name),
        "nested Mapped's iter-var constraint metadata must not surface T"
    );
}

// =========================================================================
// is_literal_or_primitive_or_compound_of_those
// =========================================================================
//
// This predicate decides whether an evaluated generic-alias application should
// drop its alias name in diagnostic display (matching tsc's behaviour). When
// the result is a literal/primitive (or a union/intersection of those), the
// alias is dropped — `KeysExtendedBy<M, number>` shows as `'"b"'`. When the
// result is structural (object/interface/array/etc.), the alias is preserved.

#[test]
fn literal_or_primitive_compound_for_string_literal() {
    let interner = TypeInterner::new();
    let lit = interner.literal_string("b");
    assert!(is_literal_or_primitive_or_compound_of_those(&interner, lit));
}

#[test]
fn literal_or_primitive_compound_for_number_literal() {
    let interner = TypeInterner::new();
    let lit = interner.literal_number(1.0);
    assert!(is_literal_or_primitive_or_compound_of_those(&interner, lit));
}

#[test]
fn literal_or_primitive_compound_for_intrinsic_primitives() {
    let interner = TypeInterner::new();
    for ty in [
        TypeId::STRING,
        TypeId::NUMBER,
        TypeId::BOOLEAN,
        TypeId::BIGINT,
        TypeId::SYMBOL,
        TypeId::NULL,
        TypeId::UNDEFINED,
        TypeId::VOID,
    ] {
        assert!(
            is_literal_or_primitive_or_compound_of_those(&interner, ty),
            "{ty:?} should be classified as primitive/literal-like"
        );
    }
}

#[test]
fn literal_or_primitive_compound_for_union_of_literals() {
    let interner = TypeInterner::new();
    let a = interner.literal_string("a");
    let b = interner.literal_string("b");
    let union = interner.union2(a, b);
    assert!(is_literal_or_primitive_or_compound_of_those(
        &interner, union
    ));
}

#[test]
fn literal_or_primitive_compound_for_union_of_literal_and_primitive() {
    let interner = TypeInterner::new();
    let a = interner.literal_string("a");
    let union = interner.union2(a, TypeId::NUMBER);
    assert!(is_literal_or_primitive_or_compound_of_those(
        &interner, union
    ));
}

#[test]
fn literal_or_primitive_compound_rejects_objects() {
    let interner = TypeInterner::new();
    let obj = interner.object(vec![]);
    assert!(!is_literal_or_primitive_or_compound_of_those(
        &interner, obj
    ));
}

#[test]
fn literal_or_primitive_compound_rejects_arrays_and_tuples() {
    let interner = TypeInterner::new();
    let arr = interner.array(TypeId::STRING);
    assert!(!is_literal_or_primitive_or_compound_of_those(
        &interner, arr
    ));
    let tuple = interner.tuple(vec![crate::types::TupleElement {
        type_id: TypeId::STRING,
        optional: false,
        rest: false,
        name: None,
    }]);
    assert!(!is_literal_or_primitive_or_compound_of_those(
        &interner, tuple
    ));
}

#[test]
fn literal_or_primitive_compound_rejects_union_with_object() {
    let interner = TypeInterner::new();
    let obj = interner.object(vec![]);
    let union = interner.union2(obj, TypeId::STRING);
    assert!(!is_literal_or_primitive_or_compound_of_those(
        &interner, union
    ));
}

#[test]
fn literal_or_primitive_compound_rejects_application() {
    let interner = TypeInterner::new();
    let base = interner.lazy(crate::def::DefId(1));
    let app = interner.application(base, vec![TypeId::STRING]);
    assert!(!is_literal_or_primitive_or_compound_of_those(
        &interner, app
    ));
}

#[test]
fn contains_application_unknown_arg_finds_direct_application_arg() {
    let interner = TypeInterner::new();
    let app = interner.application(TypeId::OBJECT, vec![TypeId::UNKNOWN]);

    assert!(contains_application_unknown_arg(&interner, app));
}

#[test]
fn contains_application_unknown_arg_finds_nested_application_arg() {
    let interner = TypeInterner::new();
    let app = interner.application(TypeId::OBJECT, vec![TypeId::UNKNOWN]);
    let nested = interner.union2(TypeId::STRING, app);

    assert!(contains_application_unknown_arg(&interner, nested));
}

#[test]
fn contains_application_unknown_arg_rejects_non_application_unknown() {
    let interner = TypeInterner::new();
    let app = interner.application(TypeId::OBJECT, vec![TypeId::ANY]);

    assert!(!contains_application_unknown_arg(
        &interner,
        TypeId::UNKNOWN
    ));
    assert!(!contains_application_unknown_arg(&interner, app));
}

/// `is_substitution_dependent_type` must treat only substitution-bound nodes
/// (`TypeParameter`/`Infer`/`ThisType`/`BoundParameter`) as dependent, NOT
/// `Lazy`/`TypeQuery` references (which resolve identically for a project's
/// single fixed resolver). This is the input gate for the closed-eval cache.
#[test]
fn is_substitution_dependent_type_classifies_lazy_vs_type_param() {
    let interner = TypeInterner::new();

    // A bare Lazy ref is NOT substitution-dependent (resolver-fixed).
    let lazy = interner.lazy(crate::def::DefId(7));
    assert!(!is_substitution_dependent_type(&interner, lazy));

    // An application over a Lazy base with concrete args stays independent.
    let app_concrete = interner.application(lazy, vec![TypeId::STRING, TypeId::NUMBER]);
    assert!(!is_substitution_dependent_type(&interner, app_concrete));

    // An IndexAccess over concrete operands is independent.
    let idx_concrete = interner.index_access(app_concrete, TypeId::STRING);
    assert!(!is_substitution_dependent_type(&interner, idx_concrete));

    // A type parameter (any spelling) IS substitution-dependent.
    for name in ["T", "K", "Element"] {
        let tp = interner.type_param(crate::types::TypeParamInfo {
            name: interner.intern_string(name),
            constraint: None,
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        });
        assert!(is_substitution_dependent_type(&interner, tp));
        // Buried inside an application argument it still propagates up.
        let app_generic = interner.application(lazy, vec![tp]);
        assert!(is_substitution_dependent_type(&interner, app_generic));
        // And inside an index access.
        let idx_generic = interner.index_access(app_concrete, tp);
        assert!(is_substitution_dependent_type(&interner, idx_generic));
    }

    // Intrinsics are never substitution-dependent.
    assert!(!is_substitution_dependent_type(&interner, TypeId::ANY));
}

/// `is_structurally_eval_inert` is the read gate for the resolver-independent
/// fixed-point fast path: it must be `true` only for types that evaluate to
/// themselves under every evaluator and resolver. A `Lazy`/`Application`/
/// `TypeQuery`/type-parameter anywhere in the full structural surface (including
/// nested under arrays/unions/tuples) disqualifies the type, because a
/// better-equipped resolver or a substitution could rewrite it. The check is
/// purely structural (name-agnostic) and stable across the cold and cached
/// walks.
#[test]
fn is_structurally_eval_inert_excludes_resolver_and_substitution_dependent_nodes() {
    let interner = TypeInterner::new();

    // Intrinsics and concrete structural composites (with no compound member
    // the deep reducer could touch) are inert.
    assert!(is_structurally_eval_inert(&interner, TypeId::STRING));
    let arr_concrete = interner.array(TypeId::NUMBER);
    assert!(is_structurally_eval_inert(&interner, arr_concrete));
    // Cached re-read is stable.
    assert!(is_structurally_eval_inert(&interner, arr_concrete));

    // `Union` / `Intersection` are NOT inert even when every member is itself
    // inert: `evaluate_union` / `evaluate_intersection` run a deep
    // `SubtypeChecker` reduction that can rewrite a fully concrete compound the
    // interner's shallow construction-time normalization left untouched (e.g.
    // `(string | undefined) & 'string'` reduces to `'string'`). Classifying
    // such a compound as inert from its children alone would short-circuit
    // that reduction and drop discriminated-union excess-property errors.
    let union_concrete = interner.union2(TypeId::STRING, arr_concrete);
    assert!(!is_structurally_eval_inert(&interner, union_concrete));
    // Cached re-read is stable.
    assert!(!is_structurally_eval_inert(&interner, union_concrete));
    let str_or_undef = interner.union2(TypeId::STRING, TypeId::UNDEFINED);
    let lit_string = interner.literal_string("string");
    let reducible_intersection = interner.intersection(vec![str_or_undef, lit_string]);
    // Only meaningful if construction did not already collapse the intersection
    // to the literal; when it stays a compound it must not be cached inert.
    if matches!(
        interner.lookup(reducible_intersection),
        Some(crate::types::TypeData::Intersection(_))
    ) {
        assert!(!is_structurally_eval_inert(
            &interner,
            reducible_intersection
        ));
    }

    // A bare Lazy ref is a deferral the resolver could expand: NOT inert, even
    // though it is not substitution-dependent.
    let lazy = interner.lazy(crate::def::DefId(7));
    assert!(!is_structurally_eval_inert(&interner, lazy));

    // A Lazy buried under array/union wrappers still disqualifies the whole
    // type (full-surface descent).
    let union_with_lazy = interner.union2(TypeId::STRING, interner.array(lazy));
    assert!(!is_structurally_eval_inert(&interner, union_with_lazy));

    // An Application is a meta-type the evaluator rewrites: NOT inert.
    let app = interner.application(lazy, vec![TypeId::STRING]);
    assert!(!is_structurally_eval_inert(&interner, app));

    // An IndexAccess / KeyOf meta-operation is NOT inert even over concrete
    // operands (the evaluator computes the element / key set).
    let idx = interner.index_access(TypeId::OBJECT, TypeId::STRING);
    let keyof = interner.keyof(TypeId::OBJECT);
    assert!(!is_structurally_eval_inert(&interner, idx));
    assert!(!is_structurally_eval_inert(&interner, keyof));

    // Type parameters (any spelling) are substitution-dependent: NOT inert,
    // and the rule is name-agnostic.
    for name in ["T", "K", "Element"] {
        let tp = interner.type_param(crate::types::TypeParamInfo {
            name: interner.intern_string(name),
            constraint: None,
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        });
        assert!(!is_structurally_eval_inert(&interner, tp));
        // Buried inside an array it still propagates up.
        assert!(!is_structurally_eval_inert(&interner, interner.array(tp)));
    }
}

/// The deeply-cached `contains_type_parameters_db` must agree with the
/// non-cached generic walker for the same shapes, regardless of the iteration
/// variable's spelling (anti-hardcoding: the rule is structural, not name-based).
/// The second call exercises the persistent interner cache path.
#[test]
fn contains_type_parameters_db_is_name_agnostic_and_cache_stable() {
    let interner = TypeInterner::new();

    for name in ["T", "K", "P", "Element"] {
        let tp = interner.type_param(crate::types::TypeParamInfo {
            name: interner.intern_string(name),
            constraint: None,
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        });
        // Bury the param under array/union/index-access wrappers.
        let arr = interner.array(tp);
        let union = interner.union2(TypeId::STRING, arr);
        let idx = interner.index_access(union, TypeId::NUMBER);

        // First (cold) and second (cached) calls must both report `true`.
        assert!(contains_type_parameters_db(&interner, idx));
        assert!(contains_type_parameters_db(&interner, idx));

        // A fully concrete sibling shape must report `false` both times.
        let concrete = interner.index_access(
            interner.union2(TypeId::STRING, interner.array(TypeId::NUMBER)),
            TypeId::NUMBER,
        );
        assert!(!contains_type_parameters_db(&interner, concrete));
        assert!(!contains_type_parameters_db(&interner, concrete));
    }
}

/// `get_union_members` hands back a zero-copy view of the interned member
/// list: the returned `Arc` must point at the *same* allocation that
/// `db.type_list` returns, not a fresh `to_vec()` copy. This is the
/// allocation-churn fix's core invariant.
#[test]
fn union_members_returns_zero_copy_view_of_interned_list() {
    let interner = TypeInterner::new();
    let union = interner.union2(TypeId::STRING, TypeId::NUMBER);

    let list_id = match interner.lookup(union) {
        Some(crate::types::TypeData::Union(id)) => id,
        other => panic!("expected union, got {other:?}"),
    };
    let interned = interner.type_list(list_id);

    let members = get_union_members(&interner, union).expect("union has members");

    // Same backing allocation — a refcount bump, not a copy.
    assert!(
        std::sync::Arc::ptr_eq(members.as_arc(), &interned),
        "union_members must reuse the interned Arc, not allocate a fresh Vec",
    );
    // A second query also reuses the same allocation.
    let members2 = get_union_members(&interner, union).expect("union has members");
    assert!(std::sync::Arc::ptr_eq(members.as_arc(), members2.as_arc()));
}

/// `TypeIdList` must behave like the `Vec<TypeId>` it replaced for all the
/// read patterns callers rely on: slice deref, by-value and by-reference
/// iteration, double-ended iteration (`.rev()`), and `==` against a `Vec`.
#[test]
fn type_id_list_is_a_drop_in_for_vec_read_patterns() {
    let interner = TypeInterner::new();
    let a = interner.literal_string("a");
    let b = interner.literal_string("b");
    let c = interner.literal_string("c");
    let union = interner.union(vec![a, b, c]);
    let expected = vec![a, b, c];

    let members = get_union_members(&interner, union).expect("union has members");

    // Deref-to-slice surface.
    assert_eq!(members.len(), 3);
    assert!(!members.is_empty());
    assert_eq!(members[0], a);
    assert!(members.contains(&b));
    assert_eq!(members.first(), Some(&a));
    assert_eq!(members.last(), Some(&c));
    assert_eq!(members.to_vec(), expected);

    // Equality with `Vec<TypeId>` works from both sides.
    assert_eq!(members, expected);
    assert_eq!(expected, members);

    // By-reference iteration yields `&TypeId` (like `&Vec`).
    let by_ref: Vec<TypeId> = (&members).into_iter().copied().collect();
    assert_eq!(by_ref, expected);
    let by_iter: Vec<TypeId> = members.iter().copied().collect();
    assert_eq!(by_iter, expected);

    // Forward by-value iteration yields owned `TypeId` (like `Vec::into_iter`).
    let forward: Vec<TypeId> = members.clone().into_iter().collect();
    assert_eq!(forward, expected);

    // Double-ended iteration matches `Vec`'s `.rev()`.
    let reversed: Vec<TypeId> = members.clone().into_iter().rev().collect();
    assert_eq!(reversed, vec![c, b, a]);

    // Mixed front/back consumption drains every element exactly once.
    let mut it = members.into_iter();
    assert_eq!(it.next(), Some(a));
    assert_eq!(it.next_back(), Some(c));
    assert_eq!(it.next(), Some(b));
    assert_eq!(it.next(), None);
    assert_eq!(it.next_back(), None);
}

/// `ExactSizeIterator::len` and `size_hint` stay accurate as the iterator
/// is consumed from both ends — relied on by callers that pre-size buffers.
#[test]
fn type_id_list_iter_reports_exact_remaining_len() {
    let interner = TypeInterner::new();
    let union = interner.union(vec![
        interner.literal_string("x"),
        interner.literal_string("y"),
        interner.literal_string("z"),
    ]);
    let members = get_union_members(&interner, union).expect("union has members");

    let mut it = members.into_iter();
    assert_eq!(it.len(), 3);
    assert_eq!(it.size_hint(), (3, Some(3)));
    it.next();
    assert_eq!(it.len(), 2);
    it.next_back();
    assert_eq!(it.len(), 1);
    assert_eq!(it.size_hint(), (1, Some(1)));
    it.next();
    assert_eq!(it.len(), 0);
}

/// Corpus of roots covering every `TypeData` variant reachable from test
/// construction, with predicate-relevant leaves planted at varying depths.
/// Used to pin the project-cached content walker to the generic uncached
/// walker: both must descend the same `ChildPolicy::CONTENT_PREDICATE` child
/// set, so their answers must agree on every root for every predicate.
fn content_walk_agreement_corpus(interner: &TypeInterner) -> Vec<TypeId> {
    use crate::types::{
        CallableShape, ConditionalType, IndexSignature, ObjectShape, PropertyInfo, TemplateSpan,
        TupleElement,
    };

    let t_name = interner.intern_string("T");
    let v_name = interner.intern_string("V");
    let prop_name = interner.intern_string("p");

    let plain_param = interner.type_param(TypeParamInfo::simple(t_name));
    let infer_v = interner.infer(TypeParamInfo::simple(v_name));
    let constrained_param = interner.type_param(TypeParamInfo {
        constraint: Some(infer_v),
        default: Some(TypeId::STRING),
        ..TypeParamInfo::simple(t_name)
    });
    let lazy = interner.lazy(crate::def::DefId(7));
    let type_query = interner.type_query(crate::types::SymbolRef(3));
    let this_obj = interner.object(vec![PropertyInfo::new(prop_name, interner.this_type())]);

    let leaves = [
        TypeId::STRING,
        plain_param,
        infer_v,
        constrained_param,
        lazy,
        type_query,
        this_obj,
        interner.conditional(ConditionalType {
            check_type: TypeId::STRING,
            extends_type: plain_param,
            true_type: type_query,
            false_type: TypeId::NEVER,
            is_distributive: false,
        }),
    ];

    let mut corpus = Vec::new();
    for leaf in leaves {
        corpus.push(leaf);
        corpus.push(interner.array(leaf));
        corpus.push(interner.union(vec![TypeId::NUMBER, leaf]));
        corpus.push(interner.intersection(vec![interner.object(vec![]), leaf]));
        corpus.push(interner.tuple(vec![TupleElement::fixed(leaf)]));
        corpus.push(interner.object(vec![PropertyInfo::new(prop_name, leaf)]));
        corpus.push(interner.object_with_index(ObjectShape {
            properties: vec![],
            string_index: Some(IndexSignature {
                key_type: TypeId::STRING,
                value_type: leaf,
                readonly: false,
                param_name: None,
            }),
            ..ObjectShape::default()
        }));
        corpus.push(interner.function(crate::types::FunctionShape::new(
            vec![ParamInfo::unnamed(leaf)],
            TypeId::VOID,
        )));
        corpus.push(interner.callable(CallableShape {
            call_signatures: vec![CallSignature::new(vec![], leaf)],
            ..CallableShape::default()
        }));
        corpus.push(interner.application(lazy, vec![leaf]));
        corpus.push(interner.application(leaf, vec![TypeId::STRING]));
        corpus.push(interner.conditional(ConditionalType {
            check_type: leaf,
            extends_type: TypeId::UNKNOWN,
            true_type: TypeId::STRING,
            false_type: TypeId::NEVER,
            is_distributive: true,
        }));
        corpus.push(interner.mapped(MappedType {
            type_param: TypeParamInfo::simple(interner.intern_string("K")),
            constraint: interner.keyof(leaf),
            name_type: None,
            template: leaf,
            readonly_modifier: None,
            optional_modifier: None,
        }));
        corpus.push(interner.index_access(leaf, TypeId::STRING));
        corpus.push(interner.template_literal(vec![
            TemplateSpan::Text(interner.intern_string("a")),
            TemplateSpan::Type(leaf),
        ]));
        corpus.push(interner.keyof(leaf));
        corpus.push(interner.readonly_type(leaf));
        corpus.push(interner.no_infer(leaf));
        corpus.push(interner.enum_type(crate::def::DefId(9), leaf));
        corpus.push(interner.array(interner.union(vec![
            interner.tuple(vec![TupleElement::fixed(
                interner.object(vec![PropertyInfo::new(prop_name, leaf)]),
            )]),
            TypeId::NULL,
        ])));
    }
    corpus
}

/// The project-cached content walker (`contains_*_db`) and the generic
/// uncached `contains_type_matching` walk must give identical answers: both
/// are drivers over the same canonical `CONTENT_PREDICATE` child enumeration.
/// This replaces the old "must mirror `check_key` exactly" comment contract
/// with an executable check over a generated shape corpus, driven by the REAL
/// `ContentPredicate` impls so predicate edits cannot desynchronize the pin.
#[test]
fn cached_content_walker_agrees_with_generic_walker_on_corpus() {
    use super::content_predicates::{
        ConditionalPredicate, ContentPredicate, InferPredicate, LazyOrRecursivePredicate,
        SubstitutionDependentPredicate, ThisTypePredicate, TypeQueryPredicate,
    };
    use crate::visitors::visitor_predicates::contains_type_matching;

    let interner = TypeInterner::new();
    let corpus = content_walk_agreement_corpus(&interner);
    assert!(corpus.len() > 100);

    fn assert_agreement<P: ContentPredicate>(
        interner: &TypeInterner,
        corpus: &[TypeId],
        predicate: &P,
        cached_query: impl Fn(&TypeInterner, TypeId) -> bool,
        label: &str,
    ) {
        for &root in corpus {
            let cached = cached_query(interner, root);
            let generic =
                contains_type_matching(interner, root, |key| predicate.matches_node(interner, key));
            assert_eq!(cached, generic, "{label} mismatch on {root:?}");
        }
    }

    assert_agreement(
        &interner,
        &corpus,
        &InferPredicate,
        |i, t| contains_infer_types_db(i, t),
        "contains_infer",
    );
    assert_agreement(
        &interner,
        &corpus,
        &TypeQueryPredicate,
        |i, t| contains_type_query_db(i, t),
        "contains_type_query",
    );
    assert_agreement(
        &interner,
        &corpus,
        &LazyOrRecursivePredicate,
        |i, t| contains_lazy_or_recursive_db(i, t),
        "contains_lazy_or_recursive",
    );
    assert_agreement(
        &interner,
        &corpus,
        &ThisTypePredicate,
        |i, t| contains_this_type_db(i, t),
        "contains_this",
    );
    assert_agreement(
        &interner,
        &corpus,
        &ConditionalPredicate,
        |i, t| contains_conditional_type(i, t),
        "contains_conditional",
    );
    assert_agreement(
        &interner,
        &corpus,
        &SubstitutionDependentPredicate,
        |i, t| is_substitution_dependent_type(i, t),
        "substitution-dependent",
    );
}

/// `has_policy_children` must stay in lockstep with the canonical enumerator:
/// whenever it reports a node as terminal under a policy, the enumerator must
/// yield zero children for that node under the same policy. A `false` from
/// `has_policy_children` while children exist would make walkers silently
/// skip subtrees behind their memo/terminal fast paths.
#[test]
fn has_policy_children_matches_enumerator_on_corpus() {
    use crate::visitors::child_policy::{
        ChildPolicy, for_each_child_with_policy, has_policy_children,
    };

    let interner = TypeInterner::new();
    let corpus = content_walk_agreement_corpus(&interner);
    let policies = [
        ("FULL", ChildPolicy::FULL),
        ("EVERYTHING", ChildPolicy::EVERYTHING),
        ("CONTENT_PREDICATE", ChildPolicy::CONTENT_PREDICATE),
        ("FREE_TYPE_PARAMS", ChildPolicy::FREE_TYPE_PARAMS),
        ("FREE_INFER", ChildPolicy::FREE_INFER),
        ("FREE_PARAM_COLLECT", ChildPolicy::FREE_PARAM_COLLECT),
        ("STRUCTURAL_USES", ChildPolicy::STRUCTURAL_USES),
        ("ERROR_CONTAINMENT", ChildPolicy::ERROR_CONTAINMENT),
        ("SHALLOW", ChildPolicy::SHALLOW),
        (
            "STRUCTURAL_USES_SHALLOW",
            ChildPolicy::STRUCTURAL_USES_SHALLOW,
        ),
    ];
    for &root in &corpus {
        let Some(key) = interner.lookup(root) else {
            continue;
        };
        for (name, policy) in &policies {
            if has_policy_children(&key, policy) {
                continue;
            }
            let mut children = 0usize;
            for_each_child_with_policy(&interner, &key, policy, |_| children += 1);
            assert_eq!(
                children, 0,
                "has_policy_children claims terminal under {name} but enumerator \
                 yields {children} children for {root:?}"
            );
        }
    }
}

/// `contains_error_type_db` and the visitor-side `contains_error_type` are one
/// canonical walk: every nested error position — application args, application
/// bases, the raw `TypeId::ERROR` sentinel, and wrapper kinds — must be
/// detected identically through both entry points.
#[test]
fn error_containment_is_unified_across_query_paths() {
    let interner = TypeInterner::new();

    let cases = [
        (TypeId::ERROR, true),
        (
            interner.application(interner.lazy(crate::def::DefId(7)), vec![TypeId::ERROR]),
            true,
        ),
        (
            interner.application(TypeId::ERROR, vec![TypeId::STRING]),
            true,
        ),
        // Deferred operations are opaque: an error inside a keyof/conditional
        // operand is only real once evaluation selects it.
        (interner.keyof(TypeId::ERROR), false),
        (interner.array(TypeId::ERROR), true),
        (interner.union(vec![TypeId::STRING, TypeId::NUMBER]), false),
        (interner.array(TypeId::STRING), false),
    ];
    for (root, expected) in cases {
        assert_eq!(
            contains_error_type_db(&interner, root),
            expected,
            "contains_error_type_db on {root:?}"
        );
        assert_eq!(
            crate::visitors::visitor_predicates::contains_error_type(&interner, root),
            expected,
            "visitor contains_error_type on {root:?}"
        );
    }
}

// =============================================================================
// contains_file_relative_content_db
// =============================================================================

/// Direct file-relative roots: every variant whose meaning depends on the
/// producing file or lexical scope must be flagged.
#[test]
fn file_relative_content_flags_direct_roots() {
    use crate::types::SymbolRef;
    let interner = TypeInterner::new();

    let unresolved = interner.unresolved_type_name(interner.intern_string("LocalName"));
    let type_query = interner.type_query(SymbolRef(7));
    let unique_symbol = interner.unique_symbol(SymbolRef(7));
    let module_ns = interner.module_namespace(SymbolRef(7));
    let this_type = interner.this_type();

    for ty in [unresolved, type_query, unique_symbol, module_ns, this_type] {
        assert!(
            contains_file_relative_content_db(&interner, ty),
            "expected file-relative root to be flagged"
        );
    }
}

/// File-relative content nested inside structural types is found by the deep
/// walk (union member, array element, tuple element).
#[test]
fn file_relative_content_flags_nested_content() {
    use crate::types::{SymbolRef, TupleElement};
    let interner = TypeInterner::new();

    let type_query = interner.type_query(SymbolRef(3));
    let in_union = interner.union(vec![TypeId::STRING, type_query]);
    assert!(contains_file_relative_content_db(&interner, in_union));

    let unresolved = interner.unresolved_type_name(interner.intern_string("Gaps"));
    let in_array = interner.array(unresolved);
    assert!(contains_file_relative_content_db(&interner, in_array));

    let in_tuple = interner.tuple(vec![TupleElement {
        type_id: interner.this_type(),
        optional: false,
        rest: false,
        name: None,
    }]);
    assert!(contains_file_relative_content_db(&interner, in_tuple));
}

/// Program-global content is NOT file-relative: intrinsics, literals,
/// `Lazy(DefId)` references, and applications of lazy bases over concrete
/// args all have one program-wide meaning through the shared store.
#[test]
fn file_relative_content_accepts_program_global_types() {
    use crate::def::DefId;
    let interner = TypeInterner::new();

    let literal = interner.literal_string("transformation");
    let lazy = interner.lazy(DefId(42));
    let app = interner.application(lazy, vec![TypeId::STRING, literal]);
    let union = interner.union(vec![TypeId::NUMBER, app]);
    let arr = interner.array(union);

    for ty in [TypeId::STRING, literal, lazy, app, union, arr] {
        assert!(
            !contains_file_relative_content_db(&interner, ty),
            "expected program-global type to be shareable"
        );
    }
}

/// The memoized walk returns consistent answers on repeat queries (the
/// per-node results live in the shared interner cache).
#[test]
fn file_relative_content_is_stable_across_repeat_queries() {
    use crate::types::SymbolRef;
    let interner = TypeInterner::new();

    let tainted = interner.union(vec![TypeId::STRING, interner.type_query(SymbolRef(9))]);
    let clean = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    for _ in 0..3 {
        assert!(contains_file_relative_content_db(&interner, tainted));
        assert!(!contains_file_relative_content_db(&interner, clean));
    }
}
