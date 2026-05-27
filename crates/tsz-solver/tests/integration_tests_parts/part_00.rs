mod generic_strictness_tests {
    use super::*;

    // DELETED: test_generic_with_constraint_uses_constraint_not_any
    // This test expected is_assignable(source, T) to return TRUE when source satisfies T's constraint.
    // This is incorrect TypeScript behavior. A type parameter T is opaque - even if source
    // satisfies the constraint, source is NOT assignable to T because T could be instantiated
    // with a more specific subtype that source doesn't satisfy.
    //
    // Example: If T extends { id: number }, T could be instantiated as { id: number, tag: 'special' }
    // The test source { id: 5, name: 'hi' } does NOT have the 'tag' property, so it's not assignable.
    //
    // See test_generic_constraint_violation_fails below for the correct test (constraint violations fail).

    #[test]
    fn test_generic_constraint_violation_fails() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        // Create a generic type with constraint: T extends { id: number }
        let identifiable_constraint = interner.object(vec![PropertyInfo::new(
            interner.intern_string("id"),
            TypeId::NUMBER,
        )]);

        let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
            name: interner.intern_string("T"),
            constraint: Some(identifiable_constraint),
            default: None,
            is_const: false,
        }));

        // Create an instance WITHOUT the required property
        let obj_without_id = interner.object(vec![PropertyInfo::new(
            interner.intern_string("name"),
            TypeId::STRING,
        )]);

        // This should NOT be assignable - constraint is violated
        assert!(!checker.is_assignable(obj_without_id, t_param));
    }

    /// Test that unconstrained generic falls back to Unknown
    ///
    /// Unconstrained type parameters behave like unknown for assignability checking.
    /// While unknown is a "top type" (everything is assignable TO it), it does NOT
    /// accept all types (unknown is not assignable TO most types).
    #[test]
    fn test_unconstrained_generic_fallback_to_unknown() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        // Create an unconstrained generic parameter
        let t_param_unconstrained = interner.intern(TypeData::TypeParameter(TypeParamInfo {
            name: interner.intern_string("T"),
            constraint: None,
            default: None,
            is_const: false,
        }));

        // When checking against unconstrained generic, use Unknown (not Any)
        let number_type = TypeId::NUMBER;

        // Number should NOT be assignable to unconstrained generic
        // T behaves like unknown, and unknown doesn't accept concrete types
        // (T could be instantiated as string, which number is not assignable to)
        let result = checker.is_assignable(number_type, t_param_unconstrained);
        assert!(
            !result,
            "Concrete type should not be assignable to opaque type parameter"
        );
    }

    /// Test that multiple generic constraints are correctly combined
    ///
    /// NOTE: This test was deleted because it had incorrect expectations.
    /// The test expected `is_assignable(source`, T) to return TRUE when source satisfies T's constraint.
    /// This is wrong - type parameters are opaque and don't accept concrete types even if they
    /// satisfy the constraint. See deleted `test_generic_with_constraint_uses_constraint_not_any`
    /// for detailed explanation.

    #[test]
    fn test_generic_function_with_constraints() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        // Create function type: <T extends { id: number }>(obj: T): number
        let identifiable_constraint = interner.object(vec![PropertyInfo::new(
            interner.intern_string("id"),
            TypeId::NUMBER,
        )]);

        let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
            name: interner.intern_string("T"),
            constraint: Some(identifiable_constraint),
            default: None,
            is_const: false,
        }));

        let obj_param = interner.object(vec![PropertyInfo::new(
            interner.intern_string("id"),
            TypeId::NUMBER,
        )]);

        // Function type: (obj: T) => number
        let func_type = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("obj")),
                type_id: t_param,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::NUMBER,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });

        // Implementation: (obj: { id: number }) => number
        let impl_type = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("obj")),
                type_id: obj_param,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::NUMBER,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });

        // Implementation should be assignable to function type
        assert!(checker.is_assignable(impl_type, func_type));
    }
}
mod tuple_subtyping_tests {
    use super::*;

