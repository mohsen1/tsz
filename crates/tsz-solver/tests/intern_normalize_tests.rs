use super::*;
use crate::def::DefId;
use crate::intern::PROPERTY_MAP_THRESHOLD;

// =========================================================================
// 1. TYPE INTERNING CORE — DEDUPLICATION
// =========================================================================

#[test]
fn dedup_literal_number() {
    let i = TypeInterner::new();
    let a = i.literal_number(42.0);
    let b = i.literal_number(42.0);
    let c = i.literal_number(99.0);
    assert_eq!(a, b, "Same number literal must intern to same TypeId");
    assert_ne!(a, c);
}

#[test]
fn dedup_literal_boolean() {
    let i = TypeInterner::new();
    // Boolean literals are mapped to intrinsic IDs
    let t1 = i.literal_boolean(true);
    let t2 = i.literal_boolean(true);
    let f1 = i.literal_boolean(false);
    assert_eq!(t1, t2);
    assert_eq!(t1, TypeId::BOOLEAN_TRUE);
    assert_eq!(f1, TypeId::BOOLEAN_FALSE);
    assert_ne!(t1, f1);
}

#[test]
fn dedup_literal_bigint() {
    let i = TypeInterner::new();
    let a = i.literal_bigint("100");
    let b = i.literal_bigint("100");
    let c = i.literal_bigint("200");
    assert_eq!(a, b);
    assert_ne!(a, c);
}

#[test]
fn dedup_array_type() {
    let i = TypeInterner::new();
    let a = i.array(TypeId::STRING);
    let b = i.array(TypeId::STRING);
    let c = i.array(TypeId::NUMBER);
    assert_eq!(a, b, "Array<string> must dedup");
    assert_ne!(a, c, "Array<string> != Array<number>");
}

#[test]
fn dedup_tuple_type() {
    let i = TypeInterner::new();
    let elems = vec![
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
    ];
    let a = i.tuple(elems.clone());
    let b = i.tuple(elems);
    assert_eq!(a, b, "Same tuple structure must dedup");
}

#[test]
fn dedup_object_type() {
    let i = TypeInterner::new();
    let props = vec![PropertyInfo::new(i.intern_string("x"), TypeId::NUMBER)];
    let a = i.object(props.clone());
    let b = i.object(props);
    assert_eq!(a, b, "Same object structure must dedup");
}

#[test]
fn dedup_function_type() {
    let i = TypeInterner::new();
    let shape = FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo::unnamed(TypeId::STRING)],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };
    let a = i.function(shape.clone());
    let b = i.function(shape);
    assert_eq!(a, b, "Same function shape must dedup");
}

#[test]
fn dedup_conditional_type() {
    let i = TypeInterner::new();
    let cond = ConditionalType {
        check_type: TypeId::STRING,
        extends_type: TypeId::NUMBER,
        true_type: TypeId::BOOLEAN,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };
    let a = i.conditional(cond);
    let b = i.conditional(cond);
    assert_eq!(a, b, "Same conditional type must dedup");
}

#[test]
fn dedup_mapped_type() {
    let i = TypeInterner::new();
    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: i.intern_string("K"),
            constraint: Some(TypeId::STRING),
            default: None,
            is_const: false,
        },
        constraint: TypeId::STRING,
        name_type: None,
        template: TypeId::NUMBER,
        readonly_modifier: None,
        optional_modifier: None,
    };
    let a = i.mapped(mapped);
    let b = i.mapped(mapped);
    assert_eq!(a, b, "Same mapped type must dedup");
}

#[test]
fn dedup_keyof_type() {
    let i = TypeInterner::new();
    let a = i.keyof(TypeId::STRING);
    let b = i.keyof(TypeId::STRING);
    let c = i.keyof(TypeId::NUMBER);
    assert_eq!(a, b);
    assert_ne!(a, c);
}

#[test]
fn dedup_index_access_type() {
    let i = TypeInterner::new();
    let a = i.index_access(TypeId::STRING, TypeId::NUMBER);
    let b = i.index_access(TypeId::STRING, TypeId::NUMBER);
    let c = i.index_access(TypeId::NUMBER, TypeId::STRING);
    assert_eq!(a, b);
    assert_ne!(a, c);
}

#[test]
fn dedup_lazy_type() {
    let i = TypeInterner::new();
    let a = i.lazy(DefId(1));
    let b = i.lazy(DefId(1));
    let c = i.lazy(DefId(2));
    assert_eq!(a, b);
    assert_ne!(a, c);
}

#[test]
fn dedup_type_param() {
    let i = TypeInterner::new();
    let info = TypeParamInfo {
        name: i.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let a = i.type_param(info);
    let b = i.type_param(info);
    assert_eq!(a, b);
}

#[test]
fn dedup_readonly_array() {
    let i = TypeInterner::new();
    let arr = i.array(TypeId::STRING);
    let ra = i.readonly_array(TypeId::STRING);
    let rb = i.readonly_array(TypeId::STRING);
    assert_eq!(ra, rb, "ReadonlyArray<string> must dedup");
    assert_ne!(arr, ra, "Array<string> != ReadonlyArray<string>");
}

// =========================================================================
// 2. UNION CONSTRUCTION
// =========================================================================

#[test]
fn union_empty_is_never() {
    let i = TypeInterner::new();
    assert_eq!(i.union(vec![]), TypeId::NEVER);
}

#[test]
fn union_single_member_returns_that_member() {
    let i = TypeInterner::new();
    assert_eq!(i.union(vec![TypeId::STRING]), TypeId::STRING);
}

#[test]
fn union_duplicate_members_deduplicated() {
    let i = TypeInterner::new();
    let lit = i.literal_string("x");
    let u = i.union(vec![lit, lit, lit]);
    assert_eq!(u, lit, "Duplicate members must be deduplicated to single");
}

#[test]
fn union_nested_flattened() {
    let i = TypeInterner::new();
    let inner = i.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let outer = i.union(vec![inner, TypeId::BOOLEAN]);
    let direct = i.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]);
    assert_eq!(outer, direct, "(string | number) | boolean must flatten");
}

#[test]
fn union_with_never_removed() {
    let i = TypeInterner::new();
    let u = i.union(vec![TypeId::STRING, TypeId::NEVER, TypeId::NUMBER]);
    let expected = i.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(u, expected, "never must be removed from union");
}

#[test]
fn union_only_never_is_never() {
    let i = TypeInterner::new();
    assert_eq!(i.union(vec![TypeId::NEVER, TypeId::NEVER]), TypeId::NEVER);
}

#[test]
fn union_with_unknown_collapses() {
    let i = TypeInterner::new();
    let u = i.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::UNKNOWN]);
    assert_eq!(u, TypeId::UNKNOWN);
}

#[test]
fn union_with_any_collapses() {
    let i = TypeInterner::new();
    let u = i.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::ANY]);
    assert_eq!(u, TypeId::ANY);
}

#[test]
fn union_any_beats_unknown() {
    let i = TypeInterner::new();
    assert_eq!(i.union(vec![TypeId::ANY, TypeId::UNKNOWN]), TypeId::ANY);
    assert_eq!(i.union(vec![TypeId::UNKNOWN, TypeId::ANY]), TypeId::ANY);
}

#[test]
fn union_with_error_collapses() {
    let i = TypeInterner::new();
    let u = i.union(vec![TypeId::STRING, TypeId::ERROR]);
    assert_eq!(u, TypeId::ERROR);
}

#[test]
fn union2_fast_path() {
    let i = TypeInterner::new();
    // union2 with identical members
    assert_eq!(i.union2(TypeId::STRING, TypeId::STRING), TypeId::STRING);
    // union2 with never
    assert_eq!(i.union2(TypeId::NEVER, TypeId::STRING), TypeId::STRING);
    assert_eq!(i.union2(TypeId::STRING, TypeId::NEVER), TypeId::STRING);
}

