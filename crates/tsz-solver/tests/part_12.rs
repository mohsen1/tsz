use super::*;
use crate::TypeInterner;
use crate::def::DefId;
use crate::{SubtypeChecker, TypeSubstitution, instantiate_type};
#[test]
fn test_keyof_never() {
    let interner = TypeInterner::new();

    // keyof never = never
    let result = evaluate_keyof(&interner, TypeId::NEVER);
    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_keyof_nullish() {
    let interner = TypeInterner::new();

    // keyof null/undefined/void = never
    assert_eq!(evaluate_keyof(&interner, TypeId::NULL), TypeId::NEVER);
    assert_eq!(evaluate_keyof(&interner, TypeId::UNDEFINED), TypeId::NEVER);
    assert_eq!(evaluate_keyof(&interner, TypeId::VOID), TypeId::NEVER);
}

#[test]
fn test_keyof_string_apparent_members() {
    let interner = TypeInterner::new();

    let result = evaluate_keyof(&interner, TypeId::STRING);
    let key = interner
        .lookup(result)
        .expect("expected union for keyof string");

    match key {
        TypeData::Union(members) => {
            let members = interner.type_list(members);
            let length = interner.literal_string("length");
            let to_string = interner.literal_string("toString");
            assert!(members.contains(&length));
            assert!(members.contains(&to_string));
            assert!(members.contains(&TypeId::NUMBER));
        }
        other => panic!("Expected union, got {other:?}"),
    }
}

#[test]
fn test_apparent_number_keyof_members() {
    let interner = TypeInterner::new();

    let result = evaluate_keyof(&interner, TypeId::NUMBER);
    let key = interner
        .lookup(result)
        .expect("expected union for keyof number");

    match key {
        TypeData::Union(members) => {
            let members = interner.type_list(members);
            let to_fixed = interner.literal_string("toFixed");
            let value_of = interner.literal_string("valueOf");
            assert!(members.contains(&to_fixed));
            assert!(members.contains(&value_of));
        }
        other => panic!("Expected union, got {other:?}"),
    }
}

#[test]
fn test_keyof_template_literal_matches_string() {
    let interner = TypeInterner::new();

    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("prefix")),
        TemplateSpan::Type(TypeId::STRING),
        TemplateSpan::Text(interner.intern_string("suffix")),
    ]);

    let result = evaluate_keyof(&interner, template);
    let expected = evaluate_keyof(&interner, TypeId::STRING);
    assert_eq!(result, expected);
}

// =============================================================================
// KEYOF OPERATOR TESTS
// =============================================================================

/// Test basic keyof on simple object type.
///
/// keyof { a: string, b: number, c: boolean } = "a" | "b" | "c"
#[test]
fn test_keyof_basic_object_type() {
    let interner = TypeInterner::new();

    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("c"), TypeId::BOOLEAN),
    ]);

    let result = evaluate_keyof(&interner, obj);

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let key_c = interner.literal_string("c");
    let expected = interner.union(vec![key_a, key_b, key_c]);
    assert_eq!(result, expected);
}

/// Test keyof on single property object.
///
/// keyof { only: string } = "only"
#[test]
fn test_keyof_single_property() {
    let interner = TypeInterner::new();

    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("only"),
        TypeId::STRING,
    )]);

    let result = evaluate_keyof(&interner, obj);

    // Single property should produce the literal key
    let expected = interner.literal_string("only");
    assert_eq!(result, expected);
}

/// Test keyof on intersection produces union of all keys.
///
/// keyof ({ a: string } & { b: number }) = "a" | "b"
#[test]
fn test_keyof_intersection_produces_union() {
    let interner = TypeInterner::new();

    let obj1 = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let obj2 = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);

    let intersection = interner.intersection(vec![obj1, obj2]);
    let result = evaluate_keyof(&interner, intersection);

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let expected = interner.union(vec![key_a, key_b]);
    assert_eq!(result, expected);
}

