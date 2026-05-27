#[test]
fn test_satisfies_literal_widening_preserved_string() {
    use crate::LiteralValue;
    use crate::relations::subtype::SubtypeChecker;

    // With satisfies, literal types are preserved:
    // const x = "hello" satisfies string -> type is "hello"
    // With type annotation:
    // const x: string = "hello" -> type is string (widened)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let hello = interner.literal_string("hello");

    // satisfies: literal type is preserved
    assert!(checker.is_subtype_of(hello, TypeId::STRING));
    // The type is still the literal, not widened
    match interner.lookup(hello) {
        Some(TypeData::Literal(LiteralValue::String(_))) => {} // Expected - literal preserved
        other => panic!("Expected Literal(String), got {other:?}"),
    }
}

#[test]
fn test_satisfies_literal_widening_preserved_number() {
    use crate::LiteralValue;
    use crate::relations::subtype::SubtypeChecker;

    // const x = 42 satisfies number -> type remains 42 (literal)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let forty_two = interner.literal_number(42.0);

    assert!(checker.is_subtype_of(forty_two, TypeId::NUMBER));
    match interner.lookup(forty_two) {
        Some(TypeData::Literal(LiteralValue::Number(_))) => {} // Expected - literal preserved
        other => panic!("Expected Literal(Number), got {other:?}"),
    }
}

#[test]
fn test_satisfies_literal_widening_preserved_boolean() {
    use crate::LiteralValue;
    use crate::relations::subtype::SubtypeChecker;

    // const x = true satisfies boolean -> type remains true (literal)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let lit_true = interner.literal_boolean(true);

    assert!(checker.is_subtype_of(lit_true, TypeId::BOOLEAN));
    match interner.lookup(lit_true) {
        Some(TypeData::Literal(LiteralValue::Boolean(true))) => {} // Expected - literal preserved
        other => panic!("Expected Literal(Boolean(true)), got {other:?}"),
    }
}

#[test]
fn test_satisfies_excess_property_check_fails() {
    use crate::relations::subtype::SubtypeChecker;

    // In TypeScript, satisfies performs excess property checking:
    // const x = { a: 1, b: 2, c: 3 } satisfies { a: number, b: number }
    // This is a compile error because 'c' is not in the constraint
    //
    // However, in structural subtyping, extra properties are allowed
    // (an object with more props is a subtype of one with fewer)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let source = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("c"), TypeId::NUMBER),
    ]);

    let target = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
    ]);

    // Structurally, {a, b, c} is a subtype of {a, b}
    // Note: Excess property checking is a separate, expression-level check
    assert!(checker.is_subtype_of(source, target));
}

#[test]
fn test_satisfies_missing_property_fails() {
    use crate::relations::subtype::SubtypeChecker;

    // const x = { a: 1 } satisfies { a: number, b: number }
    // This fails because 'b' is required but missing
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let source = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::NUMBER,
    )]);

    let target = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
    ]);

    // Missing required property 'b' - should fail
    assert!(!checker.is_subtype_of(source, target));
}

#[test]
fn test_satisfies_optional_property_satisfied() {
    use crate::relations::subtype::SubtypeChecker;

    // const x = { a: 1 } satisfies { a: number, b?: number }
    // This succeeds because 'b' is optional
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let source = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::NUMBER,
    )]);

    let target = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::NUMBER),
        PropertyInfo {
            name: interner.intern_string("b"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: true, // optional property
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
            is_symbol_named: false,
            single_quoted_name: false,
        },
    ]);

    // Missing optional property is ok
    assert!(checker.is_subtype_of(source, target));
}

#[test]
fn test_satisfies_vs_annotation_literal_preservation() {
    use crate::relations::subtype::SubtypeChecker;

    // Demonstrating satisfies vs type annotation difference:
    //
    // Type annotation widens:
    //   const x: string = "hello"  // x has type 'string'
    //
    // Satisfies preserves:
    //   const x = "hello" satisfies string  // x has type '"hello"'
    //
    // Both are valid (literal is subtype of base), but the resulting type differs
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let hello = interner.literal_string("hello");

    // With satisfies: type stays as "hello"
    let satisfies_type = hello;

    // With annotation: type would be widened to string
    let annotation_type = TypeId::STRING;

    // Both satisfy the string constraint
    assert!(checker.is_subtype_of(satisfies_type, TypeId::STRING));
    assert!(checker.is_subtype_of(annotation_type, TypeId::STRING));

    // But satisfies preserves more specific type
    // "hello" is a subtype of string, but not vice versa
    assert!(checker.is_subtype_of(satisfies_type, annotation_type));
    assert!(!checker.is_subtype_of(annotation_type, satisfies_type));
}

#[test]
fn test_satisfies_vs_annotation_object_properties() {
    use crate::relations::subtype::SubtypeChecker;

    // With satisfies, object property types are preserved:
    //   const x = { status: "success" } satisfies { status: string }
    //   x.status is "success" (can be used in narrowing)
    //
    // With annotation, property types are widened:
    //   const x: { status: string } = { status: "success" }
    //   x.status is string
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let success = interner.literal_string("success");

    // Satisfies result: property type is literal
    let satisfies_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("status"),
        success,
    )]);

    // Annotation result: property type is widened
    let annotation_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("status"),
        TypeId::STRING,
    )]);

    // Both satisfy the constraint
    assert!(checker.is_subtype_of(satisfies_obj, annotation_obj));

    // But satisfies result is more specific
    assert!(!checker.is_subtype_of(annotation_obj, satisfies_obj));
}

#[test]
fn test_satisfies_union_constraint() {
    use crate::relations::subtype::SubtypeChecker;

    // const x = "a" satisfies "a" | "b" | "c"
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let lit_c = interner.literal_string("c");

    let union = interner.union(vec![lit_a, lit_b, lit_c]);

    // "a" satisfies the union
    assert!(checker.is_subtype_of(lit_a, union));
    // But the type remains "a", not the union
    assert_ne!(lit_a, union);
}