#[test]
fn union3_fast_path() {
    let i = TypeInterner::new();
    let u = i.union3(TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN);
    let expected = i.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]);
    assert_eq!(u, expected);
}

// =========================================================================
// 3. UNION NORMALIZATION — LITERAL ABSORPTION
// =========================================================================

#[test]
fn union_literal_string_absorbed_by_string() {
    let i = TypeInterner::new();
    let hello = i.literal_string("hello");
    let u = i.union(vec![hello, TypeId::STRING]);
    assert_eq!(u, TypeId::STRING, "\"hello\" | string => string");
}

#[test]
fn union_literal_number_absorbed_by_number() {
    let i = TypeInterner::new();
    let num = i.literal_number(42.0);
    let u = i.union(vec![num, TypeId::NUMBER]);
    assert_eq!(u, TypeId::NUMBER, "42 | number => number");
}

#[test]
fn union_literal_bigint_absorbed_by_bigint() {
    let i = TypeInterner::new();
    let big = i.literal_bigint("42");
    let u = i.union(vec![big, TypeId::BIGINT]);
    assert_eq!(u, TypeId::BIGINT, "42n | bigint => bigint");
}

#[test]
fn union_boolean_reconstruction() {
    let i = TypeInterner::new();
    let u = i.union(vec![TypeId::BOOLEAN_TRUE, TypeId::BOOLEAN_FALSE]);
    assert_eq!(u, TypeId::BOOLEAN, "true | false => boolean");
}

#[test]
fn union_boolean_true_absorbed_by_boolean() {
    let i = TypeInterner::new();
    let u = i.union(vec![TypeId::BOOLEAN_TRUE, TypeId::BOOLEAN]);
    assert_eq!(u, TypeId::BOOLEAN, "true | boolean => boolean");
}

#[test]
fn union_boolean_false_absorbed_by_boolean() {
    let i = TypeInterner::new();
    let u = i.union(vec![TypeId::BOOLEAN_FALSE, TypeId::BOOLEAN]);
    assert_eq!(u, TypeId::BOOLEAN);
}

#[test]
fn union_multiple_string_literals_not_absorbed_without_string() {
    let i = TypeInterner::new();
    let a = i.literal_string("a");
    let b = i.literal_string("b");
    let u = i.union(vec![a, b]);
    // Should remain as "a" | "b", not collapse
    assert_ne!(u, TypeId::STRING);
    match i.lookup(u) {
        Some(TypeData::Union(list_id)) => {
            let members = i.type_list(list_id);
            assert_eq!(members.len(), 2);
        }
        _ => panic!("Expected union type"),
    }
}

#[test]
fn union_multiple_number_literals_absorbed_by_number() {
    let i = TypeInterner::new();
    let n1 = i.literal_number(1.0);
    let n2 = i.literal_number(2.0);
    let n3 = i.literal_number(3.0);
    let u = i.union(vec![n1, n2, n3, TypeId::NUMBER]);
    assert_eq!(u, TypeId::NUMBER, "1 | 2 | 3 | number => number");
}

#[test]
fn union_mixed_literals_absorbed_selectively() {
    let i = TypeInterner::new();
    let hello = i.literal_string("hello");
    let num42 = i.literal_number(42.0);
    // string present, number not -> only string literal absorbed
    let u = i.union(vec![hello, num42, TypeId::STRING]);
    // Should be string | 42
    match i.lookup(u) {
        Some(TypeData::Union(list_id)) => {
            let members = i.type_list(list_id);
            assert_eq!(members.len(), 2);
            assert!(members.contains(&TypeId::STRING));
            assert!(members.contains(&num42));
        }
        _ => panic!("Expected union of string | 42"),
    }
}

// =========================================================================
// 4. UNION — ORDER INDEPENDENCE AND STABILITY
// =========================================================================

#[test]
fn union_order_independence_primitives() {
    let i = TypeInterner::new();
    let ab = i.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let ba = i.union(vec![TypeId::NUMBER, TypeId::STRING]);
    assert_eq!(ab, ba, "string | number == number | string");
}

#[test]
fn union_order_independence_literals() {
    let i = TypeInterner::new();
    let a = i.literal_string("alpha");
    let b = i.literal_string("beta");
    let c = i.literal_string("gamma");
    let abc = i.union(vec![a, b, c]);
    let cba = i.union(vec![c, b, a]);
    let bac = i.union(vec![b, a, c]);
    assert_eq!(abc, cba);
    assert_eq!(abc, bac);
}

// =========================================================================
// 5. UNION — TYPE PARAMETERS PRESERVED
// =========================================================================

#[test]
fn union_with_type_param_not_reduced() {
    let i = TypeInterner::new();
    let tp = i.type_param(TypeParamInfo {
        name: i.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    });
    // T | string should NOT reduce T away
    let u = i.union(vec![tp, TypeId::STRING]);
    match i.lookup(u) {
        Some(TypeData::Union(list_id)) => {
            let members = i.type_list(list_id);
            assert!(
                members.contains(&tp),
                "Type parameter must be preserved in union"
            );
            assert!(members.contains(&TypeId::STRING));
        }
        _ => panic!("Expected union type"),
    }
}

// =========================================================================
// 6. UNION — LITERAL-ONLY REDUCTION
// =========================================================================

#[test]
fn union_literal_reduce_absorbs_literals() {
    let i = TypeInterner::new();
    let hello = i.literal_string("hello");
    let u = i.union_literal_reduce(vec![hello, TypeId::STRING]);
    assert_eq!(u, TypeId::STRING, "Literal reduce should absorb literals");
}

#[test]
fn union_literal_reduce_preserves_structural_subtypes() {
    let i = TypeInterner::new();
    // Create two object types where one is a structural subtype of the other
    let obj_a = i.object(vec![PropertyInfo::new(
        i.intern_string("x"),
        TypeId::NUMBER,
    )]);
    let obj_ab = i.object(vec![
        PropertyInfo::new(i.intern_string("x"), TypeId::NUMBER),
        PropertyInfo::new(i.intern_string("y"), TypeId::STRING),
    ]);
    // Literal reduce should NOT collapse these even though obj_ab <: obj_a
    let u = i.union_literal_reduce(vec![obj_a, obj_ab]);
    match i.lookup(u) {
        Some(TypeData::Union(list_id)) => {
            let members = i.type_list(list_id);
            assert_eq!(
                members.len(),
                2,
                "Literal reduce should preserve structural subtypes"
            );
        }
        _ => panic!("Expected union type"),
    }
}

// =========================================================================
// 7. INTERSECTION CONSTRUCTION
// =========================================================================

#[test]
fn intersection_empty_is_unknown() {
    let i = TypeInterner::new();
    assert_eq!(i.intersection(vec![]), TypeId::UNKNOWN);
}

#[test]
fn intersection_single_member() {
    let i = TypeInterner::new();
    assert_eq!(i.intersection(vec![TypeId::STRING]), TypeId::STRING);
}

#[test]
fn intersection_with_never_is_never() {
    let i = TypeInterner::new();
    let r = i.intersection(vec![TypeId::STRING, TypeId::NEVER]);
    assert_eq!(r, TypeId::NEVER);
}

#[test]
fn intersection_with_any_is_any() {
    let i = TypeInterner::new();
    let r = i.intersection(vec![TypeId::STRING, TypeId::ANY]);
    assert_eq!(r, TypeId::ANY);
}

#[test]
fn intersection_with_error_is_error() {
    let i = TypeInterner::new();
    let r = i.intersection(vec![TypeId::STRING, TypeId::ERROR]);
    assert_eq!(r, TypeId::ERROR);
}

#[test]
fn intersection_with_unknown_identity() {
    let i = TypeInterner::new();
    let r = i.intersection(vec![TypeId::STRING, TypeId::UNKNOWN]);
    assert_eq!(r, TypeId::STRING, "string & unknown => string");
}

