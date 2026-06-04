/// Number literal unions are canonical: the same set of members always
/// produces the same TypeId regardless of input order.
#[test]
fn test_union_number_literal_ordering() {
    let interner = TypeInterner::new();

    let n3 = interner.literal_number(3.0);
    let n1 = interner.literal_number(1.0);
    let n2 = interner.literal_number(2.0);

    let union_mixed = interner.union(vec![n3, n1, n2]);
    let union_sorted = interner.union(vec![n1, n2, n3]);
    let union_rev = interner.union(vec![n2, n3, n1]);

    assert_eq!(
        union_mixed, union_sorted,
        "Number literal unions should be order-independent"
    );
    assert_eq!(
        union_mixed, union_rev,
        "Number literal unions should be order-independent (reversed)"
    );

    if let Some(TypeData::Union(list_id)) = interner.lookup(union_mixed) {
        let members = interner.type_list(list_id);
        assert_eq!(members.len(), 3);
    }
}

/// Application types (generic instantiations) in unions should sort by their base
/// type's DefId ordering, not by raw TypeId. This ensures that `I1<number> | I2<number>`
/// displays in source declaration order (I1 before I2) matching tsc behavior.
#[test]
fn test_union_application_types_sort_by_base_def_id() {
    use crate::def::DefId;

    let interner = TypeInterner::new();

    // Create two Lazy types with known DefIds — lower DefId = declared first in source
    let def_i1 = DefId(10); // I1 declared first
    let def_i2 = DefId(20); // I2 declared second
    let lazy_i1 = interner.lazy(def_i1);
    let lazy_i2 = interner.lazy(def_i2);

    // Create Application types: I1<number> and I2<number>
    let app_i1_num = interner.application(lazy_i1, vec![TypeId::NUMBER]);
    let app_i2_num = interner.application(lazy_i2, vec![TypeId::NUMBER]);

    // Create union in REVERSE order: I2<number> | I1<number>
    let union_reversed = interner.union(vec![app_i2_num, app_i1_num]);

    // The normalized union should sort I1<number> before I2<number> (lower DefId first)
    if let Some(TypeData::Union(list_id)) = interner.lookup(union_reversed) {
        let members = interner.type_list(list_id);
        assert_eq!(members.len(), 2, "Union should have 2 members");
        assert_eq!(
            members[0], app_i1_num,
            "I1<number> (DefId=10) should come first in the union"
        );
        assert_eq!(
            members[1], app_i2_num,
            "I2<number> (DefId=20) should come second in the union"
        );
    } else {
        panic!("Expected Union type");
    }

    // Union created in source order should produce the same TypeId
    let union_ordered = interner.union(vec![app_i1_num, app_i2_num]);
    assert_eq!(
        union_reversed, union_ordered,
        "Application union ordering should be deterministic regardless of input order"
    );
}

/// Symbol-backed and anonymous object members must keep order-independent identity
/// without forcing symbol-first display order for object/object pairs.
#[test]
fn test_union_object_members_sort_total_with_mixed_symbols() {
    let interner = TypeInterner::new();

    let named_high = interner.object_type_from_shape(interner.intern_object_shape(ObjectShape {
        symbol: Some(SymbolId(20)),
        ..Default::default()
    }));
    let anonymous = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        TypeId::STRING,
    )]);
    let named_low = interner.object_type_from_shape(interner.intern_object_shape(ObjectShape {
        symbol: Some(SymbolId(10)),
        ..Default::default()
    }));

    let union_a = interner.union(vec![named_high, anonymous, named_low]);
    let union_b = interner.union(vec![anonymous, named_low, named_high]);
    let union_c = interner.union(vec![named_low, named_high, anonymous]);

    assert_eq!(union_a, union_b);
    assert_eq!(union_a, union_c);
}

/// Application types with the same base but different args should sort by args.
#[test]
fn test_union_application_types_same_base_sort_by_args() {
    use crate::def::DefId;

    let interner = TypeInterner::new();

    let def_id = DefId(10);
    let lazy_base = interner.lazy(def_id);

    // Create Application types: Foo<number> and Foo<string>
    let app_num = interner.application(lazy_base, vec![TypeId::NUMBER]);
    let app_str = interner.application(lazy_base, vec![TypeId::STRING]);

    // Create union in both orders — should normalize to same result
    let union_a = interner.union(vec![app_num, app_str]);
    let union_b = interner.union(vec![app_str, app_num]);
    assert_eq!(
        union_a, union_b,
        "Same-base application unions should be order-independent"
    );

    // Verify it's a union (not collapsed)
    if let Some(TypeData::Union(list_id)) = interner.lookup(union_a) {
        let members = interner.type_list(list_id);
        assert_eq!(members.len(), 2, "Union should have 2 members");
    } else {
        panic!("Expected Union type");
    }
}