/// Test keyof on intersection with overlapping keys.
///
/// keyof ({ a: string, b: number } & { b: boolean, c: string }) = "a" | "b" | "c"
#[test]
fn test_keyof_intersection_overlapping_keys() {
    let interner = TypeInterner::new();

    let obj1 = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
    ]);

    let obj2 = interner.object(vec![
        PropertyInfo::new(interner.intern_string("b"), TypeId::BOOLEAN),
        PropertyInfo::new(interner.intern_string("c"), TypeId::STRING),
    ]);

    let intersection = interner.intersection(vec![obj1, obj2]);
    let result = evaluate_keyof(&interner, intersection);

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let key_c = interner.literal_string("c");
    let expected = interner.union(vec![key_a, key_b, key_c]);
    assert_eq!(result, expected);
}

/// Test keyof on union produces intersection of keys.
///
/// keyof ({ a: string, b: number } | { b: boolean, c: string }) = "b"
/// (only common keys)
#[test]
fn test_keyof_union_common_keys_only() {
    let interner = TypeInterner::new();

    let obj1 = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
    ]);

    let obj2 = interner.object(vec![
        PropertyInfo::new(interner.intern_string("b"), TypeId::BOOLEAN),
        PropertyInfo::new(interner.intern_string("c"), TypeId::STRING),
    ]);

    let union = interner.union(vec![obj1, obj2]);
    let result = evaluate_keyof(&interner, union);

    // Only "b" is common to both
    let expected = interner.literal_string("b");
    assert_eq!(result, expected);
}

/// Test keyof on union with no common keys produces never.
///
/// keyof ({ a: string } | { b: number }) = never
#[test]
fn test_keyof_union_no_common_keys() {
    let interner = TypeInterner::new();

    let obj1 = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let obj2 = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);

    let union = interner.union(vec![obj1, obj2]);
    let result = evaluate_keyof(&interner, union);

    // No common keys = never
    assert_eq!(result, TypeId::NEVER);
}

/// Test keyof with mapped type constraint.
///
/// keyof { [K in "x" | "y"]: number } = "x" | "y"
#[test]
fn test_keyof_mapped_type_basic() {
    let interner = TypeInterner::new();

    let key_x = interner.literal_string("x");
    let key_y = interner.literal_string("y");
    let keys = interner.union(vec![key_x, key_y]);

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keys,
        name_type: None,
        template: TypeId::NUMBER,
        readonly_modifier: None,
        optional_modifier: None,
    };

    // First evaluate the mapped type to get the resulting object
    let mapped_result = evaluate_mapped(&interner, &mapped);

    // Then get keyof the result
    let result = evaluate_keyof(&interner, mapped_result);

    let expected = interner.union(vec![key_x, key_y]);
    assert_eq!(result, expected);
}

/// Test keyof with mapped type with remapped keys.
///
/// keyof { [K in "a" | "b" as `${K}_key`]: string } = "`a_key`" | "`b_key`"
#[test]
fn test_keyof_mapped_type_remapped_keys() {
    let interner = TypeInterner::new();

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let keys = interner.union(vec![key_a, key_b]);

    let key_a_key = interner.literal_string("a_key");
    let key_b_key = interner.literal_string("b_key");

    let key_param = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: Some(keys),
        default: None,
        is_const: false,
    };
    let key_param_id = interner.intern(TypeData::TypeParameter(key_param));

    // Create conditional: K extends "a" ? "a_key" : K extends "b" ? "b_key" : never
    let inner_cond = interner.conditional(ConditionalType {
        check_type: key_param_id,
        extends_type: key_b,
        true_type: key_b_key,
        false_type: TypeId::NEVER,
        is_distributive: false,
    });
    let name_type = interner.conditional(ConditionalType {
        check_type: key_param_id,
        extends_type: key_a,
        true_type: key_a_key,
        false_type: inner_cond,
        is_distributive: false,
    });

    let mapped = MappedType {
        type_param: key_param,
        constraint: keys,
        name_type: Some(name_type),
        template: TypeId::STRING,
        readonly_modifier: None,
        optional_modifier: None,
    };

    // First evaluate the mapped type to get the resulting object
    let mapped_result = evaluate_mapped(&interner, &mapped);

    // Then get keyof the result
    let result = evaluate_keyof(&interner, mapped_result);

    let expected = interner.union(vec![key_a_key, key_b_key]);
    assert_eq!(result, expected);
}