    #[test]
    fn test_tuple_covariant_subtyping_same_length() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        // [string, number]
        let tuple1 = interner.tuple(vec![
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
        ]);

        // [string | number, number | boolean]
        let tuple2 = interner.tuple(vec![
            TupleElement {
                type_id: interner.union(vec![TypeId::STRING, TypeId::NUMBER]),
                name: None,
                optional: false,
                rest: false,
            },
            TupleElement {
                type_id: interner.union(vec![TypeId::NUMBER, TypeId::BOOLEAN]),
                name: None,
                optional: false,
                rest: false,
            },
        ]);

        // tuple1 should be assignable to tuple2 (covariant elements)
        assert!(checker.is_assignable(tuple1, tuple2));
    }

    #[test]
    fn test_tuple_length_mismatch_fails() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        // [string, number]
        let tuple1 = interner.tuple(vec![
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
        ]);

        // [string, number, boolean]
        let tuple2 = interner.tuple(vec![
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
            TupleElement {
                type_id: TypeId::BOOLEAN,
                name: None,
                optional: false,
                rest: false,
            },
        ]);

        // tuple1 is NOT assignable to tuple2 (shorter tuple)
        assert!(!checker.is_assignable(tuple1, tuple2));

        // tuple2 is NOT assignable to tuple1 (longer tuple)
        assert!(!checker.is_assignable(tuple2, tuple1));
    }

    #[test]
    fn test_tuple_with_optional_elements() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        // [string, number?]
        let tuple_with_optional = interner.tuple(vec![
            TupleElement {
                type_id: TypeId::STRING,
                name: None,
                optional: false,
                rest: false,
            },
            TupleElement {
                type_id: TypeId::NUMBER,
                name: None,
                optional: true,
                rest: false,
            },
        ]);

        // [string]
        let tuple_shorter = interner.tuple(vec![TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        }]);

        // Shorter tuple should be assignable to tuple with optional trailing element
        assert!(checker.is_assignable(tuple_shorter, tuple_with_optional));
    }

    #[test]
    fn test_tuple_with_rest_element() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        // [string, ...number[]]
        let tuple_with_rest = interner.tuple(vec![
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

        // [string, number, number]
        let tuple_concrete = interner.tuple(vec![
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
            TupleElement {
                type_id: TypeId::NUMBER,
                name: None,
                optional: false,
                rest: false,
            },
        ]);

        // Concrete tuple should be assignable to tuple with rest
        assert!(checker.is_assignable(tuple_concrete, tuple_with_rest));
    }

    #[test]
    fn test_tuple_element_variance() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        // [number] where number <: string | number
        let tuple_literal = interner.tuple(vec![TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        }]);

        let tuple_union = interner.tuple(vec![TupleElement {
            type_id: interner.union(vec![TypeId::STRING, TypeId::NUMBER]),
            name: None,
            optional: false,
            rest: false,
        }]);

        // Covariant: literal should be assignable to union
        assert!(checker.is_assignable(tuple_literal, tuple_union));

        // NOT contravariant: union is NOT assignable to literal
        assert!(!checker.is_assignable(tuple_union, tuple_literal));
    }

    #[test]
    fn test_tuple_to_array_assignability() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        // [string, number]
        let tuple_type = interner.tuple(vec![
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
        ]);

        // string[] (array type - not a tuple with rest)
        let string_array = interner.array(TypeId::STRING);

        // Tuple [string, number] should NOT be assignable to string[]
        // because number is not assignable to string
        assert!(!checker.is_assignable(tuple_type, string_array));

        // (string | number)[]
        let union_array = interner.array(interner.union(vec![TypeId::STRING, TypeId::NUMBER]));

        // Tuple [string, number] SHOULD be assignable to (string | number)[]
        // because both string and number are subtypes of string | number
        assert!(checker.is_assignable(tuple_type, union_array));

        // [string, string]
        let string_tuple = interner.tuple(vec![
            TupleElement {
                type_id: TypeId::STRING,
                name: None,
                optional: false,
                rest: false,
            },
            TupleElement {
                type_id: TypeId::STRING,
                name: None,
                optional: false,
                rest: false,
            },
        ]);

        // [string, string] SHOULD be assignable to string[]
        assert!(checker.is_assignable(string_tuple, string_array));
    }

    #[test]
    fn test_tuple_to_array_with_rest_element() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        // [string, ...number[]] - tuple with rest
        let tuple_with_rest = interner.tuple(vec![
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

        // (string | number)[]
        let union_array = interner.array(interner.union(vec![TypeId::STRING, TypeId::NUMBER]));

        // [string, ...number[]] SHOULD be assignable to (string | number)[]
        assert!(checker.is_assignable(tuple_with_rest, union_array));

        // number[]
        let number_array = interner.array(TypeId::NUMBER);

        // [string, ...number[]] should NOT be assignable to number[]
        // (string is not assignable to number)
        assert!(!checker.is_assignable(tuple_with_rest, number_array));
    }

    #[test]
    fn test_named_tuple_elements() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        // [name: string, age: number]
        let named_tuple = interner.tuple(vec![
            TupleElement {
                type_id: TypeId::STRING,
                name: Some(interner.intern_string("name")),
                optional: false,
                rest: false,
            },
            TupleElement {
                type_id: TypeId::NUMBER,
                name: Some(interner.intern_string("age")),
                optional: false,
                rest: false,
            },
        ]);

        // [string, number]
        let unnamed_tuple = interner.tuple(vec![
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
        ]);

        // Named and unnamed tuples with same types should be compatible
        assert!(checker.is_assignable(named_tuple, unnamed_tuple));
        assert!(checker.is_assignable(unnamed_tuple, named_tuple));
    }
}
mod function_variance_tests {
    use super::*;