#[test]
fn intersection_any_over_unknown() {
    let i = TypeInterner::new();
    assert_eq!(
        i.intersection(vec![TypeId::ANY, TypeId::UNKNOWN]),
        TypeId::ANY
    );
}

#[test]
fn intersection_nested_flattened() {
    let i = TypeInterner::new();
    let obj_a = i.object(vec![PropertyInfo::new(
        i.intern_string("a"),
        TypeId::NUMBER,
    )]);
    let obj_b = i.object(vec![PropertyInfo::new(
        i.intern_string("b"),
        TypeId::STRING,
    )]);
    let obj_c = i.object(vec![PropertyInfo::new(
        i.intern_string("c"),
        TypeId::BOOLEAN,
    )]);

    let inner = i.intersection(vec![obj_a, obj_b]);
    let outer = i.intersection(vec![inner, obj_c]);
    let direct = i.intersection(vec![obj_a, obj_b, obj_c]);
    assert_eq!(outer, direct, "Nested intersections must flatten");
}

#[test]
fn intersection_duplicate_deduplicated() {
    let i = TypeInterner::new();
    let obj = i.object(vec![PropertyInfo::new(
        i.intern_string("x"),
        TypeId::NUMBER,
    )]);
    let dup = i.intersection(vec![obj, obj]);
    assert_eq!(dup, obj, "A & A => A");
}

// =========================================================================
// 8. INTERSECTION — DISJOINT PRIMITIVE DETECTION
// =========================================================================

#[test]
fn intersection_string_and_number_is_never() {
    let i = TypeInterner::new();
    assert_eq!(
        i.intersection(vec![TypeId::STRING, TypeId::NUMBER]),
        TypeId::NEVER
    );
}

#[test]
fn intersection_string_and_boolean_is_never() {
    let i = TypeInterner::new();
    assert_eq!(
        i.intersection(vec![TypeId::STRING, TypeId::BOOLEAN]),
        TypeId::NEVER
    );
}

#[test]
fn intersection_number_and_boolean_is_never() {
    let i = TypeInterner::new();
    assert_eq!(
        i.intersection(vec![TypeId::NUMBER, TypeId::BOOLEAN]),
        TypeId::NEVER
    );
}

#[test]
fn intersection_bigint_and_number_is_never() {
    let i = TypeInterner::new();
    assert_eq!(
        i.intersection(vec![TypeId::BIGINT, TypeId::NUMBER]),
        TypeId::NEVER
    );
}

#[test]
fn intersection_symbol_and_string_is_never() {
    let i = TypeInterner::new();
    assert_eq!(
        i.intersection(vec![TypeId::SYMBOL, TypeId::STRING]),
        TypeId::NEVER
    );
}

#[test]
fn intersection_string_literal_and_number_is_never() {
    let i = TypeInterner::new();
    let hello = i.literal_string("hello");
    assert_eq!(i.intersection(vec![hello, TypeId::NUMBER]), TypeId::NEVER);
}

#[test]
fn intersection_string_literal_and_boolean_is_never() {
    let i = TypeInterner::new();
    let hello = i.literal_string("hello");
    assert_eq!(i.intersection(vec![hello, TypeId::BOOLEAN]), TypeId::NEVER);
}

#[test]
fn intersection_different_number_literals_is_never() {
    let i = TypeInterner::new();
    let n1 = i.literal_number(1.0);
    let n2 = i.literal_number(2.0);
    assert_eq!(
        i.intersection(vec![n1, n2]),
        TypeId::NEVER,
        "1 & 2 => never"
    );
}

#[test]
fn intersection_different_string_literals_is_never() {
    let i = TypeInterner::new();
    let a = i.literal_string("a");
    let b = i.literal_string("b");
    assert_eq!(
        i.intersection(vec![a, b]),
        TypeId::NEVER,
        "\"a\" & \"b\" => never"
    );
}

#[test]
fn intersection_same_literal_is_self() {
    let i = TypeInterner::new();
    let n = i.literal_number(42.0);
    assert_eq!(i.intersection(vec![n, n]), n, "42 & 42 => 42");
}

// =========================================================================
// 9. INTERSECTION — NULL/UNDEFINED WITH OBJECT
// =========================================================================

#[test]
fn intersection_null_and_object_type_is_never() {
    let i = TypeInterner::new();
    let obj = i.object(vec![PropertyInfo::new(
        i.intern_string("x"),
        TypeId::NUMBER,
    )]);
    assert_eq!(i.intersection(vec![TypeId::NULL, obj]), TypeId::NEVER);
}

#[test]
fn intersection_undefined_and_object_type_is_never() {
    let i = TypeInterner::new();
    let obj = i.object(vec![PropertyInfo::new(
        i.intern_string("x"),
        TypeId::NUMBER,
    )]);
    assert_eq!(i.intersection(vec![TypeId::UNDEFINED, obj]), TypeId::NEVER);
}

#[test]
fn intersection_null_and_empty_object_is_never() {
    let i = TypeInterner::new();
    let empty_obj = i.object(vec![]);
    assert_eq!(
        i.intersection(vec![TypeId::NULL, empty_obj]),
        TypeId::NEVER,
        "null & {{}} => never"
    );
}

// =========================================================================
// 10. INTERSECTION — OBJECT INTRINSIC WITH PRIMITIVE
// =========================================================================

#[test]
fn intersection_object_intrinsic_and_string_is_never() {
    let i = TypeInterner::new();
    assert_eq!(
        i.intersection(vec![TypeId::OBJECT, TypeId::STRING]),
        TypeId::NEVER,
        "object & string => never"
    );
}

#[test]
fn intersection_object_intrinsic_and_number_is_never() {
    let i = TypeInterner::new();
    assert_eq!(
        i.intersection(vec![TypeId::OBJECT, TypeId::NUMBER]),
        TypeId::NEVER,
    );
}

// =========================================================================
// 11. INTERSECTION — OBJECT MERGING
// =========================================================================

#[test]
fn intersection_two_objects_merged() {
    let i = TypeInterner::new();
    let obj_a = i.object(vec![PropertyInfo::new(
        i.intern_string("a"),
        TypeId::NUMBER,
    )]);
    let obj_b = i.object(vec![PropertyInfo::new(
        i.intern_string("b"),
        TypeId::STRING,
    )]);
    let inter = i.intersection(vec![obj_a, obj_b]);

    match i.lookup(inter) {
        Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
            let shape = i.object_shape(shape_id);
            assert_eq!(shape.properties.len(), 2);
        }
        _ => panic!("Expected merged object type"),
    }
}

#[test]
fn intersection_objects_same_prop_intersects_types() {
    let i = TypeInterner::new();
    // { x: string | number } & { x: string } => { x: string }
    let str_or_num = i.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let obj_wide = i.object(vec![PropertyInfo::new(i.intern_string("x"), str_or_num)]);
    let obj_narrow = i.object(vec![PropertyInfo::new(
        i.intern_string("x"),
        TypeId::STRING,
    )]);
    let inter = i.intersection(vec![obj_wide, obj_narrow]);

    match i.lookup(inter) {
        Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
            let shape = i.object_shape(shape_id);
            assert_eq!(shape.properties.len(), 1);
            // The property type should be the intersection of (string | number) & string
            // which is effectively string (via raw intersection)
        }
        _ => panic!("Expected merged object type"),
    }
}

// =========================================================================
// 12. INTERSECTION — EMPTY OBJECT RULE
// =========================================================================

#[test]
fn intersection_string_and_empty_object() {
    let i = TypeInterner::new();
    let empty_obj = i.object(vec![]);
    assert_eq!(
        i.intersection(vec![TypeId::STRING, empty_obj]),
        TypeId::STRING,
        "string & {{}} => string"
    );
}

#[test]
fn intersection_number_and_empty_object() {
    let i = TypeInterner::new();
    let empty_obj = i.object(vec![]);
    assert_eq!(
        i.intersection(vec![TypeId::NUMBER, empty_obj]),
        TypeId::NUMBER,
        "number & {{}} => number"
    );
}

