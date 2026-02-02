//! Tests for the Type Visitor Pattern implementation.

use super::*;

// =============================================================================
// TypeKind Tests
// =============================================================================

#[test]
fn test_type_kind_classification() {
    let interner = TypeInterner::new();

    // Intrinsic types
    assert_eq!(
        TypeKindVisitor::get_kind_of(&interner, TypeId::STRING),
        TypeKind::Primitive
    );
    assert_eq!(
        TypeKindVisitor::get_kind_of(&interner, TypeId::NUMBER),
        TypeKind::Primitive
    );
    assert_eq!(
        TypeKindVisitor::get_kind_of(&interner, TypeId::BOOLEAN),
        TypeKind::Primitive
    );

    // Literal types
    let lit = interner.literal_string("hello");
    assert_eq!(
        TypeKindVisitor::get_kind_of(&interner, lit),
        TypeKind::Literal
    );

    let lit_num = interner.literal_number(42.0);
    assert_eq!(
        TypeKindVisitor::get_kind_of(&interner, lit_num),
        TypeKind::Literal
    );

    // Object types
    let obj = interner.object(vec![]);
    assert_eq!(
        TypeKindVisitor::get_kind_of(&interner, obj),
        TypeKind::Object
    );

    // Array types
    let arr = interner.array(TypeId::STRING);
    assert_eq!(
        TypeKindVisitor::get_kind_of(&interner, arr),
        TypeKind::Array
    );

    // Union types
    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(
        TypeKindVisitor::get_kind_of(&interner, union),
        TypeKind::Union
    );

    // Intersection types - use type parameters since the interner simplifies
    // primitive intersections to never and object intersections to merged objects
    let t_param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
    }));
    let u_param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: None,
        default: None,
    }));
    let inter = interner.intersection(vec![t_param, u_param]);
    assert_eq!(
        TypeKindVisitor::get_kind_of(&interner, inter),
        TypeKind::Intersection
    );

    // Function types
    let func = interner.function(FunctionShape {
        params: vec![],
        return_type: TypeId::VOID,
        type_params: vec![],
        this_type: None,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    assert_eq!(
        TypeKindVisitor::get_kind_of(&interner, func),
        TypeKind::Function
    );
}

#[test]
fn test_is_type_kind() {
    let interner = TypeInterner::new();

    let lit = interner.literal_string("test");
    assert!(is_type_kind(&interner, lit, TypeKind::Literal));
    assert!(!is_type_kind(&interner, lit, TypeKind::Primitive));

    let obj = interner.object(vec![]);
    assert!(is_type_kind(&interner, obj, TypeKind::Object));
    assert!(!is_type_kind(&interner, obj, TypeKind::Array));
}

// =============================================================================
// Type Predicate Tests
// =============================================================================

#[test]
fn test_is_literal_type() {
    let interner = TypeInterner::new();

    let str_lit = interner.literal_string("hello");
    let num_lit = interner.literal_number(42.0);
    let bool_lit = interner.literal_boolean(true);

    assert!(is_literal_type(&interner, str_lit));
    assert!(is_literal_type(&interner, num_lit));
    assert!(is_literal_type(&interner, bool_lit));
    assert!(!is_literal_type(&interner, TypeId::STRING));
    assert!(!is_literal_type(&interner, TypeId::NUMBER));
}

#[test]
fn test_is_function_type() {
    let interner = TypeInterner::new();

    let func = interner.function(FunctionShape {
        params: vec![],
        return_type: TypeId::VOID,
        type_params: vec![],
        this_type: None,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    assert!(is_function_type(&interner, func));
    assert!(!is_function_type(&interner, TypeId::STRING));
    assert!(!is_function_type(&interner, TypeId::OBJECT));

    // Intersection containing function
    let obj = interner.object(vec![]);
    let inter = interner.intersection(vec![func, obj]);
    assert!(is_function_type(&interner, inter));
}

#[test]
fn test_is_object_like_type() {
    let interner = TypeInterner::new();

    let obj = interner.object(vec![]);
    let arr = interner.array(TypeId::STRING);
    let tuple = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    }]);

    assert!(is_object_like_type(&interner, obj));
    assert!(is_object_like_type(&interner, arr));
    assert!(is_object_like_type(&interner, tuple));
    assert!(!is_object_like_type(&interner, TypeId::STRING));
    assert!(!is_object_like_type(&interner, TypeId::NUMBER));
}

