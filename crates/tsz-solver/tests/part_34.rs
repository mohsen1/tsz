use super::*;
use crate::TypeInterner;
use crate::def::DefId;
use crate::{SubtypeChecker, TypeSubstitution, instantiate_type};
#[test]
fn test_satisfies_missing_property_fails() {
    use crate::SubtypeChecker;

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
    use crate::SubtypeChecker;

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
        },
    ]);

    // Missing optional property is ok
    assert!(checker.is_subtype_of(source, target));
}

#[test]
fn test_satisfies_vs_annotation_literal_preservation() {
    use crate::SubtypeChecker;

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
    use crate::SubtypeChecker;

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
    use crate::SubtypeChecker;

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
    use crate::SubtypeChecker;

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
    use crate::SubtypeChecker;

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
    use crate::SubtypeChecker;

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
    use crate::SubtypeChecker;
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
    use crate::SubtypeChecker;

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
    use crate::SubtypeChecker;

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