    #[test]
    fn test_function_parameter_contravariance() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        // Enable strict function types for proper contravariance
        checker.set_strict_function_types(true);

        // (x: number) => void
        let func_number = interner.function(FunctionShape {
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

        // (x: string | number) => void
        let func_union = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: interner.union(vec![TypeId::STRING, TypeId::NUMBER]),
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });

        // In strict mode, function parameters are contravariant
        // func_union should be assignable to func_number
        // because string | number is a supertype of number
        assert!(checker.is_assignable(func_union, func_number));

        // func_number is NOT assignable to func_union
        assert!(!checker.is_assignable(func_number, func_union));
    }

    #[test]
    fn test_function_return_type_covariance() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        // () => number
        let func_returns_number = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: TypeId::NUMBER,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });

        // () => string | number
        let func_returns_union = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: interner.union(vec![TypeId::STRING, TypeId::NUMBER]),
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });

        // Return types are covariant
        // func_returns_number should be assignable to func_returns_union
        assert!(checker.is_assignable(func_returns_number, func_returns_union));

        // func_returns_union is NOT assignable to func_returns_number
        assert!(!checker.is_assignable(func_returns_union, func_returns_number));
    }

    #[test]
    fn test_function_bivariant_without_strict() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        // Ensure strict function types is OFF (default)
        checker.set_strict_function_types(false);

        // (x: number) => void
        let func_number = interner.function(FunctionShape {
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

        // (x: string | number) => void
        let func_union = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: interner.union(vec![TypeId::STRING, TypeId::NUMBER]),
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });

        // Without strict function types, parameters are bivariant
        // Both directions should succeed
        assert!(checker.is_assignable(func_number, func_union));
        assert!(checker.is_assignable(func_union, func_number));
    }

    #[test]
    fn test_function_with_multiple_parameters() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        checker.set_strict_function_types(true);

        // (x: number, y: string) => void
        let func1 = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![
                ParamInfo {
                    name: Some(interner.intern_string("x")),
                    type_id: TypeId::NUMBER,
                    optional: false,
                    rest: false,
                },
                ParamInfo {
                    name: Some(interner.intern_string("y")),
                    type_id: TypeId::STRING,
                    optional: false,
                    rest: false,
                },
            ],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });

        // (x: string | number, y: string | number) => void
        let func2 = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![
                ParamInfo {
                    name: Some(interner.intern_string("x")),
                    type_id: interner.union(vec![TypeId::STRING, TypeId::NUMBER]),
                    optional: false,
                    rest: false,
                },
                ParamInfo {
                    name: Some(interner.intern_string("y")),
                    type_id: interner.union(vec![TypeId::STRING, TypeId::NUMBER]),
                    optional: false,
                    rest: false,
                },
            ],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });

        // func2 should be assignable to func1 (contravariant parameters)
        assert!(checker.is_assignable(func2, func1));
        assert!(!checker.is_assignable(func1, func2));
    }

    #[test]
    fn test_function_with_optional_parameters() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        checker.set_strict_function_types(true);

        // (x: number, y?: string) => void
        let func_with_optional = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![
                ParamInfo {
                    name: Some(interner.intern_string("x")),
                    type_id: TypeId::NUMBER,
                    optional: false,
                    rest: false,
                },
                ParamInfo {
                    name: Some(interner.intern_string("y")),
                    type_id: TypeId::STRING,
                    optional: true,
                    rest: false,
                },
            ],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });

        // (x: number) => void
        let func_without_optional = interner.function(FunctionShape {
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

        // Function with fewer required parameters should be assignable
        assert!(checker.is_assignable(func_without_optional, func_with_optional));
    }

    #[test]
    fn test_function_with_rest_parameters() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        checker.set_strict_function_types(true);

        // (...args: number[]) => void
        let func_with_rest = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("args")),
                type_id: TypeId::NUMBER,
                optional: false,
                rest: true,
            }],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });

        // (x: number, y: number) => void
        let func_with_params = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![
                ParamInfo {
                    name: Some(interner.intern_string("x")),
                    type_id: TypeId::NUMBER,
                    optional: false,
                    rest: false,
                },
                ParamInfo {
                    name: Some(interner.intern_string("y")),
                    type_id: TypeId::NUMBER,
                    optional: false,
                    rest: false,
                },
            ],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });

        // Rest parameter functions should be assignable to fixed parameter functions
        assert!(checker.is_assignable(func_with_rest, func_with_params));
    }

    #[test]
    fn test_function_this_parameter() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        // Object type with method
        let obj_type = interner.object(vec![PropertyInfo::new(
            interner.intern_string("value"),
            TypeId::NUMBER,
        )]);

        // (this: ObjType) => void
        let func_with_this = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![],
            this_type: Some(obj_type),
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });

        // Regular function without this parameter
        let func_without_this = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });

        // Functions should be compatible regardless of this annotation
        // (this is handled differently in TypeScript)
        assert!(checker.is_assignable(func_without_this, func_with_this));
    }

    #[test]
    fn test_generic_function_variance() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        checker.set_strict_function_types(true);

        // <T>(x: T) => T
        let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
            name: interner.intern_string("T"),
            constraint: None,
            default: None,
            is_const: false,
        }));

        let generic_func = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: t_param,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: t_param,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });

        // <T>(x: T) => T where T extends number
        let number_constraint = TypeId::NUMBER;
        let t_constrained = interner.intern(TypeData::TypeParameter(TypeParamInfo {
            name: interner.intern_string("T"),
            constraint: Some(number_constraint),
            default: None,
            is_const: false,
        }));

        let constrained_func = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: t_constrained,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: t_constrained,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });

        // Constrained function should be assignable to unconstrained
        // when the constraint is satisfied
        assert!(checker.is_assignable(constrained_func, generic_func));
    }

    #[test]
    fn test_strict_member_compat_rejects_outer_type_param_as_generic_signature() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);
        checker.set_erase_generics(false);

        let source_t = TypeParamInfo {
            name: interner.intern_string("T"),
            constraint: None,
            default: None,
            is_const: false,
        };
        let source_t_type = interner.type_param(source_t);
        let source = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: source_t_type,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });

        let target_t = TypeParamInfo {
            name: interner.intern_string("T"),
            constraint: None,
            default: None,
            is_const: false,
        };
        let target_t_type = interner.type_param(target_t);
        let target = interner.function(FunctionShape {
            type_params: vec![target_t],
            params: vec![],
            this_type: None,
            return_type: target_t_type,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });

        assert!(
            !checker.is_assignable(source, target),
            "strict TS2416/TS2430 member compatibility must not promote an outer type parameter into a generic signature"
        );
    }
}
mod lawyer_strict_mode_tests {
    use super::*;