#[test]
fn test_is_empty_object_type() {
    let interner = TypeInterner::new();

    let empty_obj = interner.object(vec![]);
    let non_empty_obj = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    assert!(is_empty_object_type(&interner, empty_obj));
    assert!(!is_empty_object_type(&interner, non_empty_obj));
    assert!(!is_empty_object_type(&interner, TypeId::STRING));
}

#[test]
fn test_is_primitive_type() {
    let interner = TypeInterner::new();

    assert!(is_primitive_type(&interner, TypeId::STRING));
    assert!(is_primitive_type(&interner, TypeId::NUMBER));
    assert!(is_primitive_type(&interner, TypeId::BOOLEAN));
    assert!(is_primitive_type(&interner, TypeId::BIGINT));
    assert!(is_primitive_type(&interner, TypeId::SYMBOL));
    assert!(is_primitive_type(&interner, TypeId::UNDEFINED));
    assert!(is_primitive_type(&interner, TypeId::NULL));

    let lit = interner.literal_string("test");
    assert!(is_primitive_type(&interner, lit));

    let obj = interner.object(vec![]);
    assert!(!is_primitive_type(&interner, obj));
}

#[test]
fn test_is_union_type() {
    let interner = TypeInterner::new();

    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert!(is_union_type(&interner, union));
    assert!(!is_union_type(&interner, TypeId::STRING));
}

#[test]
fn test_is_intersection_type() {
    let interner = TypeInterner::new();

    // Use type parameters since the interner simplifies primitive intersections
    // to never and object intersections to merged objects
    let t_param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
    }));
    let u_param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: None,
        default: None,
    }));
    let inter = interner.intersection(vec![t_param, u_param]);
    assert!(is_intersection_type(&interner, inter));
    assert!(!is_intersection_type(&interner, TypeId::STRING));
}

#[test]
fn test_is_array_type() {
    let interner = TypeInterner::new();

    let arr = interner.array(TypeId::STRING);
    assert!(is_array_type(&interner, arr));
    assert!(!is_array_type(&interner, TypeId::STRING));
}

#[test]
fn test_is_tuple_type() {
    let interner = TypeInterner::new();

    let tuple = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    }]);
    assert!(is_tuple_type(&interner, tuple));
    assert!(!is_tuple_type(&interner, TypeId::STRING));
}

#[test]
fn test_is_type_parameter() {
    let interner = TypeInterner::new();

    let param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
    }));
    assert!(is_type_parameter(&interner, param));
    assert!(!is_type_parameter(&interner, TypeId::STRING));
}

// =============================================================================
// Recursive Type Collector Tests
// =============================================================================

#[test]
fn test_collect_all_types_simple() {
    let interner = TypeInterner::new();

    // string[]
    let arr = interner.array(TypeId::STRING);
    let collected = collect_all_types(&interner, arr);

    assert!(collected.contains(&arr));
    assert!(collected.contains(&TypeId::STRING));
}

#[test]
fn test_collect_all_types_nested() {
    let interner = TypeInterner::new();

    // { x: number, y: string }
    let obj = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("x"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: interner.intern_string("y"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    let collected = collect_all_types(&interner, obj);

    assert!(collected.contains(&obj));
    assert!(collected.contains(&TypeId::NUMBER));
    assert!(collected.contains(&TypeId::STRING));
}

#[test]
fn test_collect_all_types_union() {
    let interner = TypeInterner::new();

    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]);
    let collected = collect_all_types(&interner, union);

    assert!(collected.contains(&union));
    assert!(collected.contains(&TypeId::STRING));
    assert!(collected.contains(&TypeId::NUMBER));
    assert!(collected.contains(&TypeId::BOOLEAN));
}

#[test]
fn test_collect_all_types_function() {
    let interner = TypeInterner::new();

    let func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        return_type: TypeId::STRING,
        type_params: vec![],
        this_type: None,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let collected = collect_all_types(&interner, func);

    assert!(collected.contains(&func));
    assert!(collected.contains(&TypeId::NUMBER));
    assert!(collected.contains(&TypeId::STRING));
}