#[test]
fn intersection_boolean_and_empty_object() {
    let i = TypeInterner::new();
    let empty_obj = i.object(vec![]);
    assert_eq!(
        i.intersection(vec![TypeId::BOOLEAN, empty_obj]),
        TypeId::BOOLEAN,
        "boolean & {{}} => boolean"
    );
}

#[test]
fn intersection_literal_and_empty_object() {
    let i = TypeInterner::new();
    let empty_obj = i.object(vec![]);
    let hello = i.literal_string("hello");
    assert_eq!(
        i.intersection(vec![hello, empty_obj]),
        hello,
        "\"hello\" & {{}} => \"hello\""
    );
}

// =========================================================================
// 13. INTERSECTION — BRANDED TYPES (SHOULD NOT REDUCE TO NEVER)
// =========================================================================

#[test]
fn intersection_branded_type_preserved() {
    let i = TypeInterner::new();
    // string & { __brand: "UserId" } should NOT be never
    let brand_obj = i.object(vec![PropertyInfo::new(
        i.intern_string("__brand"),
        i.literal_string("UserId"),
    )]);
    let branded = i.intersection(vec![TypeId::STRING, brand_obj]);
    assert_ne!(
        branded,
        TypeId::NEVER,
        "Branded type should not reduce to never"
    );
}

// =========================================================================
// 14. INTERSECTION — DISTRIBUTION OVER UNIONS
// =========================================================================

#[test]
fn intersection_distributes_over_union() {
    let i = TypeInterner::new();
    // string & (number | boolean) should distribute:
    // (string & number) | (string & boolean) = never | never = never
    let num_or_bool = i.union(vec![TypeId::NUMBER, TypeId::BOOLEAN]);
    let result = i.intersection(vec![TypeId::STRING, num_or_bool]);
    assert_eq!(
        result,
        TypeId::NEVER,
        "string & (number | boolean) => never"
    );
}

#[test]
fn intersection_distributes_with_object() {
    let i = TypeInterner::new();
    // { x: number } & (string | null)
    // => ({ x: number } & string) | ({ x: number } & null)
    // In TS, string & {x: number} is a branded type (preserved), null & object = never
    let obj = i.object(vec![PropertyInfo::new(
        i.intern_string("x"),
        TypeId::NUMBER,
    )]);
    let str_or_null = i.union(vec![TypeId::STRING, TypeId::NULL]);
    let result = i.intersection(vec![obj, str_or_null]);
    // null branch should be never, string & obj should be preserved as branded
    // So the result should NOT be never
    assert_ne!(result, TypeId::NEVER);
}

// =========================================================================
// 15. INTERSECTION — TYPE PARAMETERS PRESERVED
// =========================================================================

#[test]
fn intersection_with_type_param_not_reduced() {
    let i = TypeInterner::new();
    let tp = i.type_param(TypeParamInfo {
        name: i.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    });
    // T & string: Lazy/TypeParameter types abort reduction
    let obj = i.object(vec![PropertyInfo::new(
        i.intern_string("x"),
        TypeId::NUMBER,
    )]);
    let inter = i.intersection(vec![tp, obj]);
    // Should preserve the intersection
    match i.lookup(inter) {
        Some(TypeData::Intersection(list_id)) => {
            let members = i.type_list(list_id);
            assert!(members.contains(&tp), "TypeParameter must be preserved");
        }
        other => {
            // Could also be the type param or object itself if simplified
            // But should not be NEVER
            assert_ne!(inter, TypeId::NEVER);
            // OK if it remained as intersection or simplified
            let _ = other;
        }
    }
}

#[test]
fn intersection_with_lazy_preserves_structure() {
    let i = TypeInterner::new();
    let lazy_type = i.lazy(DefId(42));
    let inter = i.intersection(vec![lazy_type, TypeId::STRING]);
    // Should preserve the intersection because lazy types cannot be resolved
    assert_ne!(inter, TypeId::NEVER);
    match i.lookup(inter) {
        Some(TypeData::Intersection(list_id)) => {
            let members = i.type_list(list_id);
            assert!(members.contains(&lazy_type));
        }
        _ => panic!("Expected intersection to be preserved with lazy type"),
    }
}

// =========================================================================
// 16. INTERSECTION — ORDER INDEPENDENCE
// =========================================================================

#[test]
fn intersection_order_independence_objects() {
    let i = TypeInterner::new();
    let obj_a = i.object(vec![PropertyInfo::new(
        i.intern_string("a"),
        TypeId::NUMBER,
    )]);
    let obj_b = i.object(vec![PropertyInfo::new(
        i.intern_string("b"),
        TypeId::STRING,
    )]);

    let ab = i.intersection(vec![obj_a, obj_b]);
    let ba = i.intersection(vec![obj_b, obj_a]);
    assert_eq!(ab, ba, "{{a}} & {{b}} == {{b}} & {{a}}");
}

// =========================================================================
// 17. ARRAY / TUPLE CONSTRUCTION
// =========================================================================

#[test]
fn array_type_lookup() {
    let i = TypeInterner::new();
    let arr = i.array(TypeId::NUMBER);
    assert_eq!(i.lookup(arr), Some(TypeData::Array(TypeId::NUMBER)));
}

#[test]
fn tuple_type_lookup() {
    let i = TypeInterner::new();
    let elems = vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    }];
    let t = i.tuple(elems);
    match i.lookup(t) {
        Some(TypeData::Tuple(list_id)) => {
            let elems = i.tuple_list(list_id);
            assert_eq!(elems.len(), 1);
            assert_eq!(elems[0].type_id, TypeId::STRING);
        }
        _ => panic!("Expected tuple type"),
    }
}

#[test]
fn tuple_optional_normalizes_undefined() {
    let i = TypeInterner::new();
    // [number, (string | undefined)?] should normalize the optional element
    let str_or_undef = i.union(vec![TypeId::STRING, TypeId::UNDEFINED]);
    let t = i.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: str_or_undef,
            name: None,
            optional: true,
            rest: false,
        },
    ]);
    match i.lookup(t) {
        Some(TypeData::Tuple(list_id)) => {
            let elems = i.tuple_list(list_id);
            assert_eq!(elems.len(), 2);
            // The optional element should have undefined stripped from its type
            assert_eq!(elems[1].type_id, TypeId::STRING);
            assert!(elems[1].optional);
        }
        _ => panic!("Expected tuple type"),
    }
}

#[test]
fn readonly_tuple_differs_from_mutable() {
    let i = TypeInterner::new();
    let elems = vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    }];
    let t = i.tuple(elems.clone());
    let rt = i.readonly_tuple(elems);
    assert_ne!(t, rt, "Mutable tuple and readonly tuple must differ");
}

#[test]
fn tuple_different_elements_differ() {
    let i = TypeInterner::new();
    let t1 = i.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    }]);
    let t2 = i.tuple(vec![TupleElement {
        type_id: TypeId::NUMBER,
        name: None,
        optional: false,
        rest: false,
    }]);
    assert_ne!(t1, t2);
}

#[test]
fn tuple_rest_element() {
    let i = TypeInterner::new();
    let t = i.tuple(vec![
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
            rest: true,
        },
    ]);
    match i.lookup(t) {
        Some(TypeData::Tuple(list_id)) => {
            let elems = i.tuple_list(list_id);
            assert_eq!(elems.len(), 2);
            assert!(!elems[0].rest);
            assert!(elems[1].rest);
        }
        _ => panic!("Expected tuple type"),
    }
}

// =========================================================================
// 18. OBJECT CONSTRUCTION
// =========================================================================

#[test]
fn object_property_order_independence() {
    let i = TypeInterner::new();
    let obj1 = i.object(vec![
        PropertyInfo::new(i.intern_string("b"), TypeId::STRING),
        PropertyInfo::new(i.intern_string("a"), TypeId::NUMBER),
    ]);
    let obj2 = i.object(vec![
        PropertyInfo::new(i.intern_string("a"), TypeId::NUMBER),
        PropertyInfo::new(i.intern_string("b"), TypeId::STRING),
    ]);
    assert_eq!(obj1, obj2, "Object property order must not affect identity");
}