    fn object_with_property(interner: &TypeInterner, name: &str, type_id: TypeId) -> TypeId {
        interner.object(vec![PropertyInfo {
            name: interner.intern_string(name),
            type_id,
            write_type: type_id,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
            is_symbol_named: false,
            single_quoted_name: false,
        }])
    }

    #[test]
    fn test_strict_mode_any_does_not_suppress_errors() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        // Enable strict any propagation
        checker.set_strict_any_propagation(true);

        // Create object with 'any' property
        let obj_with_any = object_with_property(&interner, "value", TypeId::ANY);

        // Create object with 'number' property
        let obj_with_number = object_with_property(&interner, "value", TypeId::NUMBER);

        // In strict mode, 'any' should NOT silence structural mismatches
        // obj_with_any should NOT be assignable to obj_with_number
        assert!(!checker.is_assignable(obj_with_any, obj_with_number));
    }

    #[test]
    fn test_non_strict_mode_any_suppresses_errors() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        // Ensure non-strict mode (default)
        checker.set_strict_any_propagation(false);

        // Create object with 'any' property
        let obj_with_any = object_with_property(&interner, "value", TypeId::ANY);

        // Create object with 'number' property
        let obj_with_number = object_with_property(&interner, "value", TypeId::NUMBER);

        // In non-strict mode, 'any' should silence errors
        // obj_with_any SHOULD be assignable to obj_with_number
        assert!(checker.is_assignable(obj_with_any, obj_with_number));
    }

    #[test]
    fn test_any_direct_assignment_in_strict_mode() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        checker.set_strict_any_propagation(true);

        // Direct assignment: any -> specific type
        // This should still work (it's a direct any assignment)
        assert!(checker.is_assignable(TypeId::ANY, TypeId::NUMBER));

        // Direct assignment: specific type -> any
        assert!(checker.is_assignable(TypeId::NUMBER, TypeId::ANY));
    }

    #[test]
    fn test_any_in_complex_structure_strict_mode() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        checker.set_strict_any_propagation(true);

        // [any, number]
        let tuple_with_any = interner.tuple(vec![
            TupleElement {
                type_id: TypeId::ANY,
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
        ]);

        // [number, number]
        let tuple_number = interner.tuple(vec![
            TupleElement {
                type_id: TypeId::NUMBER,
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
        ]);

        // In strict mode, tuple with 'any' should NOT match tuple with 'number'
        assert!(!checker.is_assignable(tuple_with_any, tuple_number));
    }

    #[test]
    fn test_any_in_tuple_non_strict_mode_allows_assignment() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        checker.set_strict_any_propagation(false);

        let tuple_with_any = interner.tuple(vec![
            TupleElement {
                type_id: TypeId::ANY,
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
        ]);

        let tuple_number = interner.tuple(vec![
            TupleElement {
                type_id: TypeId::NUMBER,
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
        ]);

        assert!(checker.is_assignable(tuple_with_any, tuple_number));
    }

    #[test]
    fn test_any_in_array_element_strict_mode_blocks_assignment() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        checker.set_strict_any_propagation(true);

        let array_any = interner.array(TypeId::ANY);
        let array_number = interner.array(TypeId::NUMBER);

        assert!(!checker.is_assignable(array_any, array_number));
    }

    #[test]
    fn test_any_in_array_element_non_strict_mode_allows_assignment() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        checker.set_strict_any_propagation(false);

        let array_any = interner.array(TypeId::ANY);
        let array_number = interner.array(TypeId::NUMBER);

        assert!(checker.is_assignable(array_any, array_number));
    }

    #[test]
    fn test_any_in_nested_object_property_strict_mode_blocks_assignment() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        checker.set_strict_any_propagation(true);

        let inner_any = object_with_property(&interner, "inner", TypeId::ANY);
        let outer_any = object_with_property(&interner, "value", inner_any);

        let inner_number = object_with_property(&interner, "inner", TypeId::NUMBER);
        let outer_number = object_with_property(&interner, "value", inner_number);

        assert!(!checker.is_assignable(outer_any, outer_number));
    }

    #[test]
    fn test_any_in_nested_object_property_non_strict_allows_assignment() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        checker.set_strict_any_propagation(false);

        let inner_any = object_with_property(&interner, "inner", TypeId::ANY);
        let outer_any = object_with_property(&interner, "value", inner_any);

        let inner_number = object_with_property(&interner, "inner", TypeId::NUMBER);
        let outer_number = object_with_property(&interner, "value", inner_number);

        assert!(checker.is_assignable(outer_any, outer_number));
    }

    #[test]
    fn test_any_in_return_type_strict_mode_blocks_assignment() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        checker.set_strict_any_propagation(true);

        let fn_any_return = interner.function(FunctionShape {
            type_params: Vec::new(),
            params: Vec::new(),
            this_type: None,
            return_type: TypeId::ANY,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });

        let fn_number_return = interner.function(FunctionShape {
            type_params: Vec::new(),
            params: Vec::new(),
            this_type: None,
            return_type: TypeId::NUMBER,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });

        assert!(!checker.is_assignable(fn_any_return, fn_number_return));
    }

    #[test]
    fn test_any_in_return_type_non_strict_mode_allows_assignment() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        checker.set_strict_any_propagation(false);

        let fn_any_return = interner.function(FunctionShape {
            type_params: Vec::new(),
            params: Vec::new(),
            this_type: None,
            return_type: TypeId::ANY,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });

        let fn_number_return = interner.function(FunctionShape {
            type_params: Vec::new(),
            params: Vec::new(),
            this_type: None,
            return_type: TypeId::NUMBER,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });

        assert!(checker.is_assignable(fn_any_return, fn_number_return));
    }

    #[test]
    fn test_any_in_string_index_signature_strict_mode_blocks_assignment() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        checker.set_strict_any_propagation(true);

        let string_index_any = IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::ANY,
            readonly: false,
            param_name: None,
        };
        let string_index_number = IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        };

        let obj_any = interner.object_with_index(ObjectShape {
            symbol: None,
            flags: ObjectFlags::empty(),
            properties: Vec::new(),
            string_index: Some(string_index_any),
            number_index: None,
        });
        let obj_number = interner.object_with_index(ObjectShape {
            symbol: None,
            flags: ObjectFlags::empty(),
            properties: Vec::new(),
            string_index: Some(string_index_number),
            number_index: None,
        });

        assert!(!checker.is_assignable(obj_any, obj_number));
    }

    #[test]
    fn test_any_in_string_index_signature_non_strict_allows_assignment() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        checker.set_strict_any_propagation(false);

        let string_index_any = IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::ANY,
            readonly: false,
            param_name: None,
        };
        let string_index_number = IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        };

        let obj_any = interner.object_with_index(ObjectShape {
            symbol: None,
            flags: ObjectFlags::empty(),
            properties: Vec::new(),
            string_index: Some(string_index_any),
            number_index: None,
        });
        let obj_number = interner.object_with_index(ObjectShape {
            symbol: None,
            flags: ObjectFlags::empty(),
            properties: Vec::new(),
            string_index: Some(string_index_number),
            number_index: None,
        });

        assert!(checker.is_assignable(obj_any, obj_number));
    }
}
mod error_detection_tests {
    use super::*;