// =============================================================================
// Contains Type Tests
// =============================================================================

#[test]
fn test_contains_type_parameters() {
    let interner = TypeInterner::new();

    let t_param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
    }));

    // Array<T>
    let arr = interner.array(t_param);
    assert!(contains_type_parameters(&interner, arr));

    // string[]
    let str_arr = interner.array(TypeId::STRING);
    assert!(!contains_type_parameters(&interner, str_arr));
}

#[test]
fn test_contains_error_type() {
    let interner = TypeInterner::new();

    assert!(contains_error_type(&interner, TypeId::ERROR));

    let union_with_error = interner.union(vec![TypeId::STRING, TypeId::ERROR]);
    assert!(contains_error_type(&interner, union_with_error));

    let union_no_error = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert!(!contains_error_type(&interner, union_no_error));
}

#[test]
fn test_contains_type_matching() {
    let interner = TypeInterner::new();

    // Check for any literal type - use number | "hello" since string | "hello"
    // normalizes to just string (literal subsumed by base type)
    let lit = interner.literal_string("hello");
    let union = interner.union(vec![TypeId::NUMBER, lit]);

    let has_literal =
        contains_type_matching(&interner, union, |key| matches!(key, TypeKey::Literal(_)));
    assert!(has_literal);

    let no_literal = contains_type_matching(&interner, TypeId::STRING, |key| {
        matches!(key, TypeKey::Literal(_))
    });
    assert!(!no_literal);
}

// =============================================================================
// TypeVisitor Trait Tests
// =============================================================================

#[test]
fn test_type_predicate_visitor() {
    let interner = TypeInterner::new();

    let lit = interner.literal_string("test");
    let is_str_lit = test_type(&interner, lit, |key| {
        matches!(key, TypeKey::Literal(LiteralValue::String(_)))
    });
    assert!(is_str_lit);

    let is_num_lit = test_type(&interner, lit, |key| {
        matches!(key, TypeKey::Literal(LiteralValue::Number(_)))
    });
    assert!(!is_num_lit);
}

#[test]
fn test_type_collector_visitor_basic() {
    let interner = TypeInterner::new();

    let arr = interner.array(TypeId::STRING);
    let collected = collect_referenced_types(&interner, arr);

    assert!(collected.contains(&TypeId::STRING));
}

// =============================================================================
// Type Data Extraction Helper Tests
// =============================================================================

#[test]
fn test_type_list_extractors_for_union_and_intersection() {
    let interner = TypeInterner::new();

    let t_param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
    }));
    let u_param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: None,
        default: None,
    }));

    let union = interner.union(vec![t_param, u_param]);
    let union_list = union_list_id(&interner, union).expect("expected union list id");
    let mut union_members = interner.type_list(union_list).to_vec();
    union_members.sort_by_key(|id| id.0);
    let mut expected_union = vec![t_param, u_param];
    expected_union.sort_by_key(|id| id.0);
    assert_eq!(union_members, expected_union);
    assert!(union_list_id(&interner, TypeId::STRING).is_none());

    let intersection = interner.intersection(vec![t_param, u_param]);
    let intersection_list =
        intersection_list_id(&interner, intersection).expect("expected intersection list id");
    let mut intersection_members = interner.type_list(intersection_list).to_vec();
    intersection_members.sort_by_key(|id| id.0);
    let mut expected_intersection = vec![t_param, u_param];
    expected_intersection.sort_by_key(|id| id.0);
    assert_eq!(intersection_members, expected_intersection);
    assert!(intersection_list_id(&interner, TypeId::STRING).is_none());
}

#[test]
fn test_object_shape_extractors() {
    let interner = TypeInterner::new();

    let obj = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    assert!(object_shape_id(&interner, obj).is_some());
    assert!(object_with_index_shape_id(&interner, obj).is_none());

    let obj_with_index = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name: interner.intern_string("y"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
        }),
        number_index: None,
    });
    assert!(object_shape_id(&interner, obj_with_index).is_none());
    assert!(object_with_index_shape_id(&interner, obj_with_index).is_some());
}