/// Test keyof with readonly and optional properties.
///
/// keyof { readonly a: string, b?: number } = "a" | "b"
#[test]
fn test_keyof_readonly_and_optional_properties() {
    let interner = TypeInterner::new();

    let obj = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("a"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: true, // readonly
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        },
        PropertyInfo {
            name: interner.intern_string("b"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: true, // optional
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        },
    ]);

    let result = evaluate_keyof(&interner, obj);

    // readonly and optional don't affect keyof
    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let expected = interner.union(vec![key_a, key_b]);
    assert_eq!(result, expected);
}

/// Test keyof on triple intersection.
///
/// keyof ({ a: string } & { b: number } & { c: boolean }) = "a" | "b" | "c"
#[test]
fn test_keyof_triple_intersection() {
    let interner = TypeInterner::new();

    let obj1 = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let obj2 = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);

    let obj3 = interner.object(vec![PropertyInfo::new(
        interner.intern_string("c"),
        TypeId::BOOLEAN,
    )]);

    let intersection = interner.intersection(vec![obj1, obj2, obj3]);
    let result = evaluate_keyof(&interner, intersection);

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let key_c = interner.literal_string("c");
    let expected = interner.union(vec![key_a, key_b, key_c]);
    assert_eq!(result, expected);
}

/// Test keyof on union with identical keys.
///
/// keyof ({ a: string, b: number } | { a: boolean, b: string }) = "a" | "b"
#[test]
fn test_keyof_union_identical_keys() {
    let interner = TypeInterner::new();

    let obj1 = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
    ]);

    let obj2 = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::BOOLEAN),
        PropertyInfo::new(interner.intern_string("b"), TypeId::STRING),
    ]);

    let union = interner.union(vec![obj1, obj2]);
    let result = evaluate_keyof(&interner, union);

    // Both objects have "a" and "b"
    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let expected = interner.union(vec![key_a, key_b]);
    assert_eq!(result, expected);
}

// =============================================================================
// KEYOF EDGE CASE TESTS
// =============================================================================

#[test]
fn test_keyof_nested_object_only_top_level() {
    // keyof { a: { b: number } } = "a" (not "a" | "b")
    let interner = TypeInterner::new();

    let inner_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);

    let outer_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        inner_obj,
    )]);

    let result = evaluate_keyof(&interner, outer_obj);
    let expected = interner.literal_string("a");
    assert_eq!(result, expected);
}

#[test]
fn test_keyof_both_index_signatures() {
    // keyof { [k: string]: any, [n: number]: any } = string | number
    let interner = TypeInterner::new();

    let obj = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::ANY,
            readonly: false,
            param_name: None,
        }),
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::ANY,
            readonly: false,
            param_name: None,
        }),
    });

    let result = evaluate_keyof(&interner, obj);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

#[test]
fn test_keyof_numeric_literal_keys() {
    // keyof { 0: string, 1: number } = "0" | "1"
    let interner = TypeInterner::new();

    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("0"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("1"), TypeId::NUMBER),
    ]);

    let result = evaluate_keyof(&interner, obj);
    let key_0 = interner.literal_string("0");
    let key_1 = interner.literal_string("1");
    let expected = interner.union(vec![key_0, key_1]);
    assert_eq!(result, expected);
}

#[test]
fn test_keyof_mixed_optional_required() {
    // keyof { a: string, b?: number, c: boolean } = "a" | "b" | "c"
    let interner = TypeInterner::new();

    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::opt(interner.intern_string("b"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("c"), TypeId::BOOLEAN),
    ]);

    let result = evaluate_keyof(&interner, obj);
    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let key_c = interner.literal_string("c");
    let expected = interner.union(vec![key_a, key_b, key_c]);
    assert_eq!(result, expected);
}