#[test]
fn test_union_member_order_uses_allocation_order() {
    // Short string literals (1-2 chars) are sorted by content to match tsc's
    // lib.d.ts pre-allocation order. tsc pre-creates common short string
    // literals during lib processing in roughly alphabetical order.
    // Longer strings use allocation order (source encounter order).
    let interner = TypeInterner::new();

    // Create short string literals in a specific order (d, c, a)
    let lit_d = interner.literal_string("d");
    let lit_c = interner.literal_string("c");
    let lit_a = interner.literal_string("a");

    // Short strings should sort by content (alphabetical), matching tsc lib ordering
    let union_id = interner.union(vec![lit_a, lit_c, lit_d]);

    if let Some(TypeData::Union(list_id)) = interner.lookup(union_id) {
        let members = interner.type_list(list_id);
        assert_eq!(members.len(), 3);
        // Content order: a, c, d (alphabetical for short strings)
        assert_eq!(
            members[0], lit_a,
            "First member should be 'a' (alphabetically first)"
        );
        assert_eq!(
            members[1], lit_c,
            "Second member should be 'c' (alphabetically second)"
        );
        assert_eq!(
            members[2], lit_d,
            "Third member should be 'd' (alphabetically third)"
        );
    } else {
        panic!("Expected Union type");
    }

    // Longer strings should preserve allocation order (source encounter order)
    let lit_foo = interner.literal_string("foo");
    let lit_bar = interner.literal_string("bar");

    let union_id2 = interner.union(vec![lit_bar, lit_foo]);

    if let Some(TypeData::Union(list_id)) = interner.lookup(union_id2) {
        let members = interner.type_list(list_id);
        assert_eq!(members.len(), 2);
        // Allocation order: foo was interned first, then bar
        assert_eq!(
            members[0], lit_foo,
            "First member should be 'foo' (interned first)"
        );
        assert_eq!(
            members[1], lit_bar,
            "Second member should be 'bar' (interned second)"
        );
    } else {
        panic!("Expected Union type");
    }
}

#[test]
fn test_union_order_independent_of_input_order() {
    // Unions constructed with different input orders should normalize
    // to the same allocation-order-based result.
    let interner = TypeInterner::new();

    // Intern in order: x, y, z
    let x = interner.literal_string("x");
    let y = interner.literal_string("y");
    let z = interner.literal_string("z");

    let union1 = interner.union(vec![z, x, y]);
    let union2 = interner.union(vec![y, z, x]);
    let union3 = interner.union(vec![x, y, z]);

    assert_eq!(union1, union2, "Union should be order-independent");
    assert_eq!(union2, union3, "Union should be order-independent");
}

#[test]
fn test_estimated_size_bytes_is_nonzero_for_fresh_interner() {
    let interner = TypeInterner::new();
    let size = interner.estimated_size_bytes();
    assert!(
        size > 0,
        "estimated_size_bytes should be nonzero even for a fresh interner (struct overhead + intrinsics)"
    );
    // Must be at least the struct size itself
    assert!(
        size >= std::mem::size_of::<TypeInterner>(),
        "estimate ({size}) should be >= struct size ({})",
        std::mem::size_of::<TypeInterner>()
    );
}

#[test]
fn test_estimated_size_bytes_grows_with_interned_types() {
    let interner = TypeInterner::new();
    let baseline = interner.estimated_size_bytes();

    // Intern a bunch of types
    for i in 0..100 {
        interner.literal_string(&format!("prop_{i}"));
    }

    let after_types = interner.estimated_size_bytes();
    assert!(
        after_types > baseline,
        "Size should grow after interning types: baseline={baseline}, after={after_types}"
    );
}

#[test]
fn test_estimated_size_bytes_grows_with_object_shapes() {
    let interner = TypeInterner::new();
    let baseline = interner.estimated_size_bytes();

    // Intern object shapes (heavier than primitives)
    for i in 0..20 {
        let prop_name = interner.string_interner.intern(&format!("field_{i}"));
        let prop = PropertyInfo {
            name: prop_name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            visibility: Visibility::Public,
            is_method: false,
            is_class_prototype: false,
            parent_id: None,
            declaration_order: i as u32,
            is_string_named: false,
            is_symbol_named: false,
            single_quoted_name: false,
        };
        interner.object(vec![prop]);
    }

    let after_objects = interner.estimated_size_bytes();
    assert!(
        after_objects > baseline,
        "Size should grow after interning objects: baseline={baseline}, after={after_objects}"
    );
}

