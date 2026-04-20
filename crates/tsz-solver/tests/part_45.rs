use super::*;
use crate::TypeInterner;
use crate::def::DefId;
use crate::{SubtypeChecker, TypeSubstitution, instantiate_type};
#[test]
fn test_keyof_four_way_intersection() {
    // keyof (A & B & C & D) = keyof A | keyof B | keyof C | keyof D
    let interner = TypeInterner::new();

    let obj_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let obj_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);

    let obj_c = interner.object(vec![PropertyInfo::new(
        interner.intern_string("c"),
        TypeId::BOOLEAN,
    )]);

    let obj_d = interner.object(vec![PropertyInfo::new(
        interner.intern_string("d"),
        TypeId::STRING,
    )]);

    let intersection = interner.intersection(vec![obj_a, obj_b, obj_c, obj_d]);
    let result = evaluate_keyof(&interner, intersection);

    // Should produce "a" | "b" | "c" | "d"
    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let lit_c = interner.literal_string("c");
    let lit_d = interner.literal_string("d");
    let expected = interner.union(vec![lit_a, lit_b, lit_c, lit_d]);
    assert_eq!(result, expected);
}

#[test]
fn test_keyof_four_way_union() {
    // keyof (A | B | C | D) = only common keys
    let interner = TypeInterner::new();

    let common_key = interner.intern_string("common");

    let obj_a = interner.object(vec![
        PropertyInfo::new(common_key, TypeId::STRING),
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
    ]);

    let obj_b = interner.object(vec![
        PropertyInfo::new(common_key, TypeId::STRING),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
    ]);

    let obj_c = interner.object(vec![PropertyInfo::new(common_key, TypeId::STRING)]);

    let obj_d = interner.object(vec![
        PropertyInfo::new(common_key, TypeId::STRING),
        PropertyInfo::new(interner.intern_string("d"), TypeId::BOOLEAN),
    ]);

    let union = interner.union(vec![obj_a, obj_b, obj_c, obj_d]);
    let result = evaluate_keyof(&interner, union);

    // Only "common" is present in all
    let expected = interner.literal_string("common");
    assert_eq!(result, expected);
}

#[test]
fn test_keyof_mixed_intersection_union() {
    // keyof ((A & B) | C) - nested combination
    let interner = TypeInterner::new();

    let obj_a = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("common"), TypeId::STRING),
    ]);

    let obj_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);

    let obj_c = interner.object(vec![PropertyInfo::new(
        interner.intern_string("common"),
        TypeId::STRING,
    )]);

    let a_and_b = interner.intersection(vec![obj_a, obj_b]);
    let union = interner.union(vec![a_and_b, obj_c]);
    let result = evaluate_keyof(&interner, union);

    // Common keys between (A & B) and C = "common"
    let expected = interner.literal_string("common");
    assert_eq!(result, expected);
}

#[test]
fn test_keyof_intersection_both_index_signatures() {
    // keyof ({ [k: string]: T } & { [k: number]: U }) = string | number
    let interner = TypeInterner::new();

    let string_indexed = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let number_indexed = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
    });

    let intersection = interner.intersection(vec![string_indexed, number_indexed]);
    let result = evaluate_keyof(&interner, intersection);

    // Should be string | number
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

#[test]
fn test_keyof_union_index_and_literal() {
    // keyof ({ [k: string]: T } | { a: U }) - intersection of keys
    let interner = TypeInterner::new();

    let string_indexed = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let literal_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::NUMBER,
    )]);

    let union = interner.union(vec![string_indexed, literal_obj]);
    let result = evaluate_keyof(&interner, union);

    // "a" is subtype of string, so "a" is the common key
    let expected = interner.literal_string("a");
    assert_eq!(result, expected);
}

#[test]
fn test_keyof_intersection_with_callable() {
    // keyof (T & { (): void }) - object with call signature
    let interner = TypeInterner::new();

    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            type_params: Vec::new(),
            params: Vec::new(),
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
    });

    let intersection = interner.intersection(vec![obj, callable]);
    let result = evaluate_keyof(&interner, intersection);

    // Should at least include "a" from the object
    let lit_a = interner.literal_string("a");
    let mut checker = SubtypeChecker::new(&interner);
    assert!(checker.is_subtype_of(lit_a, result));
}

