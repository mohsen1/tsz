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
            non_widening: false,
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