#[test]
fn test_estimated_size_bytes_grows_with_functions() {
    let interner = TypeInterner::new();
    let baseline = interner.estimated_size_bytes();

    // Intern function shapes
    for i in 0..20 {
        interner.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.string_interner.intern(&format!("p_{i}"))),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });
    }

    let after_fns = interner.estimated_size_bytes();
    assert!(
        after_fns > baseline,
        "Size should grow after interning functions: baseline={baseline}, after={after_fns}"
    );
}

/// TS2590: Intersection of many unions should trigger the `union_too_complex` flag.
///
/// When `normalize_intersection` receives an all-union intersection like
/// `(A|B) & (C|D) & ... & (Y|Z)` with a cross-product ≥ 100,000,
/// the flag must be set even though distribution is skipped.
#[test]
fn test_intersection_of_many_unions_sets_too_complex_flag() {
    let interner = TypeInterner::new();

    // Create 18 unions, each with 2 members → cross-product = 2^18 = 262,144 > 100,000
    let mut union_members = Vec::new();
    for i in 0..18u32 {
        let a_name = interner.intern_string(&format!("a{i}"));
        let b_name = interner.intern_string(&format!("b{i}"));
        let obj_a = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);
        let obj_b = interner.object(vec![PropertyInfo::new(b_name, TypeId::NUMBER)]);
        let u = interner.union(vec![obj_a, obj_b]);
        union_members.push(u);
    }

    // Clear any flag that might have been set during union construction
    let _ = interner.take_union_too_complex();

    // Create the intersection of 18 unions
    let _result = interner.intersection(union_members);

    // The flag should be set because 2^18 = 262,144 > 100,000
    assert!(
        interner.take_union_too_complex(),
        "Intersection of 18 two-member unions should set union_too_complex flag (cross-product = 2^18 = 262,144)"
    );
}

#[test]
fn test_interner_intersection_literal_subsumed_by_primitive() {
    // string & "hello" should reduce to "hello" because "hello" <: string
    let interner = TypeInterner::new();

    let hello = interner.literal_string("hello");
    let intersection = interner.intersection(vec![TypeId::STRING, hello]);

    // The intersection should reduce to just the literal
    assert_eq!(
        intersection, hello,
        "string & 'hello' should reduce to 'hello'"
    );
}

#[test]
fn test_interner_intersection_with_union_distributes_and_reduces() {
    // string & ("hello" | number) should distribute and reduce to "hello"
    let interner = TypeInterner::new();

    let hello = interner.literal_string("hello");
    let hello_or_number = interner.union(vec![hello, TypeId::NUMBER]);
    let intersection = interner.intersection(vec![TypeId::STRING, hello_or_number]);

    // After distribution: (string & "hello") | (string & number) = "hello" | never = "hello"
    assert_eq!(
        intersection, hello,
        "string & ('hello' | number) should distribute and reduce to 'hello'"
    );
}

#[test]
fn test_interner_intersection_three_unions_divergent_accessors() {
    // This is the exact scenario from divergentAccessorsTypes8:
    // (string | number) & ("hello" | number) & ("hello" | boolean) should reduce to "hello"
    let interner = TypeInterner::new();

    let hello = interner.literal_string("hello");

    // Three's prop1 setter type: string | number
    let union1 = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    // Four's prop1 setter type: "hello" | number
    let union2 = interner.union(vec![hello, TypeId::NUMBER]);
    // Five's prop1 setter type: "hello" | boolean
    let union3 = interner.union(vec![hello, TypeId::BOOLEAN]);

    let intersection = interner.intersection(vec![union1, union2, union3]);

    // After distribution and reduction:
    // The only surviving member is "hello" since:
    // - string from union1 can only match "hello" from union2/3
    // - number from union1/2 doesn't match boolean from union3
    assert_eq!(
        intersection, hello,
        "(string|number) & (\"hello\"|number) & (\"hello\"|boolean) should reduce to \"hello\""
    );
}

#[test]
fn test_string_intrinsic_same_kind_collapsed() {
    let interner = TypeInterner::new();

    // Create Uppercase<string>
    let upper_string = interner.string_intrinsic(StringIntrinsicKind::Uppercase, TypeId::STRING);

    // Create Uppercase<Uppercase<string>> - should collapse to Uppercase<string>
    let upper_upper_string =
        interner.string_intrinsic(StringIntrinsicKind::Uppercase, upper_string);

    assert_eq!(
        upper_upper_string, upper_string,
        "Uppercase<Uppercase<string>> should collapse to Uppercase<string>"
    );

    // Different kinds should NOT collapse
    let lower_upper_string =
        interner.string_intrinsic(StringIntrinsicKind::Lowercase, upper_string);
    assert_ne!(
        lower_upper_string, upper_string,
        "Lowercase<Uppercase<string>> should NOT collapse"
    );
}
