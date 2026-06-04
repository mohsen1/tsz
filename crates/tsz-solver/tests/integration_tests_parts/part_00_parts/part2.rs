mod property_access_conformance_tests {
    use super::*;

    #[test]
    fn typed_array_to_locale_string_accepts_locale_arguments() {
        let interner = TypeInterner::new();
        let narrow_locale_method = interner.function(FunctionShape::new(
            vec![ParamInfo::required(
                interner.intern_string("locale"),
                TypeId::NUMBER,
            )],
            TypeId::STRING,
        ));

        let typed_array = interner.object_with_index(ObjectShape {
            symbol: None,
            flags: ObjectFlags::empty(),
            properties: vec![
                PropertyInfo::new(interner.intern_string("length"), TypeId::NUMBER),
                PropertyInfo::new(interner.intern_string("buffer"), TypeId::ANY),
                PropertyInfo::new(interner.intern_string("byteLength"), TypeId::NUMBER),
                PropertyInfo::new(interner.intern_string("byteOffset"), TypeId::NUMBER),
                PropertyInfo::method(
                    interner.intern_string("toLocaleString"),
                    narrow_locale_method,
                ),
            ],
            string_index: None,
            number_index: Some(IndexSignature {
                key_type: TypeId::NUMBER,
                value_type: TypeId::NUMBER,
                readonly: false,
                param_name: None,
            }),
        });

        let result = crate::operations::property::PropertyAccessEvaluator::new(&interner)
            .resolve_property_access(typed_array, "toLocaleString")
            .success_type()
            .expect("typed-array-like toLocaleString should resolve");

        let Some(TypeData::Function(shape_id)) = interner.lookup(result) else {
            panic!("toLocaleString should resolve to a function type");
        };
        let shape = interner.function_shape(shape_id);
        assert_eq!(shape.return_type, TypeId::STRING);
        assert_eq!(shape.params.len(), 1);
        assert!(
            shape.params[0].rest,
            "typed array toLocaleString must accept locales/options arguments"
        );
        assert!(matches!(
            interner.lookup(shape.params[0].type_id),
            Some(TypeData::Array(TypeId::ANY))
        ));
    }
}

mod unknown_fallback_tests {
    use super::*;

    #[test]
    fn test_function_this_parameter_compatibility() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        // Function with this parameter (explicit type)
        let func_with_this = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            }],
            this_type: Some(TypeId::STRING), // explicit this: string
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });

        // Function without this parameter
        let func_without_this = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });

        // TypeScript only checks `this` compatibility when the TARGET declares
        // an explicit `this` parameter. Since target has `this: string` and source
        // has no `this`, TypeScript skips the this check → compatible.
        assert!(checker.is_assignable(func_without_this, func_with_this));
    }

    #[test]
    fn test_generic_parameter_without_constraint_fallback_to_unknown() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        // Generic parameter without constraint should fallback to Unknown
        let t_param_unconstrained = interner.intern(TypeData::TypeParameter(TypeParamInfo {
            name: interner.intern_string("T"),
            constraint: None, // No constraint - should use Unknown
            default: None,
            is_const: false,
        }));

        // Create an object with number type
        let obj_type = interner.object(vec![PropertyInfo::new(
            interner.intern_string("value"),
            TypeId::NUMBER,
        )]);

        // With Unknown fallback, object should NOT be assignable to unconstrained generic
        // (Unknown doesn't automatically accept all types like Any does)
        assert!(!checker.is_assignable(obj_type, t_param_unconstrained));
    }

    #[test]
    fn test_array_without_type_argument_fallback_to_unknown() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        // Array<unknown> (what Array without type args should default to)
        let array_unknown = interner.array(TypeId::UNKNOWN);

        // Array<number>
        let array_number = interner.array(TypeId::NUMBER);

        // number[] is assignable to unknown[] (since unknown is a top type)
        assert!(checker.is_assignable(array_number, array_unknown));

        // But unknown[] is NOT assignable to number[] (unknown is strict)
        assert!(!checker.is_assignable(array_unknown, array_number));
    }

    #[test]
    fn test_unknown_fallback_prevents_silent_acceptance() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        // Type A: { value: number; }
        let type_a = interner.object(vec![PropertyInfo::new(
            interner.intern_string("value"),
            TypeId::NUMBER,
        )]);

        // Unknown type (what fallbacks should use)
        let unknown_type = TypeId::UNKNOWN;

        // Unknown should NOT be assignable to a specific type
        // (prevents silent acceptance of invalid code)
        assert!(!checker.is_assignable(unknown_type, type_a));

        // Everything is assignable to Unknown (it's a top type)
        assert!(checker.is_assignable(type_a, unknown_type));
    }

    #[test]
    fn test_unknown_vs_any_behavior() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        // Type A: { value: number; }
        let type_a = interner.object(vec![PropertyInfo::new(
            interner.intern_string("value"),
            TypeId::NUMBER,
        )]);

        // Any is assignable to anything (permissive)
        assert!(checker.is_assignable(TypeId::ANY, type_a));

        // Unknown is NOT assignable to specific type (strict)
        assert!(!checker.is_assignable(TypeId::UNKNOWN, type_a));

        // Everything is assignable to Any
        assert!(checker.is_assignable(type_a, TypeId::ANY));

        // Everything is assignable to Unknown (it's a top type)
        assert!(checker.is_assignable(type_a, TypeId::UNKNOWN));
    }
}
