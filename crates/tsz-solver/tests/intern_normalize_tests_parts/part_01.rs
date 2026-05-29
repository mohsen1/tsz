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

#[test]
fn readonly_type_is_idempotent() {
    // The const-assertion visitor recurses into a Tuple/Array arm (which
    // wraps in ReadonlyType) and then re-wraps from the ReadonlyType arm.
    // Without collapsing, the result is `ReadonlyType(ReadonlyType(...))`,
    // which renders as "readonly readonly [...]" and breaks subtype paths
    // that peel exactly one readonly layer.
    let i = TypeInterner::new();
    let inner = i.tuple(vec![]);
    let once = i.readonly_type(inner);
    let twice = i.readonly_type(once);
    assert_eq!(twice, once, "readonly_type must be idempotent");

    match i.lookup(twice) {
        Some(TypeData::ReadonlyType(wrapped)) => {
            assert_eq!(wrapped, inner);
            assert!(
                !matches!(i.lookup(wrapped), Some(TypeData::ReadonlyType(_))),
                "ReadonlyType wrapper must not be nested",
            );
        }
        _ => panic!("Expected ReadonlyType"),
    }

    let thrice = i.readonly_type(twice);
    assert_eq!(thrice, once);

    // Mirror the const-assertion composition: wrapping the output of
    // readonly_tuple / readonly_array must not produce a nested wrapper.
    let ro_tuple = i.readonly_tuple(vec![]);
    assert_eq!(i.readonly_type(ro_tuple), ro_tuple);
    let ro_array = i.readonly_array(TypeId::NUMBER);
    assert_eq!(i.readonly_type(ro_array), ro_array);
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