    #[test]
    fn test_missing_property_detection() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        // Type A: { a: string; b: number; }
        let type_a = interner.object(vec![
            PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
            PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
        ]);

        // Type B: { a: string; }
        let type_b = interner.object(vec![PropertyInfo::new(
            interner.intern_string("a"),
            TypeId::STRING,
        )]);

        // type_b should NOT be assignable to type_a (missing property 'b')
        assert!(!checker.is_assignable(type_b, type_a));
    }

    #[test]
    fn test_property_type_mismatch_detection() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        // Type A: { value: number; }
        let type_a = interner.object(vec![PropertyInfo::new(
            interner.intern_string("value"),
            TypeId::NUMBER,
        )]);

        // Type B: { value: string; }
        let type_b = interner.object(vec![PropertyInfo::new(
            interner.intern_string("value"),
            TypeId::STRING,
        )]);

        // type_b should NOT be assignable to type_a (property type mismatch)
        assert!(!checker.is_assignable(type_b, type_a));
    }

    #[test]
    fn test_excess_property_allowed() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        // Type A: { a: string; }
        let type_a = interner.object(vec![PropertyInfo::new(
            interner.intern_string("a"),
            TypeId::STRING,
        )]);

        // Type B: { a: string; b: number; }
        let type_b = interner.object(vec![
            PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
            PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
        ]);

        // type_b SHOULD be assignable to type_a (excess properties allowed)
        assert!(checker.is_assignable(type_b, type_a));
    }

    /// Test that excess property check for union targets uses union semantics.
    ///
    /// In TypeScript, when assigning a fresh object literal to a union type,
    /// a property is "known" (not excess) if it exists in ANY constituent.
    /// `{ a: "x", b: 1 }` assigned to `{ a: string } | { b: number }` should
    /// not report excess properties because `a` exists in member 1 and `b`
    /// exists in member 2.
    #[test]
    fn test_excess_property_union_target_any_member() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        let a_name = interner.intern_string("a");
        let b_name = interner.intern_string("b");
        let c_name = interner.intern_string("c");

        // Target union: { a: string } | { b: number }
        let member_a = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);
        let member_b = interner.object(vec![PropertyInfo::new(b_name, TypeId::NUMBER)]);
        let target = interner.union(vec![member_a, member_b]);

        // Fresh source: { a: "hello", b: 42 }
        // Both properties exist in SOME member → no excess
        let source = interner.object_fresh(vec![
            PropertyInfo::new(a_name, TypeId::STRING),
            PropertyInfo::new(b_name, TypeId::NUMBER),
        ]);
        assert!(checker.is_assignable(source, target));

        // Fresh source: { a: "hello", b: 42, c: true }
        // `c` doesn't exist in ANY member → excess property
        let source_with_excess = interner.object_fresh(vec![
            PropertyInfo::new(a_name, TypeId::STRING),
            PropertyInfo::new(b_name, TypeId::NUMBER),
            PropertyInfo::new(c_name, TypeId::BOOLEAN),
        ]);
        assert!(!checker.is_assignable(source_with_excess, target));
    }

    /// Test function parameter count compatibility.
    ///
    /// In TypeScript, a function with fewer parameters IS assignable to one with
    /// more parameters (excess parameters in target are ignored by caller).
    #[test]
    fn test_function_parameter_count_mismatch() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        checker.set_strict_function_types(true);

        // (x: number, y: string) => void
        let func_two_params = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![
                ParamInfo {
                    name: Some(interner.intern_string("x")),
                    type_id: TypeId::NUMBER,
                    optional: false,
                    rest: false,
                },
                ParamInfo {
                    name: Some(interner.intern_string("y")),
                    type_id: TypeId::STRING,
                    optional: false,
                    rest: false,
                },
            ],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });

        // (x: number) => void
        let func_one_param = interner.function(FunctionShape {
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

        // func_one_param IS assignable to func_two_params (fewer source params is fine in TS)
        assert!(checker.is_assignable(func_one_param, func_two_params));

        // func_two_params is NOT assignable to func_one_param (too many required params)
        assert!(!checker.is_assignable(func_two_params, func_one_param));
    }

    #[test]
    fn test_tuple_length_mismatch_detection() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        // [string, number]
        let tuple_two = interner.tuple(vec![
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
        ]);

        // [string]
        let tuple_one = interner.tuple(vec![TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        }]);

        // tuple_one is NOT assignable to tuple_two (length mismatch)
        assert!(!checker.is_assignable(tuple_one, tuple_two));

        // tuple_two is NOT assignable to tuple_one (length mismatch)
        assert!(!checker.is_assignable(tuple_two, tuple_one));
    }
}
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