#[test]
fn test_collection_extractors() {
    let interner = TypeInterner::new();

    let array = interner.array(TypeId::STRING);
    assert_eq!(array_element_type(&interner, array), Some(TypeId::STRING));
    assert!(array_element_type(&interner, TypeId::STRING).is_none());

    let tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
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
    let tuple_id = tuple_list_id(&interner, tuple).expect("expected tuple list id");
    let elements = interner.tuple_list(tuple_id);
    assert_eq!(elements.len(), 2);
    assert_eq!(elements[0].type_id, TypeId::STRING);
    assert_eq!(elements[1].type_id, TypeId::NUMBER);
}

#[test]
fn test_literal_and_intrinsic_extractors() {
    let interner = TypeInterner::new();

    assert_eq!(intrinsic_kind(&interner, TypeId::STRING), Some(IntrinsicKind::String));

    let hello = interner.literal_string("hello");
    let forty_two = interner.literal_number(42.0);

    assert_eq!(
        literal_value(&interner, hello),
        Some(LiteralValue::String(interner.intern_string("hello")))
    );
    assert_eq!(literal_string(&interner, hello), Some(interner.intern_string("hello")));
    assert!(literal_number(&interner, hello).is_none());

    assert!(literal_string(&interner, forty_two).is_none());
    assert!(literal_value(&interner, forty_two).is_some());
    assert!(literal_number(&interner, forty_two).is_some());
}

#[test]
fn test_template_literal_and_index_access_extractors() {
    let interner = TypeInterner::new();

    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("hello-")),
        TemplateSpan::Type(TypeId::STRING),
    ]);
    let template_id =
        template_literal_id(&interner, template).expect("expected template literal id");
    let spans = interner.template_list(template_id);
    assert_eq!(spans.len(), 2);

    let index_access = interner.intern(TypeKey::IndexAccess(TypeId::OBJECT, TypeId::NUMBER));
    assert_eq!(
        index_access_parts(&interner, index_access),
        Some((TypeId::OBJECT, TypeId::NUMBER))
    );
}

#[test]
fn test_type_param_ref_and_lazy_extractors() {
    let interner = TypeInterner::new();

    let param_info = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
    };
    let param_type = interner.intern(TypeKey::TypeParameter(param_info.clone()));
    assert_eq!(type_param_info(&interner, param_type), Some(param_info));

    let symbol = SymbolRef(42);
    let ref_type = interner.reference(symbol);
    assert_eq!(ref_symbol(&interner, ref_type), Some(symbol));

    let def_id = DefId(7);
    let lazy_type = interner.intern(TypeKey::Lazy(def_id));
    assert_eq!(lazy_def_id(&interner, lazy_type), Some(def_id));
}

#[test]
fn test_application_mapped_and_conditional_extractors() {
    let interner = TypeInterner::new();

    let app_base = interner.reference(SymbolRef(7));
    let app = interner.application(app_base, vec![TypeId::STRING]);
    let app_id = application_id(&interner, app).expect("expected application id");
    let app_data = interner.type_application(app_id);
    assert_eq!(app_data.base, app_base);
    assert_eq!(app_data.args, vec![TypeId::STRING]);

    let mapped = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
        },
        constraint: TypeId::STRING,
        name_type: None,
        template: TypeId::NUMBER,
        readonly_modifier: None,
        optional_modifier: None,
    });
    let mapped_id = mapped_type_id(&interner, mapped).expect("expected mapped type id");
    let mapped_data = interner.mapped_type(mapped_id);
    assert_eq!(mapped_data.constraint, TypeId::STRING);

    let conditional = interner.conditional(ConditionalType {
        check_type: TypeId::STRING,
        extends_type: TypeId::STRING,
        true_type: TypeId::NUMBER,
        false_type: TypeId::BOOLEAN,
        is_distributive: false,
    });
    let cond_id = conditional_type_id(&interner, conditional).expect("expected conditional id");
    let cond_data = interner.conditional_type(cond_id);
    assert_eq!(cond_data.true_type, TypeId::NUMBER);
}