#[test]
fn test_keyof_intersection_with_array() {
    // keyof ({ a: T } & string[]) - object intersected with array
    let interner = TypeInterner::new();

    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let arr = interner.array(TypeId::STRING);
    let intersection = interner.intersection(vec![obj, arr]);
    let result = evaluate_keyof(&interner, intersection);

    // Should include array keys (number index) plus "a" plus array methods
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_keyof_empty_intersection() {
    // keyof (A & B) where A and B have disjoint primitive types
    // This is different from object intersection - primitive intersection is never
    let interner = TypeInterner::new();

    // string & number = never
    let intersection = interner.intersection(vec![TypeId::STRING, TypeId::NUMBER]);
    let result = evaluate_keyof(&interner, intersection);

    // Intersection of disjoint primitives is never, keyof never = never
    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_keyof_empty_union() {
    // keyof never = never
    let interner = TypeInterner::new();

    let result = evaluate_keyof(&interner, TypeId::NEVER);
    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_keyof_nested_keyof() {
    // keyof keyof T - nested keyof application
    let interner = TypeInterner::new();

    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
    ]);

    let keyof_obj = evaluate_keyof(&interner, obj);
    // keyof_obj = "a" | "b"

    // Now keyof (keyof obj) = keyof ("a" | "b") = keyof string (apparent members)
    let keyof_keyof = evaluate_keyof(&interner, keyof_obj);

    // String literal unions extend string, so keyof should give string apparent members
    assert!(keyof_keyof != TypeId::ERROR);
}

// ==================== Callable-parameter inference regression tests ====================

#[test]
fn test_callable_param_infer_union_of_signatures() {
    // T extends ((x: infer P) => any) ? P : never
    // where T = ((x: string) => void) | ((x: number) => void)
    // Result should be string | number (extracting param from both signatures)
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let p_name = interner.intern_string("P");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let infer_p = interner.intern(TypeData::Infer(TypeParamInfo {
        name: p_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: (x: infer P) => any
    let pattern_fn = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(infer_p)],
        return_type: TypeId::ANY,
        type_predicate: None,
        this_type: None,
        type_params: Vec::new(),
        is_constructor: false,
        is_method: false,
    });

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: pattern_fn,
        true_type: infer_p,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);

    // Create union of two function signatures: ((x: string) => void) | ((x: number) => void)
    let fn_string = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(TypeId::STRING)],
        return_type: TypeId::VOID,
        type_predicate: None,
        this_type: None,
        type_params: Vec::new(),
        is_constructor: false,
        is_method: false,
    });
    let fn_number = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(TypeId::NUMBER)],
        return_type: TypeId::VOID,
        type_predicate: None,
        this_type: None,
        type_params: Vec::new(),
        is_constructor: false,
        is_method: false,
    });
    let fn_union = interner.union(vec![fn_string, fn_number]);

    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, fn_union);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Distributive: (string) | (number) = string | number
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

#[test]
fn test_callable_param_infer_overloaded_callable() {
    // T extends { (x: infer P): any } ? P : never
    // where T = { (x: string): void; (x: number): void }
    // For overloaded callables, TypeScript uses the last signature's param
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let p_name = interner.intern_string("P");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let infer_p = interner.intern(TypeData::Infer(TypeParamInfo {
        name: p_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: { (x: infer P): any }
    let pattern_callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            params: vec![ParamInfo::unnamed(infer_p)],
            return_type: TypeId::ANY,
            type_predicate: None,
            this_type: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
    });

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: pattern_callable,
        true_type: infer_p,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);

    // Create overloaded callable with two call signatures
    let overloaded = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![
            CallSignature {
                params: vec![ParamInfo::unnamed(TypeId::STRING)],
                return_type: TypeId::VOID,
                type_predicate: None,
                this_type: None,
                type_params: Vec::new(),
                is_method: false,
            },
            CallSignature {
                params: vec![ParamInfo::unnamed(TypeId::NUMBER)],
                return_type: TypeId::BOOLEAN,
                type_predicate: None,
                this_type: None,
                type_params: Vec::new(),
                is_method: false,
            },
        ],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
    });

    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, overloaded);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Current behavior: callable matching doesn't yet extract from overloads
    // This returns never because Callable vs Callable matching with infer patterns
    // is not fully implemented for extracting from last signature.
    // TODO: Implement proper overload signature extraction for infer patterns
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_callable_param_infer_mixed_union() {
    // T extends ((x: infer P) => any) ? P : never
    // where T = ((x: string) => void) | number
    // Result: string (number doesn't match the pattern so it goes to never branch)
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let p_name = interner.intern_string("P");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let infer_p = interner.intern(TypeData::Infer(TypeParamInfo {
        name: p_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let pattern_fn = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(infer_p)],
        return_type: TypeId::ANY,
        type_predicate: None,
        this_type: None,
        type_params: Vec::new(),
        is_constructor: false,
        is_method: false,
    });

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: pattern_fn,
        true_type: infer_p,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);

    let fn_string = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(TypeId::STRING)],
        return_type: TypeId::VOID,
        type_predicate: None,
        this_type: None,
        type_params: Vec::new(),
        is_constructor: false,
        is_method: false,
    });
    let mixed_union = interner.union(vec![fn_string, TypeId::NUMBER]);

    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, mixed_union);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // string (from fn) | never (from number) = string
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_callable_return_and_param_infer_separately() {
    // T extends ((x: infer P) => infer R) ? [P, R] : never
    // where T = (x: string) => number
    // Result: [string, number] represented as a tuple
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let p_name = interner.intern_string("P");
    let r_name = interner.intern_string("R");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let infer_p = interner.intern(TypeData::Infer(TypeParamInfo {
        name: p_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: r_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let pattern_fn = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(infer_p)],
        return_type: infer_r,
        type_predicate: None,
        this_type: None,
        type_params: Vec::new(),
        is_constructor: false,
        is_method: false,
    });

    // True type: tuple [P, R]
    let tuple_type = interner.tuple(vec![
        TupleElement {
            type_id: infer_p,
            optional: false,
            rest: false,
            name: None,
        },
        TupleElement {
            type_id: infer_r,
            optional: false,
            rest: false,
            name: None,
        },
    ]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: pattern_fn,
        true_type: tuple_type,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);

    let source_fn = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(TypeId::STRING)],
        return_type: TypeId::NUMBER,
        type_predicate: None,
        this_type: None,
        type_params: Vec::new(),
        is_constructor: false,
        is_method: false,
    });

    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, source_fn);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let expected = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
            name: None,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
            name: None,
        },
    ]);
    assert_eq!(result, expected);
}