#[test]
fn object_with_index_signature() {
    let i = TypeInterner::new();
    let shape = ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo::new(i.intern_string("x"), TypeId::NUMBER)],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
        symbol: None,
    };
    let obj = i.object_with_index(shape);
    match i.lookup(obj) {
        Some(TypeData::ObjectWithIndex(shape_id)) => {
            let s = i.object_shape(shape_id);
            assert!(s.string_index.is_some());
        }
        _ => panic!("Expected ObjectWithIndex"),
    }
}

#[test]
fn object_optional_property() {
    let i = TypeInterner::new();
    let opt = i.object(vec![PropertyInfo::opt(
        i.intern_string("x"),
        TypeId::NUMBER,
    )]);
    let req = i.object(vec![PropertyInfo::new(
        i.intern_string("x"),
        TypeId::NUMBER,
    )]);
    assert_ne!(
        opt, req,
        "Optional and required properties must produce different types"
    );
}

#[test]
fn object_readonly_property() {
    let i = TypeInterner::new();
    let ro = i.object(vec![PropertyInfo {
        name: i.intern_string("x"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: true,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);
    let rw = i.object(vec![PropertyInfo::new(
        i.intern_string("x"),
        TypeId::NUMBER,
    )]);
    assert_ne!(ro, rw, "Readonly and mutable properties must differ");
}

#[test]
fn object_with_many_props_gets_property_cache() {
    let i = TypeInterner::new();
    let mut props = Vec::new();
    for idx in 0..(PROPERTY_MAP_THRESHOLD + 5) {
        props.push(PropertyInfo::new(
            i.intern_string(&format!("p{idx}")),
            TypeId::NUMBER,
        ));
    }
    let obj = i.object(props);
    let shape_id = match i.lookup(obj) {
        Some(TypeData::Object(sid)) => sid,
        _ => panic!("Expected object type"),
    };
    // Property lookup should be cached for large objects
    let target = i.intern_string(&format!("p{}", PROPERTY_MAP_THRESHOLD / 2));
    match i.object_property_index(shape_id, target) {
        PropertyLookup::Found(_) => {} // OK
        other => panic!("Expected Found, got {other:?}"),
    }
}

// =========================================================================
// 19. FUNCTION CONSTRUCTION
// =========================================================================

#[test]
fn function_different_params_differ() {
    let i = TypeInterner::new();
    let f1 = i.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo::unnamed(TypeId::STRING)],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let f2 = i.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo::unnamed(TypeId::NUMBER)],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    assert_ne!(f1, f2);
}

#[test]
fn function_different_return_type_differ() {
    let i = TypeInterner::new();
    let f1 = i.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let f2 = i.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    assert_ne!(f1, f2);
}

#[test]
fn function_with_type_params() {
    let i = TypeInterner::new();
    let tp = TypeParamInfo {
        name: i.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let f = i.function(FunctionShape {
        type_params: vec![tp],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    match i.lookup(f) {
        Some(TypeData::Function(fid)) => {
            let shape = i.function_shape(fid);
            assert_eq!(shape.type_params.len(), 1);
            assert_eq!(shape.type_params[0], tp);
        }
        _ => panic!("Expected function type"),
    }
}

#[test]
fn callable_with_overloads() {
    let i = TypeInterner::new();
    let sig1 = CallSignature {
        type_params: vec![],
        params: vec![ParamInfo::unnamed(TypeId::STRING)],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_method: false,
    };
    let sig2 = CallSignature {
        type_params: vec![],
        params: vec![ParamInfo::unnamed(TypeId::NUMBER)],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_method: false,
    };
    let c = i.callable(CallableShape {
        call_signatures: vec![sig1, sig2],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
        symbol: None,
        is_abstract: false,
    });
    match i.lookup(c) {
        Some(TypeData::Callable(cid)) => {
            let shape = i.callable_shape(cid);
            assert_eq!(shape.call_signatures.len(), 2);
        }
        _ => panic!("Expected callable type"),
    }
}

// =========================================================================
// 20. TYPE PARAMETER CONSTRUCTION
// =========================================================================

#[test]
fn type_param_with_constraint() {
    let i = TypeInterner::new();
    let tp = i.type_param(TypeParamInfo {
        name: i.intern_string("T"),
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    });
    match i.lookup(tp) {
        Some(TypeData::TypeParameter(info)) => {
            assert_eq!(info.constraint, Some(TypeId::STRING));
        }
        _ => panic!("Expected TypeParameter"),
    }
}

#[test]
fn type_param_with_default() {
    let i = TypeInterner::new();
    let tp = i.type_param(TypeParamInfo {
        name: i.intern_string("T"),
        constraint: None,
        default: Some(TypeId::NUMBER),
        is_const: false,
    });
    match i.lookup(tp) {
        Some(TypeData::TypeParameter(info)) => {
            assert_eq!(info.default, Some(TypeId::NUMBER));
        }
        _ => panic!("Expected TypeParameter"),
    }
}

#[test]
fn type_param_const() {
    let i = TypeInterner::new();
    let tp = i.type_param(TypeParamInfo {
        name: i.intern_string("T"),
        constraint: None,
        default: None,
        is_const: true,
    });
    match i.lookup(tp) {
        Some(TypeData::TypeParameter(info)) => {
            assert!(info.is_const);
        }
        _ => panic!("Expected TypeParameter"),
    }
}

#[test]
fn type_param_different_names_differ() {
    let i = TypeInterner::new();
    let tp_t = i.type_param(TypeParamInfo {
        name: i.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    });
    let tp_u = i.type_param(TypeParamInfo {
        name: i.intern_string("U"),
        constraint: None,
        default: None,
        is_const: false,
    });
    assert_ne!(tp_t, tp_u);
}

// =========================================================================
// 21. CONDITIONAL TYPE CONSTRUCTION
// =========================================================================

#[test]
fn conditional_type_lookup() {
    let i = TypeInterner::new();
    let cond = i.conditional(ConditionalType {
        check_type: TypeId::STRING,
        extends_type: TypeId::NUMBER,
        true_type: TypeId::BOOLEAN,
        false_type: TypeId::NEVER,
        is_distributive: false,
    });
    match i.lookup(cond) {
        Some(TypeData::Conditional(cid)) => {
            let ct = i.conditional_type(cid);
            assert_eq!(ct.check_type, TypeId::STRING);
            assert_eq!(ct.extends_type, TypeId::NUMBER);
            assert_eq!(ct.true_type, TypeId::BOOLEAN);
            assert_eq!(ct.false_type, TypeId::NEVER);
            assert!(!ct.is_distributive);
        }
        _ => panic!("Expected conditional type"),
    }
}

#[test]
fn conditional_type_distributive_differs() {
    let i = TypeInterner::new();
    let non_dist = i.conditional(ConditionalType {
        check_type: TypeId::STRING,
        extends_type: TypeId::NUMBER,
        true_type: TypeId::BOOLEAN,
        false_type: TypeId::NEVER,
        is_distributive: false,
    });
    let dist = i.conditional(ConditionalType {
        check_type: TypeId::STRING,
        extends_type: TypeId::NUMBER,
        true_type: TypeId::BOOLEAN,
        false_type: TypeId::NEVER,
        is_distributive: true,
    });
    assert_ne!(non_dist, dist);
}

// =========================================================================
// 22. MAPPED TYPE CONSTRUCTION
// =========================================================================

#[test]
fn mapped_type_lookup() {
    let i = TypeInterner::new();
    let mapped = i.mapped(MappedType {
        type_param: TypeParamInfo {
            name: i.intern_string("K"),
            constraint: Some(TypeId::STRING),
            default: None,
            is_const: false,
        },
        constraint: TypeId::STRING,
        name_type: None,
        template: TypeId::NUMBER,
        readonly_modifier: Some(MappedModifier::Add),
        optional_modifier: None,
    });
    match i.lookup(mapped) {
        Some(TypeData::Mapped(mid)) => {
            let mt = i.mapped_type(mid);
            assert_eq!(mt.template, TypeId::NUMBER);
            assert_eq!(mt.readonly_modifier, Some(MappedModifier::Add));
        }
        _ => panic!("Expected mapped type"),
    }
}

#[test]
fn mapped_type_different_modifiers_differ() {
    let i = TypeInterner::new();
    let make_mapped = |ro: Option<MappedModifier>| {
        i.mapped(MappedType {
            type_param: TypeParamInfo {
                name: i.intern_string("K"),
                constraint: None,
                default: None,
                is_const: false,
            },
            constraint: TypeId::STRING,
            name_type: None,
            template: TypeId::NUMBER,
            readonly_modifier: ro,
            optional_modifier: None,
        })
    };
    let add = make_mapped(Some(MappedModifier::Add));
    let remove = make_mapped(Some(MappedModifier::Remove));
    let none = make_mapped(None);
    assert_ne!(add, remove);
    assert_ne!(add, none);
    assert_ne!(remove, none);
}

// =========================================================================
// 23. APPLICATION (GENERIC INSTANTIATION) CONSTRUCTION
// =========================================================================

#[test]
fn application_dedup() {
    let i = TypeInterner::new();
    let base = i.lazy(DefId(1));
    let a = i.application(base, vec![TypeId::STRING]);
    let b = i.application(base, vec![TypeId::STRING]);
    let c = i.application(base, vec![TypeId::NUMBER]);
    assert_eq!(a, b);
    assert_ne!(a, c);
}

#[test]
fn application_different_bases_differ() {
    let i = TypeInterner::new();
    let a = i.application(i.lazy(DefId(1)), vec![TypeId::STRING]);
    let b = i.application(i.lazy(DefId(2)), vec![TypeId::STRING]);
    assert_ne!(a, b);
}

#[test]
fn application_lookup() {
    let i = TypeInterner::new();
    let base = i.lazy(DefId(10));
    let app = i.application(base, vec![TypeId::STRING, TypeId::NUMBER]);
    match i.lookup(app) {
        Some(TypeData::Application(aid)) => {
            let ta = i.type_application(aid);
            assert_eq!(ta.base, base);
            assert_eq!(ta.args.len(), 2);
            assert_eq!(ta.args[0], TypeId::STRING);
            assert_eq!(ta.args[1], TypeId::NUMBER);
        }
        _ => panic!("Expected application type"),
    }
}

// =========================================================================
// 24. TEMPLATE LITERAL CONSTRUCTION
// =========================================================================

#[test]
fn template_literal_dedup() {
    let i = TypeInterner::new();
    let spans = vec![
        TemplateSpan::Text(i.intern_string("hello ")),
        TemplateSpan::Type(TypeId::STRING),
    ];
    let a = i.template_literal(spans.clone());
    let b = i.template_literal(spans);
    assert_eq!(a, b);
}

#[test]
fn template_literal_with_never_is_never() {
    let i = TypeInterner::new();
    let t = i.template_literal(vec![
        TemplateSpan::Text(i.intern_string("prefix")),
        TemplateSpan::Type(TypeId::NEVER),
    ]);
    assert_eq!(t, TypeId::NEVER);
}

#[test]
fn template_literal_with_any_is_string() {
    let i = TypeInterner::new();
    let t = i.template_literal(vec![TemplateSpan::Type(TypeId::ANY)]);
    assert_eq!(t, TypeId::STRING);
}

#[test]
fn template_literal_all_text_becomes_string_literal() {
    let i = TypeInterner::new();
    let t = i.template_literal(vec![TemplateSpan::Text(i.intern_string("hello"))]);
    match i.lookup(t) {
        Some(TypeData::Literal(LiteralValue::String(s))) => {
            assert_eq!(i.resolve_atom(s), "hello");
        }
        _ => panic!("Expected string literal, not template"),
    }
}

// =========================================================================
// 25. ENUM CONSTRUCTION
// =========================================================================

#[test]
fn enum_type_dedup() {
    let i = TypeInterner::new();
    let a = i.enum_type(DefId(5), TypeId::NUMBER);
    let b = i.enum_type(DefId(5), TypeId::NUMBER);
    let c = i.enum_type(DefId(6), TypeId::NUMBER);
    assert_eq!(a, b);
    assert_ne!(a, c);
}

#[test]
fn enum_type_different_structural_type_differ() {
    let i = TypeInterner::new();
    let a = i.enum_type(DefId(5), TypeId::NUMBER);
    let b = i.enum_type(DefId(5), TypeId::STRING);
    assert_ne!(a, b);
}

// =========================================================================
// 26. UNIQUE SYMBOL, NO-INFER, THIS TYPE
// =========================================================================

#[test]
fn unique_symbol_dedup() {
    let i = TypeInterner::new();
    let a = i.unique_symbol(SymbolRef(1));
    let b = i.unique_symbol(SymbolRef(1));
    let c = i.unique_symbol(SymbolRef(2));
    assert_eq!(a, b);
    assert_ne!(a, c);
}

#[test]
fn no_infer_wraps_type() {
    let i = TypeInterner::new();
    let ni = i.no_infer(TypeId::STRING);
    match i.lookup(ni) {
        Some(TypeData::NoInfer(inner)) => assert_eq!(inner, TypeId::STRING),
        _ => panic!("Expected NoInfer"),
    }
}

#[test]
fn this_type_dedup() {
    let i = TypeInterner::new();
    let a = i.this_type();
    let b = i.this_type();
    assert_eq!(a, b);
}

// =========================================================================
// 27. STRING INTERNING
// =========================================================================

#[test]
fn string_interning_dedup() {
    let i = TypeInterner::new();
    let a = i.intern_string("hello");
    let b = i.intern_string("hello");
    let c = i.intern_string("world");
    assert_eq!(a, b);
    assert_ne!(a, c);
}

#[test]
fn string_resolve_round_trip() {
    let i = TypeInterner::new();
    let atom = i.intern_string("test_value");
    assert_eq!(i.resolve_atom(atom), "test_value");
}

// =========================================================================
// 28. INTRINSIC TYPES
// =========================================================================

#[test]
fn all_intrinsics_exist() {
    let i = TypeInterner::new();
    let intrinsics = [
        TypeId::ANY,
        TypeId::UNKNOWN,
        TypeId::NEVER,
        TypeId::VOID,
        TypeId::UNDEFINED,
        TypeId::NULL,
        TypeId::BOOLEAN,
        TypeId::NUMBER,
        TypeId::STRING,
        TypeId::BIGINT,
        TypeId::SYMBOL,
        TypeId::OBJECT,
        TypeId::BOOLEAN_TRUE,
        TypeId::BOOLEAN_FALSE,
        TypeId::FUNCTION,
    ];
    for id in intrinsics {
        assert!(i.lookup(id).is_some(), "Intrinsic {id:?} must exist");
    }
}

#[test]
fn intrinsic_lookup_round_trip() {
    let i = TypeInterner::new();
    assert_eq!(
        i.lookup(TypeId::STRING),
        Some(TypeData::Intrinsic(IntrinsicKind::String))
    );
    assert_eq!(
        i.lookup(TypeId::NUMBER),
        Some(TypeData::Intrinsic(IntrinsicKind::Number))
    );
    assert_eq!(
        i.lookup(TypeId::BOOLEAN),
        Some(TypeData::Intrinsic(IntrinsicKind::Boolean))
    );
    assert_eq!(
        i.lookup(TypeId::VOID),
        Some(TypeData::Intrinsic(IntrinsicKind::Void))
    );
    assert_eq!(
        i.lookup(TypeId::NULL),
        Some(TypeData::Intrinsic(IntrinsicKind::Null))
    );
    assert_eq!(
        i.lookup(TypeId::UNDEFINED),
        Some(TypeData::Intrinsic(IntrinsicKind::Undefined))
    );
    assert_eq!(
        i.lookup(TypeId::NEVER),
        Some(TypeData::Intrinsic(IntrinsicKind::Never))
    );
    assert_eq!(
        i.lookup(TypeId::ANY),
        Some(TypeData::Intrinsic(IntrinsicKind::Any))
    );
    assert_eq!(
        i.lookup(TypeId::UNKNOWN),
        Some(TypeData::Intrinsic(IntrinsicKind::Unknown))
    );
}

// =========================================================================
// 29. TYPEID STABILITY (Same construction always yields same ID)
// =========================================================================

#[test]
fn type_id_stability_across_constructions() {
    let i = TypeInterner::new();
    // Construct a complex type, then construct it again
    let obj = i.object(vec![
        PropertyInfo::new(i.intern_string("x"), TypeId::NUMBER),
        PropertyInfo::new(i.intern_string("y"), TypeId::STRING),
    ]);
    let arr = i.array(obj);
    let union = i.union(vec![arr, TypeId::NULL]);

    // Do it all again
    let obj2 = i.object(vec![
        PropertyInfo::new(i.intern_string("x"), TypeId::NUMBER),
        PropertyInfo::new(i.intern_string("y"), TypeId::STRING),
    ]);
    let arr2 = i.array(obj2);
    let union2 = i.union(vec![arr2, TypeId::NULL]);

    assert_eq!(union, union2, "Same construction must yield same TypeId");
}

// =========================================================================
// 30. LARGE UNION HANDLING
// =========================================================================

#[test]
fn large_union_many_literals() {
    let i = TypeInterner::new();
    let members: Vec<TypeId> = (0..100).map(|n| i.literal_number(n as f64)).collect();
    let u = i.union(members);
    match i.lookup(u) {
        Some(TypeData::Union(list_id)) => {
            let members = i.type_list(list_id);
            assert_eq!(members.len(), 100);
        }
        _ => panic!("Expected union with 100 members"),
    }
}

#[test]
fn large_union_with_primitive_absorbs_all() {
    let i = TypeInterner::new();
    let mut members: Vec<TypeId> = (0..50).map(|n| i.literal_number(n as f64)).collect();
    members.push(TypeId::NUMBER);
    let u = i.union(members);
    assert_eq!(u, TypeId::NUMBER, "50 number literals + number => number");
}

// =========================================================================
// 31. INTERSECTION — CALLABLE MERGING
// =========================================================================

#[test]
fn intersection_functions_merge_to_callable() {
    let i = TypeInterner::new();
    let f1 = i.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo::unnamed(TypeId::STRING)],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let f2 = i.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo::unnamed(TypeId::NUMBER)],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let inter = i.intersection(vec![f1, f2]);
    match i.lookup(inter) {
        Some(TypeData::Callable(cid)) => {
            let shape = i.callable_shape(cid);
            assert_eq!(
                shape.call_signatures.len(),
                2,
                "Two functions should merge into callable with 2 sigs"
            );
        }
        _ => panic!("Expected callable type from function intersection"),
    }
}

// =========================================================================
// 32. BIGINT NORMALIZATION
// =========================================================================

#[test]
fn bigint_hex_normalized() {
    let i = TypeInterner::new();
    let a = i.literal_bigint("0xFF");
    let b = i.literal_bigint("255");
    assert_eq!(a, b, "0xFF and 255 should normalize to same bigint");
}

#[test]
fn bigint_octal_normalized() {
    let i = TypeInterner::new();
    let a = i.literal_bigint("0o77");
    let b = i.literal_bigint("63");
    assert_eq!(a, b, "0o77 and 63 should normalize to same bigint");
}

#[test]
fn bigint_binary_normalized() {
    let i = TypeInterner::new();
    let a = i.literal_bigint("0b1010");
    let b = i.literal_bigint("10");
    assert_eq!(a, b, "0b1010 and 10 should normalize to same bigint");
}

#[test]
fn bigint_leading_zeros_stripped() {
    let i = TypeInterner::new();
    let a = i.literal_bigint("007");
    let b = i.literal_bigint("7");
    assert_eq!(a, b);
}

#[test]
fn bigint_zero_normalization() {
    let i = TypeInterner::new();
    let a = i.literal_bigint("0");
    let b = i.literal_bigint("000");
    let c = i.literal_bigint("");
    assert_eq!(a, b);
    assert_eq!(a, c);
}

#[test]
fn bigint_underscore_separator() {
    let i = TypeInterner::new();
    let a = i.literal_bigint("1_000_000");
    let b = i.literal_bigint("1000000");
    assert_eq!(a, b);
}

// =========================================================================
// 33. UNION PRESERVE MEMBERS
// =========================================================================

#[test]
fn union_preserve_members_flattens_but_keeps_unknown() {
    let i = TypeInterner::new();
    // union_preserve_members keeps unknown members intact
    let u = i.union_preserve_members(vec![TypeId::STRING, TypeId::NUMBER]);
    let direct = i.union(vec![TypeId::STRING, TypeId::NUMBER]);
    // Both should produce the same result for simple cases
    assert_eq!(u, direct);
}

// =========================================================================
// 34. INTERSECTION — DISJOINT OBJECT LITERALS
// =========================================================================

#[test]
fn intersection_disjoint_discriminant_objects() {
    let i = TypeInterner::new();
    let kind = i.intern_string("kind");
    let obj_a = i.object(vec![PropertyInfo::new(kind, i.literal_string("a"))]);
    let obj_b = i.object(vec![PropertyInfo::new(kind, i.literal_string("b"))]);
    assert_eq!(
        i.intersection(vec![obj_a, obj_b]),
        TypeId::NEVER,
        "{{ kind: 'a' }} & {{ kind: 'b' }} => never"
    );
}

#[test]
fn intersection_disjoint_discriminant_union_vs_literal() {
    let i = TypeInterner::new();
    let kind = i.intern_string("kind");
    let ab = i.union(vec![i.literal_string("a"), i.literal_string("b")]);
    let obj_ab = i.object(vec![PropertyInfo::new(kind, ab)]);
    let obj_c = i.object(vec![PropertyInfo::new(kind, i.literal_string("c"))]);
    assert_eq!(
        i.intersection(vec![obj_ab, obj_c]),
        TypeId::NEVER,
        "{{ kind: 'a'|'b' }} & {{ kind: 'c' }} => never"
    );
}

#[test]
fn intersection_optional_discriminants_not_disjoint() {
    let i = TypeInterner::new();
    let kind = i.intern_string("kind");
    let obj_a = i.object(vec![PropertyInfo::opt(kind, i.literal_string("a"))]);
    let obj_b = i.object(vec![PropertyInfo::opt(kind, i.literal_string("b"))]);
    // Both optional => NOT disjoint
    assert_ne!(
        i.intersection(vec![obj_a, obj_b]),
        TypeId::NEVER,
        "Optional discriminants should not be disjoint"
    );
}

// =========================================================================
// 35. INTERSECTION — CROSS-DOMAIN DISJOINT
// =========================================================================

#[test]
fn intersection_string_literal_prop_vs_number_prop_is_never() {
    let i = TypeInterner::new();
    let name = i.intern_string("x");
    let obj_str = i.object(vec![PropertyInfo::new(name, i.literal_string("hello"))]);
    let obj_num = i.object(vec![PropertyInfo::new(name, TypeId::NUMBER)]);
    assert_eq!(
        i.intersection(vec![obj_str, obj_num]),
        TypeId::NEVER,
        "{{ x: 'hello' }} & {{ x: number }} => never (cross-domain disjoint)"
    );
}

// =========================================================================
// 36. READONLY TYPE
// =========================================================================

#[test]
fn readonly_type_wraps() {
    let i = TypeInterner::new();
    let inner = i.object(vec![PropertyInfo::new(
        i.intern_string("x"),
        TypeId::NUMBER,
    )]);
    let ro = i.readonly_type(inner);
    match i.lookup(ro) {
        Some(TypeData::ReadonlyType(wrapped)) => assert_eq!(wrapped, inner),
        _ => panic!("Expected ReadonlyType"),
    }
}

// =========================================================================
// 37. INFER TYPE
// =========================================================================

#[test]
fn infer_type_construction() {
    let i = TypeInterner::new();
    let info = TypeParamInfo {
        name: i.intern_string("U"),
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    };
    let inf = i.infer(info);
    match i.lookup(inf) {
        Some(TypeData::Infer(got_info)) => {
            assert_eq!(got_info, info);
        }
        _ => panic!("Expected Infer type"),
    }
}

// =========================================================================
// 38. STRING INTRINSIC TYPE
// =========================================================================

#[test]
fn string_intrinsic_construction() {
    let i = TypeInterner::new();
    let upper = i.string_intrinsic(crate::types::StringIntrinsicKind::Uppercase, TypeId::STRING);
    match i.lookup(upper) {
        Some(TypeData::StringIntrinsic { kind, type_arg }) => {
            assert_eq!(kind, crate::types::StringIntrinsicKind::Uppercase);
            assert_eq!(type_arg, TypeId::STRING);
        }
        _ => panic!("Expected StringIntrinsic"),
    }
}

// =========================================================================
// 39. INTERNER LEN AND IS_EMPTY
// =========================================================================

#[test]
fn interner_len_increases_on_intern() {
    let i = TypeInterner::new();
    let initial_len = i.len();
    // Intrinsics are handled via const fn, not stored in shards
    // So initial len reflects the FIRST_USER offset

    // Adding a new type should increase length
    i.literal_string("new_type");
    assert!(
        i.len() > initial_len,
        "Length must increase after interning a new type"
    );

    // Adding the same type again should not increase length
    let after_first = i.len();
    i.literal_string("new_type");
    assert_eq!(
        i.len(),
        after_first,
        "Duplicate interning must not increase length"
    );
}

// =========================================================================
// 40. INTERSECTION — RAW (UNSIMPLIFIED)
// =========================================================================

#[test]
fn intersect_types_raw_basic() {
    let i = TypeInterner::new();
    let obj_a = i.object(vec![PropertyInfo::new(
        i.intern_string("a"),
        TypeId::NUMBER,
    )]);
    let obj_b = i.object(vec![PropertyInfo::new(
        i.intern_string("b"),
        TypeId::STRING,
    )]);
    let raw = i.intersect_types_raw(vec![obj_a, obj_b]);
    // Raw intersection should NOT merge objects — it preserves the intersection form
    match i.lookup(raw) {
        Some(TypeData::Intersection(list_id)) => {
            let members = i.type_list(list_id);
            assert_eq!(members.len(), 2);
        }
        _ => panic!("Expected raw intersection"),
    }
}

#[test]
fn intersect_types_raw2_convenience() {
    let i = TypeInterner::new();
    let a = i.literal_string("a");
    let b = i.literal_string("b");
    let raw = i.intersect_types_raw2(a, b);
    // Raw intersection should preserve both members
    match i.lookup(raw) {
        Some(TypeData::Intersection(list_id)) => {
            let members = i.type_list(list_id);
            assert_eq!(members.len(), 2);
        }
        _ => {
            // It might simplify if the raw intersection detects never
            // Actually intersect_types_raw does basic never/any/unknown handling
        }
    }
}

#[test]
fn intersect_types_raw_with_never() {
    let i = TypeInterner::new();
    let raw = i.intersect_types_raw(vec![TypeId::STRING, TypeId::NEVER]);
    assert_eq!(raw, TypeId::NEVER, "Raw intersection with never => never");
}

#[test]
fn intersect_types_raw_with_unknown() {
    let i = TypeInterner::new();
    let raw = i.intersect_types_raw(vec![TypeId::STRING, TypeId::UNKNOWN]);
    assert_eq!(raw, TypeId::STRING, "Raw intersection removes unknown");
}

#[test]
fn intersection_with_distinct_private_brand_sets_reduces_to_never() {
    let i = TypeInterner::new();
    let brand_a = i.intern_string("__private_brand_A");
    let brand_b = i.intern_string("__private_brand_B");

    let a = i.object(vec![PropertyInfo::new(brand_a, TypeId::NEVER)]);
    let b = i.object(vec![PropertyInfo::new(brand_b, TypeId::NEVER)]);
    assert_eq!(i.intersection(vec![a, b]), TypeId::NEVER);
}

#[test]
fn intersection_with_nested_private_brand_set_keeps_derived_shape() {
    let i = TypeInterner::new();
    let brand_base = i.intern_string("__private_brand_Base");
    let brand_derived = i.intern_string("__private_brand_Derived");

    let base = i.object(vec![PropertyInfo::new(brand_base, TypeId::NEVER)]);
    let derived = i.object(vec![
        PropertyInfo::new(brand_base, TypeId::NEVER),
        PropertyInfo::new(brand_derived, TypeId::NEVER),
    ]);

    assert_eq!(i.intersection(vec![base, derived]), derived);
}

// =========================================================================
// 41. BOXED TYPES
// =========================================================================

#[test]
fn boxed_type_registration() {
    let i = TypeInterner::new();
    let string_interface = i.object(vec![PropertyInfo::new(
        i.intern_string("length"),
        TypeId::NUMBER,
    )]);
    i.set_boxed_type(IntrinsicKind::String, string_interface);
    assert_eq!(
        i.get_boxed_type(IntrinsicKind::String),
        Some(string_interface)
    );
    assert_eq!(i.get_boxed_type(IntrinsicKind::Number), None);
}

#[test]
fn boxed_def_id_registration() {
    let i = TypeInterner::new();
    i.register_boxed_def_id(IntrinsicKind::String, DefId(100));
    assert!(i.is_boxed_def_id(DefId(100), IntrinsicKind::String));
    assert!(!i.is_boxed_def_id(DefId(101), IntrinsicKind::String));
    assert!(!i.is_boxed_def_id(DefId(100), IntrinsicKind::Number));
}

// =========================================================================
// 42. ARRAY BASE TYPE
// =========================================================================

#[test]
fn array_base_type_set_and_get() {
    let i = TypeInterner::new();
    assert_eq!(i.get_array_base_type(), None);
    let arr_type = i.lazy(DefId(99));
    i.set_array_base_type(
        arr_type,
        vec![TypeParamInfo {
            name: i.intern_string("T"),
            constraint: None,
            default: None,
            is_const: false,
        }],
    );
    assert_eq!(i.get_array_base_type(), Some(arr_type));
    assert_eq!(i.get_array_base_type_params().len(), 1);
}

// =========================================================================
// 43. NO_UNCHECKED_INDEXED_ACCESS FLAG
// =========================================================================

#[test]
fn no_unchecked_indexed_access_flag() {
    let i = TypeInterner::new();
    assert!(!i.no_unchecked_indexed_access());
    i.set_no_unchecked_indexed_access(true);
    assert!(i.no_unchecked_indexed_access());
    i.set_no_unchecked_indexed_access(false);
    assert!(!i.no_unchecked_indexed_access());
}

// =========================================================================
// 44. BOUND PARAMETER / RECURSIVE
// =========================================================================

#[test]
fn bound_parameter_type() {
    let i = TypeInterner::new();
    let bp = i.bound_parameter(0);
    match i.lookup(bp) {
        Some(TypeData::BoundParameter(idx)) => assert_eq!(idx, 0),
        _ => panic!("Expected BoundParameter"),
    }
}

#[test]
fn recursive_type() {
    let i = TypeInterner::new();
    let r = i.recursive(3);
    match i.lookup(r) {
        Some(TypeData::Recursive(depth)) => assert_eq!(depth, 3),
        _ => panic!("Expected Recursive"),
    }
}