#[test]
fn test_keyof_readonly_query_and_unique_symbol_extractors() {
    let interner = TypeInterner::new();

    let obj = interner.object(vec![]);
    let keyof_type = interner.intern(TypeKey::KeyOf(obj));
    assert_eq!(keyof_inner_type(&interner, keyof_type), Some(obj));

    let readonly_type = interner.readonly_type(TypeId::STRING);
    assert_eq!(readonly_inner_type(&interner, readonly_type), Some(TypeId::STRING));

    let symbol = SymbolRef(99);
    let query = interner.intern(TypeKey::TypeQuery(symbol));
    assert_eq!(type_query_symbol(&interner, query), Some(symbol));

    let unique = interner.intern(TypeKey::UniqueSymbol(symbol));
    assert_eq!(unique_symbol_ref(&interner, unique), Some(symbol));
}

#[test]
fn test_function_and_callable_extractors() {
    let interner = TypeInterner::new();

    let func = interner.function(FunctionShape {
        params: Vec::new(),
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        this_type: None,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let func_id = function_shape_id(&interner, func).expect("expected function shape id");
    let func_shape = interner.function_shape(func_id);
    assert_eq!(func_shape.return_type, TypeId::VOID);

    let callable = interner.callable(CallableShape {
        call_signatures: vec![CallSignature {
            type_params: Vec::new(),
            params: Vec::new(),
            this_type: None,
            return_type: TypeId::STRING,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
    });
    let callable_id =
        callable_shape_id(&interner, callable).expect("expected callable shape id");
    let callable_shape = interner.callable_shape(callable_id);
    assert_eq!(callable_shape.call_signatures.len(), 1);
}

// =============================================================================
// Additional Predicate and Containment Tests
// =============================================================================

#[test]
fn test_is_this_type_and_contains_this_type() {
    let interner = TypeInterner::new();

    let this_type = interner.intern(TypeKey::ThisType);
    assert!(is_this_type(&interner, this_type));
    assert!(!is_this_type(&interner, TypeId::STRING));

    let union = interner.union(vec![TypeId::STRING, this_type]);
    assert!(contains_this_type(&interner, union));
    assert!(!contains_this_type(&interner, TypeId::STRING));
}

#[test]
fn test_contains_infer_types() {
    let interner = TypeInterner::new();

    let infer_type = interner.intern(TypeKey::Infer(TypeParamInfo {
        name: interner.intern_string("R"),
        constraint: None,
        default: None,
    }));
    let union = interner.union(vec![TypeId::STRING, infer_type]);

    assert!(contains_infer_types(&interner, union));
    assert!(!contains_infer_types(&interner, TypeId::STRING));
}

#[test]
fn test_reference_and_namespace_predicates() {
    let interner = TypeInterner::new();

    let symbol = SymbolRef(1);
    let ref_type = interner.reference(symbol);
    let app_type = interner.application(ref_type, vec![TypeId::STRING]);
    let module_ns = interner.intern(TypeKey::ModuleNamespace(symbol));

    assert!(is_type_reference(&interner, ref_type));
    assert!(!is_type_reference(&interner, TypeId::STRING));
    assert!(is_generic_application(&interner, app_type));
    assert!(!is_generic_application(&interner, ref_type));
    assert!(is_module_namespace_type(&interner, module_ns));
    assert!(!is_module_namespace_type(&interner, TypeId::STRING));
}

#[test]
fn test_meta_type_predicates() {
    let interner = TypeInterner::new();

    let conditional = interner.conditional(ConditionalType {
        check_type: TypeId::STRING,
        extends_type: TypeId::STRING,
        true_type: TypeId::NUMBER,
        false_type: TypeId::BOOLEAN,
        is_distributive: false,
    });
    let mapped = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
        },
        constraint: TypeId::STRING,
        name_type: None,
        template: TypeId::NUMBER,
        readonly_modifier: None,
        optional_modifier: None,
    });
    let index_access = interner.intern(TypeKey::IndexAccess(TypeId::OBJECT, TypeId::NUMBER));
    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("t")),
        TemplateSpan::Type(TypeId::STRING),
    ]);

    assert!(is_conditional_type(&interner, conditional));
    assert!(is_mapped_type(&interner, mapped));
    assert!(is_index_access_type(&interner, index_access));
    assert!(is_template_literal_type(&interner, template));
    assert!(!is_index_access_type(&interner, TypeId::STRING));
}