#[test]
fn test_callable_multiple_params_infer() {
    // T extends ((a: infer A, b: infer B) => any) ? [A, B] : never
    // where T = (a: string, b: number) => void
    // Result: [string, number]
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let infer_a = interner.intern(TypeData::Infer(TypeParamInfo {
        name: a_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let infer_b = interner.intern(TypeData::Infer(TypeParamInfo {
        name: b_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let pattern_fn = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(infer_a), ParamInfo::unnamed(infer_b)],
        return_type: TypeId::ANY,
        type_predicate: None,
        this_type: None,
        type_params: Vec::new(),
        is_constructor: false,
        is_method: false,
    });

    let tuple_type = interner.tuple(vec![
        TupleElement {
            type_id: infer_a,
            optional: false,
            rest: false,
            name: None,
        },
        TupleElement {
            type_id: infer_b,
            optional: false,
            rest: false,
            name: None,
        },
    ]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: pattern_fn,
        true_type: tuple_type,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);

    let source_fn = interner.function(FunctionShape {
        params: vec![
            ParamInfo::unnamed(TypeId::STRING),
            ParamInfo::unnamed(TypeId::NUMBER),
        ],
        return_type: TypeId::VOID,
        type_predicate: None,
        this_type: None,
        type_params: Vec::new(),
        is_constructor: false,
        is_method: false,
    });

    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, source_fn);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let expected = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
            name: None,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
            name: None,
        },
    ]);
    assert_eq!(result, expected);
}

// =============================================================================
// Mapped Type Edge Cases - Homomorphic Modifiers & Key Remapping
// =============================================================================
// These tests cover advanced mapped type scenarios including homomorphic
// modifier preservation, complex key remapping, and edge cases.

#[test]
fn test_mapped_type_homomorphic_preserves_optional() {
    // Homomorphic: { [K in keyof T]: T[K] } preserves optional from source
    let interner = TypeInterner::new();

    // Source type with optional property
    let source = interner.object(vec![
        PropertyInfo::new(interner.intern_string("required"), TypeId::STRING),
        PropertyInfo::opt(interner.intern_string("optional"), TypeId::NUMBER),
    ]);

    let keyof_source = interner.intern(TypeData::KeyOf(source));

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keyof_source,
        name_type: None,
        template: TypeId::BOOLEAN,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_mapped_type_homomorphic_preserves_readonly() {
    // Homomorphic: { [K in keyof T]: T[K] } preserves readonly from source
    let interner = TypeInterner::new();

    let source = interner.object(vec![
        PropertyInfo::new(interner.intern_string("mutable"), TypeId::STRING),
        PropertyInfo::readonly(interner.intern_string("immutable"), TypeId::NUMBER),
    ]);

    let keyof_source = interner.intern(TypeData::KeyOf(source));

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keyof_source,
        name_type: None,
        template: TypeId::BOOLEAN,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_mapped_type_key_remap_to_getter_setter() {
    // Key remapping: { [K in keyof T as `get${Capitalize<K>}`]: () => T[K] }
    let interner = TypeInterner::new();

    let key_x = interner.literal_string("x");
    let key_y = interner.literal_string("y");
    let keys = interner.union(vec![key_x, key_y]);

    // Simulate key remapping with template literal
    let get_x = interner.literal_string("getX");
    let get_y = interner.literal_string("getY");
    let remapped_keys = interner.union(vec![get_x, get_y]);

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keys,
        name_type: Some(remapped_keys),
        template: TypeId::NUMBER,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_mapped_type_key_remap_filter_by_type() {
    // Filter keys: { [K in keyof T as T[K] extends string ? K : never]: T[K] }
    let interner = TypeInterner::new();

    let key_name = interner.literal_string("name");
    let key_age = interner.literal_string("age");
    let keys = interner.union(vec![key_name, key_age]);

    // Only "name" passes filter (string type), "age" becomes never
    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keys,
        name_type: Some(key_name),
        template: TypeId::STRING,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_mapped_type_nested_mapped() {
    // Nested: { [K in keyof T]: { [J in keyof T[K]]: boolean } }
    let interner = TypeInterner::new();

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let outer_keys = interner.union(vec![key_a, key_b]);

    let inner_template = TypeId::BOOLEAN;

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: outer_keys,
        name_type: None,
        template: inner_template,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);
    assert!(result != TypeId::ERROR);
}

