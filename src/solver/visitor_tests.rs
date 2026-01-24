//! Tests for the Type Visitor Pattern implementation.

use super::*;
use crate::solver::visitor::*;

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
    assert_eq!(TypeKindVisitor::get_kind_of(&interner, lit), TypeKind::Literal);

    let lit_num = interner.literal_number(42.0);
    assert_eq!(TypeKindVisitor::get_kind_of(&interner, lit_num), TypeKind::Literal);

    // Object types
    let obj = interner.object(vec![]);
    assert_eq!(TypeKindVisitor::get_kind_of(&interner, obj), TypeKind::Object);

    // Array types
    let arr = interner.array(TypeId::STRING);
    assert_eq!(TypeKindVisitor::get_kind_of(&interner, arr), TypeKind::Array);

    // Union types
    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(TypeKindVisitor::get_kind_of(&interner, union), TypeKind::Union);

    // Intersection types
    let inter = interner.intersection(vec![TypeId::STRING, TypeId::NUMBER]);
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
    assert_eq!(TypeKindVisitor::get_kind_of(&interner, func), TypeKind::Function);
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

    let inter = interner.intersection(vec![TypeId::STRING, TypeId::NUMBER]);
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

    let param = interner.type_parameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
    });
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

    let t_param = interner.type_parameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
    });

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

    // Check for any literal type
    let lit = interner.literal_string("hello");
    let union = interner.union(vec![TypeId::STRING, lit]);

    let has_literal = contains_type_matching(&interner, union, |key| {
        matches!(key, TypeKey::Literal(_))
    });
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