#[test]
fn test_satisfies_array_type() {
    use crate::relations::subtype::SubtypeChecker;

    // const x = [1, 2, 3] satisfies number[]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // Tuple with literal types
    let one = interner.literal_number(1.0);
    let two = interner.literal_number(2.0);
    let three = interner.literal_number(3.0);

    let tuple = interner.tuple(vec![
        TupleElement {
            type_id: one,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: two,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: three,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let number_array = interner.array(TypeId::NUMBER);

    // Tuple [1, 2, 3] satisfies number[]
    assert!(checker.is_subtype_of(tuple, number_array));
}

#[test]
fn test_satisfies_record_type() {
    use crate::relations::subtype::SubtypeChecker;

    // const x = { foo: 1, bar: 2 } satisfies Record<string, number>
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let source = interner.object(vec![
        PropertyInfo::new(interner.intern_string("bar"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("foo"), TypeId::NUMBER),
    ]);

    // Record<string, number> is an object with string index signature
    let record = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    // Object with named properties satisfies Record<string, number>
    assert!(checker.is_subtype_of(source, record));
}

#[test]
fn test_satisfies_with_generic_function() {
    use crate::relations::subtype::SubtypeChecker;

    // const fn = <T>(x: T) => x satisfies <T>(x: T) => T
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let t_name = interner.intern_string("T");
    let x_name = interner.intern_string("x");

    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let source_func = interner.function(FunctionShape {
        type_params: vec![TypeParamInfo {
            name: t_name,
            constraint: None,
            default: None,
            is_const: false,
        }],
        params: vec![ParamInfo::required(x_name, t_param)],
        this_type: None,
        return_type: t_param,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let target_func = interner.function(FunctionShape {
        type_params: vec![TypeParamInfo {
            name: t_name,
            constraint: None,
            default: None,
            is_const: false,
        }],
        params: vec![ParamInfo::required(x_name, t_param)],
        this_type: None,
        return_type: t_param,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Function types should satisfy each other (structural match)
    assert!(checker.is_subtype_of(source_func, target_func));
}

#[test]
fn test_satisfies_preserves_narrower_type() {
    use crate::relations::subtype::SubtypeChecker;
    use crate::types::LiteralValue;

    // const x = "hello" satisfies string
    // Type of x should remain "hello", not widen to string
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let hello_lit = interner.literal_string("hello");

    // satisfies check passes
    assert!(checker.is_subtype_of(hello_lit, TypeId::STRING));

    // But the type itself remains the literal
    match interner.lookup(hello_lit) {
        Some(TypeData::Literal(LiteralValue::String(_))) => {} // Expected
        other => panic!("Expected Literal(String), got {other:?}"),
    }
}

#[test]
fn test_satisfies_with_union_literals() {
    use crate::relations::subtype::SubtypeChecker;

    // const x = "a" | "b" satisfies string
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_lit = interner.literal_string("a");
    let b_lit = interner.literal_string("b");
    let union = interner.union(vec![a_lit, b_lit]);

    // Union of string literals satisfies string
    assert!(checker.is_subtype_of(union, TypeId::STRING));
}

#[test]
fn test_satisfies_with_intersection() {
    use crate::relations::subtype::SubtypeChecker;

    // const x = { a: 1 } & { b: 2 } satisfies { a: number, b: number }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let one = interner.literal_number(1.0);
    let two = interner.literal_number(2.0);

    let obj_a = interner.object(vec![PropertyInfo::new(interner.intern_string("a"), one)]);
    let obj_b = interner.object(vec![PropertyInfo::new(interner.intern_string("b"), two)]);
    let intersection = interner.intersection(vec![obj_a, obj_b]);

    let target = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
    ]);

    // Intersection satisfies target (has both properties)
    assert!(checker.is_subtype_of(intersection, target));
}

#[test]
fn test_noinfer_blocks_inference_in_target() {
    // function foo<T>(x: NoInfer<T>): T
    // When NoInfer<T> is target, inference should be blocked
    use crate::inference::infer::InferenceContext;
    use crate::types::InferencePriority;

    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let hello_lit = interner.literal_string("hello");
    let noinfer_t = interner.intern(TypeData::NoInfer(t_param));

    // Source is "hello", target is NoInfer<T>
    // Should block inference (return Ok(()) without adding candidates)
    ctx.infer_from_types(hello_lit, noinfer_t, InferencePriority::NakedTypeVariable)
        .unwrap();

    // T should remain unresolved (no candidates added)
    assert!(ctx.probe(var_t).is_none());
}

#[test]
fn test_noinfer_in_union_distribution() {
    // NoInfer<string | number> should not distribute in conditionals
    use crate::evaluation::evaluate::evaluate_type;

    let interner = TypeInterner::new();

    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let noinfer_union = interner.intern(TypeData::NoInfer(union));

    // Evaluate should strip NoInfer but preserve union structure
    let evaluated = evaluate_type(&interner, noinfer_union);
    match interner.lookup(evaluated) {
        Some(TypeData::Union(_)) => {} // Union preserved
        other => panic!("Expected Union, got {other:?}"),
    }
}

#[test]
fn test_noinfer_with_array_elements() {
    // NoInfer<T[]> - should evaluate to T[] but block inference from array elements
    use crate::evaluation::evaluate::evaluate_type;

    let interner = TypeInterner::new();

    let string_array = interner.array(TypeId::STRING);
    let noinfer_array = interner.intern(TypeData::NoInfer(string_array));

    let evaluated = evaluate_type(&interner, noinfer_array);
    match interner.lookup(evaluated) {
        Some(TypeData::Array(elem)) => {
            assert_eq!(elem, TypeId::STRING);
        }
        other => panic!("Expected Array, got {other:?}"),
    }
}

#[test]
fn test_noinfer_visitor_traversal() {
    // NoInfer should be traversed by visitors
    use crate::visitor::TypeVisitor;

    struct TestVisitor {
        visited_noinfer: bool,
    }

    impl TypeVisitor for TestVisitor {
        type Output = ();

        fn visit_no_infer(&mut self, _inner: TypeId) -> Self::Output {
            self.visited_noinfer = true;
        }

        fn visit_intrinsic(&mut self, _kind: IntrinsicKind) -> Self::Output {}
        fn visit_literal(&mut self, _value: &LiteralValue) -> Self::Output {}
        fn default_output() -> Self::Output {}
    }

    let interner = TypeInterner::new();
    let noinfer_string = interner.intern(TypeData::NoInfer(TypeId::STRING));

    let mut visitor = TestVisitor {
        visited_noinfer: false,
    };
    visitor.visit_type(&interner, noinfer_string);

    assert!(visitor.visited_noinfer, "NoInfer should be visited");
}

#[test]
fn test_noinfer_contains_type_param() {
    // NoInfer<T> should contain T for type parameter collection
    // This is tested indirectly through visitor traversal
    use crate::visitor::TypeVisitor;
    use tsz_common::interner::Atom;

    struct CollectParams<'a> {
        params: Vec<Atom>,
        interner: &'a TypeInterner,
    }

    impl<'a> TypeVisitor for CollectParams<'a> {
        type Output = ();

        fn visit_type_parameter(&mut self, info: &TypeParamInfo) -> Self::Output {
            self.params.push(info.name);
        }

        fn visit_no_infer(&mut self, inner: TypeId) -> Self::Output {
            // Should recurse into inner type
            self.visit_type(self.interner, inner)
        }

        fn visit_intrinsic(&mut self, _kind: IntrinsicKind) -> Self::Output {}
        fn visit_literal(&mut self, _value: &LiteralValue) -> Self::Output {}
        fn default_output() -> Self::Output {}
    }

    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");

    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let noinfer_t = interner.intern(TypeData::NoInfer(t_param));

    let mut collector = CollectParams {
        params: Vec::new(),
        interner: &interner,
    };
    collector.visit_type(&interner, noinfer_t);

    // Should detect that NoInfer<T> contains T
    assert!(collector.params.contains(&t_name));
}

// ============================================================================
// Intrinsic Type Tests - BigInt, Symbol, Number/String Literals
// ============================================================================

/// `BigInt` literal type creation and comparison
#[test]
fn test_bigint_literal_creation() {
    let interner = TypeInterner::new();

    let bigint_42 = interner.literal_bigint("42");
    let bigint_42_dup = interner.literal_bigint("42");

    // Same bigint literal should produce same TypeId
    assert_eq!(bigint_42, bigint_42_dup);

    // Different bigint literals should produce different TypeIds
    let bigint_100 = interner.literal_bigint("100");
    assert_ne!(bigint_42, bigint_100);
}

/// `BigInt` literal extends bigint base type
#[test]
fn test_bigint_literal_extends_bigint() {
    let interner = TypeInterner::new();

    let bigint_42 = interner.literal_bigint("42");

    // 42n extends bigint ? true : false
    let cond = ConditionalType {
        check_type: bigint_42,
        extends_type: TypeId::BIGINT,
        true_type: interner.literal_boolean(true),
        false_type: interner.literal_boolean(false),
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    assert_eq!(result, interner.literal_boolean(true));
}

/// `BigInt` doesn't extend number
#[test]
fn test_bigint_not_extends_number() {
    let interner = TypeInterner::new();

    let bigint_42 = interner.literal_bigint("42");

    // 42n extends number ? true : false
    let cond = ConditionalType {
        check_type: bigint_42,
        extends_type: TypeId::NUMBER,
        true_type: interner.literal_boolean(true),
        false_type: interner.literal_boolean(false),
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    assert_eq!(result, interner.literal_boolean(false));
}

/// `BigInt` literal union
#[test]
fn test_bigint_literal_union() {
    let interner = TypeInterner::new();

    let bigint_1 = interner.literal_bigint("1");
    let bigint_2 = interner.literal_bigint("2");
    let bigint_3 = interner.literal_bigint("3");

    let union = interner.union(vec![bigint_1, bigint_2, bigint_3]);

    match interner.lookup(union) {
        Some(TypeData::Union(list_id)) => {
            let members = interner.type_list(list_id);
            assert_eq!(members.len(), 3);
        }
        _ => panic!("Expected union"),
    }
}

/// `BigInt` with negative value
#[test]
fn test_bigint_negative_literal() {
    let interner = TypeInterner::new();

    let neg_bigint = interner.literal_bigint_with_sign(true, "42");
    let pos_bigint = interner.literal_bigint("42");

    // Negative and positive should be different
    assert_ne!(neg_bigint, pos_bigint);

    // Negative bigint extends bigint
    let cond = ConditionalType {
        check_type: neg_bigint,
        extends_type: TypeId::BIGINT,
        true_type: interner.literal_boolean(true),
        false_type: interner.literal_boolean(false),
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    assert_eq!(result, interner.literal_boolean(true));
}

/// Symbol type doesn't extend string
#[test]
fn test_symbol_not_extends_string() {
    let interner = TypeInterner::new();

    // symbol extends string ? true : false
    let cond = ConditionalType {
        check_type: TypeId::SYMBOL,
        extends_type: TypeId::STRING,
        true_type: interner.literal_boolean(true),
        false_type: interner.literal_boolean(false),
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    assert_eq!(result, interner.literal_boolean(false));
}

/// Symbol extends symbol
#[test]
fn test_symbol_extends_symbol() {
    let interner = TypeInterner::new();

    // symbol extends symbol ? true : false
    let cond = ConditionalType {
        check_type: TypeId::SYMBOL,
        extends_type: TypeId::SYMBOL,
        true_type: interner.literal_boolean(true),
        false_type: interner.literal_boolean(false),
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    assert_eq!(result, interner.literal_boolean(true));
}

/// Unique symbol extends base symbol
#[test]
fn test_unique_symbol_extends_symbol() {
    let interner = TypeInterner::new();

    let unique_sym = interner.intern(TypeData::UniqueSymbol(SymbolRef(42)));

    // unique symbol extends symbol ? true : false
    let cond = ConditionalType {
        check_type: unique_sym,
        extends_type: TypeId::SYMBOL,
        true_type: interner.literal_boolean(true),
        false_type: interner.literal_boolean(false),
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    assert_eq!(result, interner.literal_boolean(true));
}

/// Unique symbols with different refs are distinct
#[test]
fn test_unique_symbol_distinct_refs() {
    let interner = TypeInterner::new();

    let sym_a = interner.intern(TypeData::UniqueSymbol(SymbolRef(1)));
    let sym_b = interner.intern(TypeData::UniqueSymbol(SymbolRef(2)));

    // Different refs produce different types
    assert_ne!(sym_a, sym_b);

    // Same ref produces same type
    let sym_a_dup = interner.intern(TypeData::UniqueSymbol(SymbolRef(1)));
    assert_eq!(sym_a, sym_a_dup);
}

/// Unique symbol in union with base symbol
#[test]
fn test_unique_symbol_union_with_symbol() {
    let interner = TypeInterner::new();

    let unique_sym = interner.intern(TypeData::UniqueSymbol(SymbolRef(1)));
    let union = interner.union(vec![unique_sym, TypeId::SYMBOL]);

    match interner.lookup(union) {
        Some(TypeData::Union(list_id)) => {
            let members = interner.type_list(list_id);
            // Union should have 2 members (unique symbol and symbol)
            assert_eq!(members.len(), 2);
        }
        _ => panic!("Expected union"),
    }
}

/// Unique symbol as union member
#[test]
fn test_unique_symbol_in_union() {
    let interner = TypeInterner::new();

    let sym1 = interner.intern(TypeData::UniqueSymbol(SymbolRef(100)));
    let sym2 = interner.intern(TypeData::UniqueSymbol(SymbolRef(101)));
    let sym3 = interner.intern(TypeData::UniqueSymbol(SymbolRef(102)));

    // Create union of unique symbols
    let union = interner.union(vec![sym1, sym2, sym3]);

    match interner.lookup(union) {
        Some(TypeData::Union(list_id)) => {
            let members = interner.type_list(list_id);
            assert_eq!(members.len(), 3);
        }
        _ => panic!("Expected union"),
    }
}

/// Number literal type creation and comparison
#[test]
fn test_number_literal_creation() {
    let interner = TypeInterner::new();

    let num_42 = interner.literal_number(42.0);
    let num_42_dup = interner.literal_number(42.0);

    // Same number literal should produce same TypeId
    assert_eq!(num_42, num_42_dup);

    // Different number literals should produce different TypeIds
    let num_100 = interner.literal_number(100.0);
    assert_ne!(num_42, num_100);
}

/// Number literal extends number
#[test]
fn test_number_literal_extends_number() {
    let interner = TypeInterner::new();

    let num_42 = interner.literal_number(42.0);

    // 42 extends number ? true : false
    let cond = ConditionalType {
        check_type: num_42,
        extends_type: TypeId::NUMBER,
        true_type: interner.literal_boolean(true),
        false_type: interner.literal_boolean(false),
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    assert_eq!(result, interner.literal_boolean(true));
}

/// Number literal doesn't extend different literal
#[test]
fn test_number_literal_not_extends_different() {
    let interner = TypeInterner::new();

    let num_42 = interner.literal_number(42.0);
    let num_100 = interner.literal_number(100.0);

    // 42 extends 100 ? true : false
    let cond = ConditionalType {
        check_type: num_42,
        extends_type: num_100,
        true_type: interner.literal_boolean(true),
        false_type: interner.literal_boolean(false),
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    assert_eq!(result, interner.literal_boolean(false));
}

/// Number literal union
#[test]
fn test_number_literal_union() {
    let interner = TypeInterner::new();

    let num_1 = interner.literal_number(1.0);
    let num_2 = interner.literal_number(2.0);
    let num_3 = interner.literal_number(3.0);

    let union = interner.union(vec![num_1, num_2, num_3]);

    match interner.lookup(union) {
        Some(TypeData::Union(list_id)) => {
            let members = interner.type_list(list_id);
            assert_eq!(members.len(), 3);
        }
        _ => panic!("Expected union"),
    }
}

/// String literal type comparison
#[test]
fn test_string_literal_comparison() {
    let interner = TypeInterner::new();

    let str_hello = interner.literal_string("hello");
    let str_hello_dup = interner.literal_string("hello");

    // Same string literal should produce same TypeId
    assert_eq!(str_hello, str_hello_dup);

    // Different string literals should produce different TypeIds
    let str_world = interner.literal_string("world");
    assert_ne!(str_hello, str_world);
}

/// String literal union narrowing via conditional
#[test]
fn test_string_literal_union_conditional() {
    let interner = TypeInterner::new();

    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let lit_c = interner.literal_string("c");

    let union_ab = interner.union(vec![lit_a, lit_b]);

    // "a" extends "a" | "b" ? true : false
    let cond_a = ConditionalType {
        check_type: lit_a,
        extends_type: union_ab,
        true_type: interner.literal_boolean(true),
        false_type: interner.literal_boolean(false),
        is_distributive: false,
    };
    let result_a = evaluate_conditional(&interner, &cond_a);
    assert_eq!(result_a, interner.literal_boolean(true));

    // "c" extends "a" | "b" ? true : false
    let cond_c = ConditionalType {
        check_type: lit_c,
        extends_type: union_ab,
        true_type: interner.literal_boolean(true),
        false_type: interner.literal_boolean(false),
        is_distributive: false,
    };
    let result_c = evaluate_conditional(&interner, &cond_c);
    assert_eq!(result_c, interner.literal_boolean(false));
}

/// Mixed numeric literal types (number and bigint)
#[test]
fn test_mixed_numeric_literal_types() {
    let interner = TypeInterner::new();

    let num_42 = interner.literal_number(42.0);
    let bigint_42 = interner.literal_bigint("42");

    // Number and bigint with same value are different types
    assert_ne!(num_42, bigint_42);

    // Create union of number and bigint literals
    let union = interner.union(vec![num_42, bigint_42]);
    match interner.lookup(union) {
        Some(TypeData::Union(list_id)) => {
            let members = interner.type_list(list_id);
            assert_eq!(members.len(), 2);
        }
        _ => panic!("Expected union"),
    }
}

/// Floating point number literals
#[test]
fn test_float_number_literal() {
    let interner = TypeInterner::new();

    const APPROX_PI: f64 = 3.15;
    const APPROX_E: f64 = 2.72;

    let float_pi = interner.literal_number(APPROX_PI);
    let float_e = interner.literal_number(APPROX_E);

    assert_ne!(float_pi, float_e);

    // Float extends number
    let cond = ConditionalType {
        check_type: float_pi,
        extends_type: TypeId::NUMBER,
        true_type: interner.literal_boolean(true),
        false_type: interner.literal_boolean(false),
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    assert_eq!(result, interner.literal_boolean(true));
}

/// Negative number literals
#[test]
fn test_negative_number_literal() {
    let interner = TypeInterner::new();

    let neg_42 = interner.literal_number(-42.0);
    let pos_42 = interner.literal_number(42.0);

    // Negative and positive are different
    assert_ne!(neg_42, pos_42);

    // Negative number extends number
    let cond = ConditionalType {
        check_type: neg_42,
        extends_type: TypeId::NUMBER,
        true_type: interner.literal_boolean(true),
        false_type: interner.literal_boolean(false),
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    assert_eq!(result, interner.literal_boolean(true));
}

/// Zero and negative zero number literals
#[test]
fn test_zero_number_literal() {
    let interner = TypeInterner::new();

    let zero = interner.literal_number(0.0);
    let neg_zero = interner.literal_number(-0.0);

    // In IEEE 754, 0.0 and -0.0 are equal for comparison purposes
    // but may or may not intern to the same TypeId depending on implementation
    // The key test is that both extend number
    let cond_zero = ConditionalType {
        check_type: zero,
        extends_type: TypeId::NUMBER,
        true_type: interner.literal_boolean(true),
        false_type: interner.literal_boolean(false),
        is_distributive: false,
    };
    assert_eq!(
        evaluate_conditional(&interner, &cond_zero),
        interner.literal_boolean(true)
    );

    let cond_neg = ConditionalType {
        check_type: neg_zero,
        extends_type: TypeId::NUMBER,
        true_type: interner.literal_boolean(true),
        false_type: interner.literal_boolean(false),
        is_distributive: false,
    };
    assert_eq!(
        evaluate_conditional(&interner, &cond_neg),
        interner.literal_boolean(true)
    );
}

/// Boolean literal type operations
#[test]
fn test_boolean_literal_operations() {
    let interner = TypeInterner::new();

    let lit_true = interner.literal_boolean(true);
    let lit_false = interner.literal_boolean(false);

    // true and false are different
    assert_ne!(lit_true, lit_false);

    // Both extend boolean
    let cond_true = ConditionalType {
        check_type: lit_true,
        extends_type: TypeId::BOOLEAN,
        true_type: interner.literal_boolean(true),
        false_type: interner.literal_boolean(false),
        is_distributive: false,
    };
    assert_eq!(evaluate_conditional(&interner, &cond_true), lit_true);

    let cond_false = ConditionalType {
        check_type: lit_false,
        extends_type: TypeId::BOOLEAN,
        true_type: interner.literal_boolean(true),
        false_type: interner.literal_boolean(false),
        is_distributive: false,
    };
    assert_eq!(evaluate_conditional(&interner, &cond_false), lit_true);
}

/// Boolean literal union equals boolean
#[test]
fn test_boolean_literal_union() {
    let interner = TypeInterner::new();

    let lit_true = interner.literal_boolean(true);
    let lit_false = interner.literal_boolean(false);

    // true | false should simplify to boolean (or remain as union)
    let union = interner.union(vec![lit_true, lit_false]);

    // Either it's BOOLEAN or a union of two
    match interner.lookup(union) {
        Some(TypeData::Union(list_id)) => {
            let members = interner.type_list(list_id);
            assert!(members.len() == 2);
        }
        Some(TypeData::Intrinsic(IntrinsicKind::Boolean)) => {
            // Simplified to boolean
        }
        _ => panic!("Expected union or boolean"),
    }
}

/// Intrinsic types are not equal to each other
#[test]
fn test_intrinsic_types_distinct() {
    // Verify all intrinsic types are distinct
    assert_ne!(TypeId::STRING, TypeId::NUMBER);
    assert_ne!(TypeId::NUMBER, TypeId::BOOLEAN);
    assert_ne!(TypeId::BOOLEAN, TypeId::BIGINT);
    assert_ne!(TypeId::BIGINT, TypeId::SYMBOL);
    assert_ne!(TypeId::SYMBOL, TypeId::NULL);
    assert_ne!(TypeId::NULL, TypeId::UNDEFINED);
    assert_ne!(TypeId::UNDEFINED, TypeId::VOID);
    assert_ne!(TypeId::VOID, TypeId::NEVER);
    assert_ne!(TypeId::NEVER, TypeId::ANY);
    assert_ne!(TypeId::ANY, TypeId::UNKNOWN);
    assert_ne!(TypeId::UNKNOWN, TypeId::OBJECT);
}

/// Null and undefined handling
#[test]
fn test_null_undefined_extends() {
    let interner = TypeInterner::new();

    // null extends null
    let cond_null = ConditionalType {
        check_type: TypeId::NULL,
        extends_type: TypeId::NULL,
        true_type: interner.literal_boolean(true),
        false_type: interner.literal_boolean(false),
        is_distributive: false,
    };
    assert_eq!(
        evaluate_conditional(&interner, &cond_null),
        interner.literal_boolean(true)
    );

    // undefined extends undefined
    let cond_undef = ConditionalType {
        check_type: TypeId::UNDEFINED,
        extends_type: TypeId::UNDEFINED,
        true_type: interner.literal_boolean(true),
        false_type: interner.literal_boolean(false),
        is_distributive: false,
    };
    assert_eq!(
        evaluate_conditional(&interner, &cond_undef),
        interner.literal_boolean(true)
    );

    // null doesn't extend undefined
    let cond_null_undef = ConditionalType {
        check_type: TypeId::NULL,
        extends_type: TypeId::UNDEFINED,
        true_type: interner.literal_boolean(true),
        false_type: interner.literal_boolean(false),
        is_distributive: false,
    };
    assert_eq!(
        evaluate_conditional(&interner, &cond_null_undef),
        interner.literal_boolean(false)
    );
}

/// Void and undefined relationship
#[test]
fn test_void_undefined_relationship() {
    let interner = TypeInterner::new();

    // undefined extends void
    let cond = ConditionalType {
        check_type: TypeId::UNDEFINED,
        extends_type: TypeId::VOID,
        true_type: interner.literal_boolean(true),
        false_type: interner.literal_boolean(false),
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    assert_eq!(result, interner.literal_boolean(true));
}

/// Never is bottom type
#[test]
fn test_never_bottom_type() {
    let interner = TypeInterner::new();

    // never extends any type
    let cond_string = ConditionalType {
        check_type: TypeId::NEVER,
        extends_type: TypeId::STRING,
        true_type: interner.literal_boolean(true),
        false_type: interner.literal_boolean(false),
        is_distributive: false,
    };
    assert_eq!(
        evaluate_conditional(&interner, &cond_string),
        interner.literal_boolean(true)
    );

    let cond_number = ConditionalType {
        check_type: TypeId::NEVER,
        extends_type: TypeId::NUMBER,
        true_type: interner.literal_boolean(true),
        false_type: interner.literal_boolean(false),
        is_distributive: false,
    };
    assert_eq!(
        evaluate_conditional(&interner, &cond_number),
        interner.literal_boolean(true)
    );
}

/// Any and unknown are top types
#[test]
fn test_any_unknown_top_types() {
    let interner = TypeInterner::new();

    // string extends any
    let cond_any = ConditionalType {
        check_type: TypeId::STRING,
        extends_type: TypeId::ANY,
        true_type: interner.literal_boolean(true),
        false_type: interner.literal_boolean(false),
        is_distributive: false,
    };
    assert_eq!(
        evaluate_conditional(&interner, &cond_any),
        interner.literal_boolean(true)
    );

    // string extends unknown
    let cond_unknown = ConditionalType {
        check_type: TypeId::STRING,
        extends_type: TypeId::UNKNOWN,
        true_type: interner.literal_boolean(true),
        false_type: interner.literal_boolean(false),
        is_distributive: false,
    };
    assert_eq!(
        evaluate_conditional(&interner, &cond_unknown),
        interner.literal_boolean(true)
    );
}

// ============================================================================
// const assertion (as const) tests
// The as const assertion creates readonly types with literal inference
// ============================================================================

#[test]
fn test_const_object_literal_readonly_properties() {
    // const x = { a: 1, b: "hello" } as const
    // -> { readonly a: 1, readonly b: "hello" }
    let interner = TypeInterner::new();

    let one = interner.literal_number(1.0);
    let hello = interner.literal_string("hello");

    // Object with readonly properties and literal types
    let const_obj = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("a"),
            type_id: one,
            write_type: one,
            optional: false,
            readonly: true, // as const makes properties readonly
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
            is_symbol_named: false,
            single_quoted_name: false,
        },
        PropertyInfo::readonly(interner.intern_string("b"), hello),
    ]);

    // Verify the object was created
    match interner.lookup(const_obj) {
        Some(TypeData::Object(shape_id)) => {
            let shape = interner.object_shape(shape_id);
            assert_eq!(shape.properties.len(), 2);
            // All properties should be readonly
            for prop in &shape.properties {
                assert!(prop.readonly);
            }
        }
        other => panic!("Expected Object type, got {other:?}"),
    }
}

#[test]
fn test_const_object_literal_nested() {
    // const x = { outer: { inner: 42 } } as const
    // -> { readonly outer: { readonly inner: 42 } }
    let interner = TypeInterner::new();

    let forty_two = interner.literal_number(42.0);

    // Inner object with readonly property
    let inner = interner.object(vec![PropertyInfo::readonly(
        interner.intern_string("inner"),
        forty_two,
    )]);

    // Outer object with readonly property pointing to inner
    let outer = interner.object(vec![PropertyInfo::readonly(
        interner.intern_string("outer"),
        inner,
    )]);

    match interner.lookup(outer) {
        Some(TypeData::Object(shape_id)) => {
            let shape = interner.object_shape(shape_id);
            assert_eq!(shape.properties.len(), 1);
            assert!(shape.properties[0].readonly);
            // The inner type should also be an object
            let inner_type = shape.properties[0].type_id;
            match interner.lookup(inner_type) {
                Some(TypeData::Object(inner_shape_id)) => {
                    let inner_shape = interner.object_shape(inner_shape_id);
                    assert!(inner_shape.properties[0].readonly);
                }
                other => panic!("Expected inner Object, got {other:?}"),
            }
        }
        other => panic!("Expected Object type, got {other:?}"),
    }
}

#[test]
fn test_const_object_literal_vs_mutable() {
    use crate::relations::subtype::SubtypeChecker;

    // const x = { a: 1 } as const  ->  { readonly a: 1 }
    // let y = { a: 1 }             ->  { a: number }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let one = interner.literal_number(1.0);

    // as const version (readonly, literal type)
    let const_obj = interner.object(vec![PropertyInfo::readonly(
        interner.intern_string("a"),
        one,
    )]);

    // Same object but with widened type (still readonly for comparison)
    let widened_readonly = interner.object(vec![PropertyInfo::readonly(
        interner.intern_string("a"),
        TypeId::NUMBER,
    )]);

    // Literal type is subtype of base type (when readonly matches)
    // { readonly a: 1 } is subtype of { readonly a: number }
    assert!(checker.is_subtype_of(const_obj, widened_readonly));

    // But not the other way around - number is not subtype of 1
    assert!(!checker.is_subtype_of(widened_readonly, const_obj));
}

#[test]
fn test_const_array_literal_tuple() {
    // const x = [1, 2, 3] as const
    // -> readonly [1, 2, 3]
    let interner = TypeInterner::new();

    let one = interner.literal_number(1.0);
    let two = interner.literal_number(2.0);
    let three = interner.literal_number(3.0);

    // Create tuple with literal elements
    let tuple = interner.tuple(vec![
        TupleElement {
            type_id: one,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: two,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: three,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    // Wrap in ReadonlyType for as const
    let readonly_tuple = interner.intern(TypeData::ReadonlyType(tuple));

    match interner.lookup(readonly_tuple) {
        Some(TypeData::ReadonlyType(inner)) => {
            assert_eq!(inner, tuple);
            // Verify inner is a tuple
            match interner.lookup(inner) {
                Some(TypeData::Tuple(list_id)) => {
                    let elements = interner.tuple_list(list_id);
                    assert_eq!(elements.len(), 3);
                }
                other => panic!("Expected Tuple, got {other:?}"),
            }
        }
        other => panic!("Expected ReadonlyType, got {other:?}"),
    }
}

#[test]
fn test_const_array_mixed_types() {
    // const x = [1, "two", true] as const
    // -> readonly [1, "two", true]
    let interner = TypeInterner::new();

    let one = interner.literal_number(1.0);
    let two_str = interner.literal_string("two");
    let lit_true = interner.literal_boolean(true);

    let tuple = interner.tuple(vec![
        TupleElement {
            type_id: one,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: two_str,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: lit_true,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let readonly_tuple = interner.intern(TypeData::ReadonlyType(tuple));

    match interner.lookup(readonly_tuple) {
        Some(TypeData::ReadonlyType(inner)) => match interner.lookup(inner) {
            Some(TypeData::Tuple(list_id)) => {
                let elements = interner.tuple_list(list_id);
                assert_eq!(elements.len(), 3);
                assert_eq!(elements[0].type_id, one);
                assert_eq!(elements[1].type_id, two_str);
                assert_eq!(elements[2].type_id, lit_true);
            }
            other => panic!("Expected Tuple, got {other:?}"),
        },
        other => panic!("Expected ReadonlyType, got {other:?}"),
    }
}

#[test]
fn test_const_array_nested() {
    // const x = [[1, 2], [3, 4]] as const
    // -> readonly [readonly [1, 2], readonly [3, 4]]
    let interner = TypeInterner::new();

    let one = interner.literal_number(1.0);
    let two = interner.literal_number(2.0);
    let three = interner.literal_number(3.0);
    let four = interner.literal_number(4.0);

    let inner1 = interner.tuple(vec![
        TupleElement {
            type_id: one,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: two,
            name: None,
            optional: false,
            rest: false,
        },
    ]);
    let inner1_readonly = interner.intern(TypeData::ReadonlyType(inner1));

    let inner2 = interner.tuple(vec![
        TupleElement {
            type_id: three,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: four,
            name: None,
            optional: false,
            rest: false,
        },
    ]);
    let inner2_readonly = interner.intern(TypeData::ReadonlyType(inner2));

    let outer = interner.tuple(vec![
        TupleElement {
            type_id: inner1_readonly,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: inner2_readonly,
            name: None,
            optional: false,
            rest: false,
        },
    ]);
    let outer_readonly = interner.intern(TypeData::ReadonlyType(outer));

    match interner.lookup(outer_readonly) {
        Some(TypeData::ReadonlyType(inner)) => {
            match interner.lookup(inner) {
                Some(TypeData::Tuple(list_id)) => {
                    let elements = interner.tuple_list(list_id);
                    assert_eq!(elements.len(), 2);
                    // Each element should be ReadonlyType
                    for elem in elements.iter() {
                        match interner.lookup(elem.type_id) {
                            Some(TypeData::ReadonlyType(_)) => {}
                            other => panic!("Expected nested ReadonlyType, got {other:?}"),
                        }
                    }
                }
                other => panic!("Expected Tuple, got {other:?}"),
            }
        }
        other => panic!("Expected ReadonlyType, got {other:?}"),
    }
}

#[test]
fn test_const_array_vs_mutable() {
    use crate::relations::subtype::SubtypeChecker;

    // const x = [1, 2] as const  ->  readonly [1, 2]
    // A non-readonly tuple [1, 2] is subtype of number[]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let one = interner.literal_number(1.0);
    let two = interner.literal_number(2.0);

    // Non-readonly tuple with literal types
    let mutable_tuple = interner.tuple(vec![
        TupleElement {
            type_id: one,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: two,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let number_array = interner.array(TypeId::NUMBER);

    // Tuple [1, 2] is subtype of number[]
    assert!(checker.is_subtype_of(mutable_tuple, number_array));

    // Readonly version
    let readonly_tuple = interner.intern(TypeData::ReadonlyType(mutable_tuple));
    let readonly_array = interner.intern(TypeData::ReadonlyType(number_array));

    // Readonly tuple is subtype of readonly number[]
    assert!(checker.is_subtype_of(readonly_tuple, readonly_array));
}

#[test]
fn test_readonly_type_wrapper() {
    // ReadonlyType wraps any type to make it readonly
    let interner = TypeInterner::new();

    let arr = interner.array(TypeId::STRING);
    let readonly_arr = interner.intern(TypeData::ReadonlyType(arr));

    match interner.lookup(readonly_arr) {
        Some(TypeData::ReadonlyType(inner)) => {
            assert_eq!(inner, arr);
        }
        other => panic!("Expected ReadonlyType, got {other:?}"),
    }
}

#[test]
fn test_readonly_inference_object() {
    // Readonly<T> applied to object makes all properties readonly
    let interner = TypeInterner::new();

    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::NUMBER,
    )]);

    // Wrap in ReadonlyType
    let readonly_obj = interner.intern(TypeData::ReadonlyType(obj));

    match interner.lookup(readonly_obj) {
        Some(TypeData::ReadonlyType(inner)) => {
            assert_eq!(inner, obj);
        }
        other => panic!("Expected ReadonlyType, got {other:?}"),
    }
}

#[test]
fn test_readonly_keyof() {
    // keyof readonly [1, 2, 3] should work the same as keyof [1, 2, 3]
    let interner = TypeInterner::new();

    let one = interner.literal_number(1.0);
    let two = interner.literal_number(2.0);
    let three = interner.literal_number(3.0);

    let tuple = interner.tuple(vec![
        TupleElement {
            type_id: one,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: two,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: three,
            name: None,
            optional: false,
            rest: false,
        },
    ]);
    let readonly_tuple = interner.intern(TypeData::ReadonlyType(tuple));

    // keyof readonly tuple
    let result = evaluate_keyof(&interner, readonly_tuple);

    // Should include tuple indices: "0" | "1" | "2" | array methods
    // At minimum, verify it returns a union containing the indices
    match interner.lookup(result) {
        Some(TypeData::Union(_)) => {} // Expected - union of keys
        other => panic!("Expected Union from keyof readonly tuple, got {other:?}"),
    }
}

#[test]
fn test_template_literal_const_basic() {
    // const x = `hello` as const -> "hello"
    // Template literals with no interpolations become string literals
    let interner = TypeInterner::new();

    let hello = interner.literal_string("hello");

    // A simple template literal `hello` with as const is just "hello"
    match interner.lookup(hello) {
        Some(TypeData::Literal(LiteralValue::String(_))) => {}
        other => panic!("Expected LiteralString, got {other:?}"),
    }
}

#[test]
fn test_template_literal_const_interpolation() {
    // const prefix = "hello" as const
    // const x = `${prefix} world` as const -> "hello world"
    // With known literal interpolations, result is a literal
    let interner = TypeInterner::new();

    // When all parts are literals, the result is a literal
    let hello_world = interner.literal_string("hello world");

    match interner.lookup(hello_world) {
        Some(TypeData::Literal(LiteralValue::String(atom))) => {
            assert_eq!(interner.resolve_atom(atom), "hello world");
        }
        other => panic!("Expected LiteralString, got {other:?}"),
    }
}

#[test]
fn test_template_literal_type_structure() {
    // Template literal types: `prefix${string}suffix`
    let interner = TypeInterner::new();

    let prefix = interner.intern_string("prefix");
    let suffix = interner.intern_string("suffix");

    let template = interner.template_literal(vec![
        TemplateSpan::Text(prefix),
        TemplateSpan::Type(TypeId::STRING),
        TemplateSpan::Text(suffix),
    ]);

    match interner.lookup(template) {
        Some(TypeData::TemplateLiteral(spans_id)) => {
            let spans = interner.template_list(spans_id);
            assert_eq!(spans.len(), 3);
            match &spans[0] {
                TemplateSpan::Text(atom) => assert_eq!(interner.resolve_atom(*atom), "prefix"),
                _ => panic!("Expected Text span"),
            }
            match &spans[1] {
                TemplateSpan::Type(t) => assert_eq!(*t, TypeId::STRING),
                _ => panic!("Expected Type span"),
            }
            match &spans[2] {
                TemplateSpan::Text(atom) => assert_eq!(interner.resolve_atom(*atom), "suffix"),
                _ => panic!("Expected Text span"),
            }
        }
        other => panic!("Expected TemplateLiteral, got {other:?}"),
    }
}

#[test]
fn test_template_literal_union_expansion() {
    use crate::relations::subtype::SubtypeChecker;

    // `${"a" | "b"}` expands to "a" | "b"
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let union = interner.union(vec![lit_a, lit_b]);

    // A template with just a union interpolation equals the union
    let template = interner.template_literal(vec![TemplateSpan::Type(union)]);

    // The template should be a subtype of string
    assert!(checker.is_subtype_of(template, TypeId::STRING));
}

#[test]
fn test_const_enum_like_object() {
    use crate::relations::subtype::SubtypeChecker;

    // const Direction = { Up: 0, Down: 1, Left: 2, Right: 3 } as const
    // -> { readonly Up: 0, readonly Down: 1, readonly Left: 2, readonly Right: 3 }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let zero = interner.literal_number(0.0);
    let one = interner.literal_number(1.0);
    let two = interner.literal_number(2.0);
    let three = interner.literal_number(3.0);

    let direction = interner.object(vec![
        PropertyInfo::readonly(interner.intern_string("Down"), one),
        PropertyInfo::readonly(interner.intern_string("Left"), two),
        PropertyInfo::readonly(interner.intern_string("Right"), three),
        PropertyInfo::readonly(interner.intern_string("Up"), zero),
    ]);

    // Get keyof Direction = "Up" | "Down" | "Left" | "Right"
    let keys = evaluate_keyof(&interner, direction);

    // Each key literal is a subtype of string
    match interner.lookup(keys) {
        Some(TypeData::Union(members_id)) => {
            let members = interner.type_list(members_id);
            assert_eq!(members.len(), 4);
            for member in members.iter() {
                assert!(checker.is_subtype_of(*member, TypeId::STRING));
            }
        }
        other => panic!("Expected Union, got {other:?}"),
    }
}

// ============================================================================
// Omit<T, K> and Pick<T, K> Utility Type Tests
// ============================================================================
