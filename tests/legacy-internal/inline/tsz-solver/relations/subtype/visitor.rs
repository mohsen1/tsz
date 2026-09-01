//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/relations/subtype/visitor.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 6662a432add431753e3faaad81f10849fc1d7451de8b191c5a84e2495612c803 1283 visitor_accepts_function_shape_as_structural_function_interface
    #[test]
    fn visitor_accepts_function_shape_as_structural_function_interface() {
        let interner = TypeInterner::new();
        let source = interner.function(FunctionShape::new(vec![], TypeId::VOID));
        let target = structural_function_interface(&interner);
        let shape_id = function_shape_id(&interner, source).expect("function shape");

        let mut checker = SubtypeChecker::new(&interner);
        let mut visitor = SubtypeVisitor {
            checker: &mut checker,
            source,
            target,
        };

        assert_eq!(visitor.visit_function(shape_id.0), SubtypeResult::True);
    }
// TSZ_INLINE_TEST_END 6662a432add431753e3faaad81f10849fc1d7451de8b191c5a84e2495612c803

// TSZ_INLINE_TEST_BEGIN cceaa381d89ce29dca74c24f859be4444acb7b6919f3884254773735de7b8a6e 1300 visitor_accepts_callable_shape_as_structural_function_interface
    #[test]
    fn visitor_accepts_callable_shape_as_structural_function_interface() {
        let interner = TypeInterner::new();
        let source = interner.callable(CallableShape {
            call_signatures: vec![CallSignature::new(vec![], TypeId::VOID)],
            ..Default::default()
        });
        let target = structural_function_interface(&interner);
        let shape_id = callable_shape_id(&interner, source).expect("callable shape");

        let mut checker = SubtypeChecker::new(&interner);
        let mut visitor = SubtypeVisitor {
            checker: &mut checker,
            source,
            target,
        };

        assert_eq!(visitor.visit_callable(shape_id.0), SubtypeResult::True);
    }
// TSZ_INLINE_TEST_END cceaa381d89ce29dca74c24f859be4444acb7b6919f3884254773735de7b8a6e

// TSZ_INLINE_TEST_BEGIN 9f038a968a65536d2ec57b51f0d4ace52c8fa173557db99192d627beb84f3e92 1320 intersection_function_and_object_satisfies_callable_with_properties
    #[test]
    fn intersection_function_and_object_satisfies_callable_with_properties() {
        let interner = TypeInterner::new();
        let member_name = interner.intern_string("member");
        let source_function = interner.function(FunctionShape::new(vec![], TypeId::STRING));
        let source_props = interner.object(vec![PropertyInfo::new(member_name, TypeId::NUMBER)]);
        let source = interner.intersection2(source_function, source_props);
        let target = interner.callable(CallableShape {
            call_signatures: vec![CallSignature::new(vec![], TypeId::STRING)],
            properties: vec![PropertyInfo::new(member_name, TypeId::NUMBER)],
            ..Default::default()
        });

        let mut checker = SubtypeChecker::new(&interner);
        let mut visitor = SubtypeVisitor {
            checker: &mut checker,
            source,
            target,
        };
        let list_id =
            crate::visitor::intersection_list_id(&interner, source).expect("intersection source");

        assert_eq!(visitor.visit_intersection(list_id.0), SubtypeResult::True);
    }
// TSZ_INLINE_TEST_END 9f038a968a65536d2ec57b51f0d4ace52c8fa173557db99192d627beb84f3e92
