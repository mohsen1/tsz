//! Integration tests for solver strictness improvements.
//!
//! This module tests the comprehensive solver improvements made in SOLV-15, SOLV-18, and SOLV-19:
//! - Generic type constraints (SOLV-15): Using constraints instead of falling back to Any
//! - Tuple type subtyping (SOLV-18): Covariant tuple subtyping with proper length handling
//! - Function type variance (SOLV-19): Proper contravariance for parameter types
//!
//! These integration tests verify TS2322 and TS7006 error detection improves with strictness.

use super::*;

/// Test suite for SOLV-15: Generic type strict subtyping
#[cfg(test)]
mod generic_strictness_tests {
    use super::*;

    /// Test that generic with constraint uses constraint, not `any`
    ///
    /// NOTE: Currently ignored - generic constraint usage in strict subtyping is not fully
    /// implemented. Generic types should use their constraint for subtyping checks, not
    /// be treated as `any`.
    #[test]
    fn test_generic_with_constraint_uses_constraint_not_any() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        // Create a generic type with constraint: T extends { id: number }
        let identifiable_constraint = interner.object(vec![PropertyInfo {
            name: interner.intern_string("id"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        }]);

        let t_param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
            name: interner.intern_string("T"),
            constraint: Some(identifiable_constraint),
            default: None,
        }));

        // Create an instance with the constraint satisfied
        let obj_with_id = interner.object(vec![
            PropertyInfo {
                name: interner.intern_string("id"),
                type_id: TypeId::NUMBER,
                write_type: TypeId::NUMBER,
                optional: false,
                readonly: false,
                is_method: false,
            },
            PropertyInfo {
                name: interner.intern_string("name"),
                type_id: TypeId::STRING,
                write_type: TypeId::STRING,
                optional: false,
                readonly: false,
                is_method: false,
            },
        ]);

        // This should be assignable - constraint is satisfied
        assert!(checker.is_assignable(obj_with_id, t_param));
    }

    #[test]
    fn test_generic_constraint_violation_fails() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        // Create a generic type with constraint: T extends { id: number }
        let identifiable_constraint = interner.object(vec![PropertyInfo {
            name: interner.intern_string("id"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        }]);

        let t_param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
            name: interner.intern_string("T"),
            constraint: Some(identifiable_constraint),
            default: None,
        }));

        // Create an instance WITHOUT the required property
        let obj_without_id = interner.object(vec![PropertyInfo {
            name: interner.intern_string("name"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }]);

        // This should NOT be assignable - constraint is violated
        assert!(!checker.is_assignable(obj_without_id, t_param));
    }

    /// Test that unconstrained generic falls back to Unknown
    ///
    /// NOTE: Currently ignored - unconstrained generic fallback to Unknown is not fully
    /// implemented. When checking against an unconstrained generic, the checker should
    /// use Unknown as the fallback type, but this is not working correctly.
    #[test]
    #[ignore = "Unconstrained generic fallback to Unknown not fully implemented"]
    fn test_unconstrained_generic_fallback_to_unknown() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        // Create an unconstrained generic parameter
        let t_param_unconstrained = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
            name: interner.intern_string("T"),
            constraint: None,
            default: None,
        }));

        // When checking against unconstrained generic, use Unknown (not Any)
        // Unknown is a top type but not assignable without check
        let number_type = TypeId::NUMBER;

        // Number should be assignable to unconstrained generic
        // (unconstrained generic effectively becomes Unknown, which is a top type)
        let result = checker.is_assignable(number_type, t_param_unconstrained);
        assert!(result);
    }

    /// Test that multiple generic constraints are correctly combined
    ///
    /// NOTE: Currently ignored - multiple generic constraint combination is not fully
    /// implemented. The type checker should combine multiple constraints using
    /// intersection types, but this is not working correctly.
    #[test]
    fn test_multiple_generic_constraints() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        // T extends { length: number }
        let length_constraint = interner.object(vec![PropertyInfo {
            name: interner.intern_string("length"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        }]);

        // U extends { name: string }
        let name_constraint = interner.object(vec![PropertyInfo {
            name: interner.intern_string("name"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }]);

        let t_param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
            name: interner.intern_string("T"),
            constraint: Some(length_constraint),
            default: None,
        }));

        let u_param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
            name: interner.intern_string("U"),
            constraint: Some(name_constraint),
            default: None,
        }));

        // Object satisfying both constraints
        let combined = interner.object(vec![
            PropertyInfo {
                name: interner.intern_string("length"),
                type_id: TypeId::NUMBER,
                write_type: TypeId::NUMBER,
                optional: false,
                readonly: false,
                is_method: false,
            },
            PropertyInfo {
                name: interner.intern_string("name"),
                type_id: TypeId::STRING,
                write_type: TypeId::STRING,
                optional: false,
                readonly: false,
                is_method: false,
            },
        ]);

        // Should satisfy both constraints
        assert!(checker.is_assignable(combined, t_param));
        assert!(checker.is_assignable(combined, u_param));
    }

    #[test]
    fn test_generic_function_with_constraints() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        // Create function type: <T extends { id: number }>(obj: T): number
        let identifiable_constraint = interner.object(vec![PropertyInfo {
            name: interner.intern_string("id"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        }]);

        let t_param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
            name: interner.intern_string("T"),
            constraint: Some(identifiable_constraint),
            default: None,
        }));

        let obj_param = interner.object(vec![PropertyInfo {
            name: interner.intern_string("id"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        }]);

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

/// Test suite for SOLV-18: Tuple type subtyping
#[cfg(test)]
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

/// Test suite for SOLV-19: Function type variance
#[cfg(test)]
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
        let obj_type = interner.object(vec![PropertyInfo {
            name: interner.intern_string("value"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        }]);

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
        let t_param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
            name: interner.intern_string("T"),
            constraint: None,
            default: None,
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
        let t_constrained = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
            name: interner.intern_string("T"),
            constraint: Some(number_constraint),
            default: None,
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
}

/// Test suite for lawyer strict mode integration
#[cfg(test)]
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
        };
        let string_index_number = IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
        };

        let obj_any = interner.object_with_index(ObjectShape {
                flags: ObjectFlags::empty(),
            properties: Vec::new(),
            string_index: Some(string_index_any),
            number_index: None,
        });
        let obj_number = interner.object_with_index(ObjectShape {
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
        };
        let string_index_number = IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
        };

        let obj_any = interner.object_with_index(ObjectShape {
                flags: ObjectFlags::empty(),
            properties: Vec::new(),
            string_index: Some(string_index_any),
            number_index: None,
        });
        let obj_number = interner.object_with_index(ObjectShape {
                flags: ObjectFlags::empty(),
            properties: Vec::new(),
            string_index: Some(string_index_number),
            number_index: None,
        });

        assert!(checker.is_assignable(obj_any, obj_number));
    }
}

/// Test suite for error detection improvements
#[cfg(test)]
mod error_detection_tests {
    use super::*;

    #[test]
    fn test_missing_property_detection() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        // Type A: { a: string; b: number; }
        let type_a = interner.object(vec![
            PropertyInfo {
                name: interner.intern_string("a"),
                type_id: TypeId::STRING,
                write_type: TypeId::STRING,
                optional: false,
                readonly: false,
                is_method: false,
            },
            PropertyInfo {
                name: interner.intern_string("b"),
                type_id: TypeId::NUMBER,
                write_type: TypeId::NUMBER,
                optional: false,
                readonly: false,
                is_method: false,
            },
        ]);

        // Type B: { a: string; }
        let type_b = interner.object(vec![PropertyInfo {
            name: interner.intern_string("a"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }]);

        // type_b should NOT be assignable to type_a (missing property 'b')
        assert!(!checker.is_assignable(type_b, type_a));
    }

    #[test]
    fn test_property_type_mismatch_detection() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        // Type A: { value: number; }
        let type_a = interner.object(vec![PropertyInfo {
            name: interner.intern_string("value"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        }]);

        // Type B: { value: string; }
        let type_b = interner.object(vec![PropertyInfo {
            name: interner.intern_string("value"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }]);

        // type_b should NOT be assignable to type_a (property type mismatch)
        assert!(!checker.is_assignable(type_b, type_a));
    }

    #[test]
    fn test_excess_property_allowed() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        // Type A: { a: string; }
        let type_a = interner.object(vec![PropertyInfo {
            name: interner.intern_string("a"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }]);

        // Type B: { a: string; b: number; }
        let type_b = interner.object(vec![
            PropertyInfo {
                name: interner.intern_string("a"),
                type_id: TypeId::STRING,
                write_type: TypeId::STRING,
                optional: false,
                readonly: false,
                is_method: false,
            },
            PropertyInfo {
                name: interner.intern_string("b"),
                type_id: TypeId::NUMBER,
                write_type: TypeId::NUMBER,
                optional: false,
                readonly: false,
                is_method: false,
            },
        ]);

        // type_b SHOULD be assignable to type_a (excess properties allowed)
        assert!(checker.is_assignable(type_b, type_a));
    }

    /// Test function parameter count mismatch detection
    ///
    /// NOTE: Currently ignored - function parameter count mismatch detection is not fully
    /// implemented. The type checker should reject assignments between functions with
    /// different parameter counts, but this is not being detected correctly.
    #[test]
    #[ignore = "Function parameter count mismatch detection not fully implemented"]
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

        // func_one_param is NOT assignable to func_two_params (missing parameter)
        assert!(!checker.is_assignable(func_one_param, func_two_params));

        // func_two_params IS assignable to func_one_param (more specific)
        assert!(checker.is_assignable(func_two_params, func_one_param));
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

/// Test suite for Unknown fallback strictness
/// Verifies that the solver uses Unknown instead of Any for stricter type checking
#[cfg(test)]
mod unknown_fallback_tests {
    use super::*;

    #[test]
    #[ignore = "Function this parameter fallback to Unknown not fully implemented"]
    fn test_function_this_parameter_fallback_to_unknown() {
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

        // Function without this parameter (should fall back to Unknown, not Any)
        let func_without_this = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            }],
            this_type: None, // No this parameter - should fallback to Unknown
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });

        // With Unknown fallback, functions should NOT be compatible
        // when one has explicit this type and the other has None
        // (Unknown is not assignable to any specific type)
        assert!(!checker.is_assignable(func_without_this, func_with_this));
    }

    #[test]
    fn test_generic_parameter_without_constraint_fallback_to_unknown() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        // Generic parameter without constraint should fallback to Unknown
        let t_param_unconstrained = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
            name: interner.intern_string("T"),
            constraint: None, // No constraint - should use Unknown
            default: None,
        }));

        // Create an object with number type
        let obj_type = interner.object(vec![PropertyInfo {
            name: interner.intern_string("value"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        }]);

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
        let type_a = interner.object(vec![PropertyInfo {
            name: interner.intern_string("value"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        }]);

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
        let type_a = interner.object(vec![PropertyInfo {
            name: interner.intern_string("value"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        }]);

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

/// Test suite for SOLVER.md Section 8.2: TypeScript Quirks (The Lawyer Layer)
///
/// This module tests the intentional unsoundness in TypeScript that we must support
/// for compatibility. These are not bugs - they are documented design decisions.
#[cfg(test)]
mod typescript_quirks_tests {
    use super::*;

    /// Test suite for SOLVER.md Section 8.2.C: The Void Exception
    ///
    /// TypeScript allows `() => void` to match `() => T` for any T because
    /// the caller promises to ignore the return value. This is an intentional
    /// unsoundness for practical callback compatibility.
    ///
    /// See: https://github.com/microsoft/TypeScript/issues/25274
    #[test]
    fn test_void_return_exception_string() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        // () => void
        let func_returns_void = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });

        // () => string
        let func_returns_string = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: TypeId::STRING,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });

        // The void return exception: () => string is assignable to () => void
        // This allows callbacks that return values to be used where the return is ignored
        assert!(
            checker.is_assignable(func_returns_string, func_returns_void),
            "void return exception: () => string should be assignable to () => void"
        );
    }

    #[test]
    fn test_void_return_exception_number() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        // () => void
        let func_returns_void = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });

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

        // () => number should be assignable to () => void
        assert!(
            checker.is_assignable(func_returns_number, func_returns_void),
            "void return exception: () => number should be assignable to () => void"
        );
    }

    #[test]
    fn test_void_return_exception_is_one_way() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        // () => void
        let func_returns_void = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });

        // () => string
        let func_returns_string = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: TypeId::STRING,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });

        // The void exception is one-way: () => void is NOT assignable to () => string
        // You can't use a function that returns void where a string return is expected
        assert!(
            !checker.is_assignable(func_returns_void, func_returns_string),
            "void return exception is one-way: () => void should NOT be assignable to () => string"
        );
    }

    #[test]
    fn test_void_return_exception_with_parameters() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        // (x: number) => void
        let callback_void = interner.function(FunctionShape {
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

        // (x: number) => string
        let callback_string = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::STRING,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });

        // Void return exception works with matching parameters
        assert!(
            checker.is_assignable(callback_string, callback_void),
            "void return exception should work with matching parameters"
        );
    }

    #[test]
    fn test_void_return_exception_with_object_return() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        // () => void
        let func_returns_void = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });

        // () => { name: string }
        let obj_type = interner.object(vec![PropertyInfo {
            name: interner.intern_string("name"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }]);

        let func_returns_object = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: obj_type,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });

        // () => { name: string } should be assignable to () => void
        assert!(
            checker.is_assignable(func_returns_object, func_returns_void),
            "void return exception should work with object return types"
        );
    }

    /// Test suite for SOLVER.md Section 8.2.A: Function Variance
    ///
    /// TypeScript supports two modes for function parameter checking:
    /// - Contravariant (strict): Target param must be subtype of source param
    /// - Bivariant (legacy): Either direction is allowed
    #[test]
    fn test_function_strict_contravariance_animal_cat_example() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        // Enable strict function types (contravariant parameters)
        checker.set_strict_function_types(true);

        // Create Animal and Cat types (Cat <: Animal)
        let animal_type = interner.object(vec![PropertyInfo {
            name: interner.intern_string("name"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }]);

        let cat_type = interner.object(vec![
            PropertyInfo {
                name: interner.intern_string("name"),
                type_id: TypeId::STRING,
                write_type: TypeId::STRING,
                optional: false,
                readonly: false,
                is_method: false,
            },
            PropertyInfo {
                name: interner.intern_string("meow"),
                type_id: TypeId::BOOLEAN,
                write_type: TypeId::BOOLEAN,
                optional: false,
                readonly: false,
                is_method: false,
            },
        ]);

        // Verify Cat <: Animal (has all Animal's properties plus more)
        assert!(checker.is_assignable(cat_type, animal_type));

        // (x: Animal) => void
        let handler_animal = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: animal_type,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });

        // (x: Cat) => void
        let handler_cat = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: cat_type,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });

        // In strict mode (contravariant):
        // handler_animal IS assignable to handler_cat
        // Because if you expect a Cat handler, an Animal handler is safer
        // (it can handle any Cat since Cat is an Animal)
        assert!(
            checker.is_assignable(handler_animal, handler_cat),
            "Contravariant: (Animal) => void should be assignable to (Cat) => void"
        );

        // handler_cat is NOT assignable to handler_animal
        // Because if you expect an Animal handler, a Cat handler is unsafe
        // (it might try to call cat-specific methods on a Dog)
        assert!(
            !checker.is_assignable(handler_cat, handler_animal),
            "Contravariant: (Cat) => void should NOT be assignable to (Animal) => void"
        );
    }

    #[test]
    fn test_function_bivariant_legacy_mode() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        // Disable strict function types (bivariant parameters - legacy mode)
        checker.set_strict_function_types(false);

        // Create Animal and Cat types (Cat <: Animal)
        let animal_type = interner.object(vec![PropertyInfo {
            name: interner.intern_string("name"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }]);

        let cat_type = interner.object(vec![
            PropertyInfo {
                name: interner.intern_string("name"),
                type_id: TypeId::STRING,
                write_type: TypeId::STRING,
                optional: false,
                readonly: false,
                is_method: false,
            },
            PropertyInfo {
                name: interner.intern_string("meow"),
                type_id: TypeId::BOOLEAN,
                write_type: TypeId::BOOLEAN,
                optional: false,
                readonly: false,
                is_method: false,
            },
        ]);

        // (x: Animal) => void
        let handler_animal = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: animal_type,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });

        // (x: Cat) => void
        let handler_cat = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: cat_type,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });

        // In bivariant mode (legacy):
        // Both directions should be allowed
        assert!(
            checker.is_assignable(handler_animal, handler_cat),
            "Bivariant: (Animal) => void should be assignable to (Cat) => void"
        );
        assert!(
            checker.is_assignable(handler_cat, handler_animal),
            "Bivariant: (Cat) => void should be assignable to (Animal) => void"
        );
    }
}

/// Test suite for full pipeline integration: CompatChecker -> Lawyer -> SubtypeChecker
///
/// These tests verify that the full type checking pipeline works correctly,
/// ensuring that the Lawyer layer properly mediates between the CompatChecker
/// and the SubtypeChecker for tuple-to-array and other coercion scenarios.
#[cfg(test)]
mod full_pipeline_integration_tests {
    use super::*;

    /// Test that tuple-to-array goes through the lawyer layer correctly
    #[test]
    fn test_pipeline_tuple_to_array_homogeneous() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        // [number, number, number] - homogeneous tuple
        let num_tuple = interner.tuple(vec![
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
            TupleElement {
                type_id: TypeId::NUMBER,
                name: None,
                optional: false,
                rest: false,
            },
        ]);

        // number[]
        let num_array = interner.array(TypeId::NUMBER);

        // Homogeneous tuple should be assignable to array of same type
        assert!(
            checker.is_assignable(num_tuple, num_array),
            "[number, number, number] should be assignable to number[]"
        );
    }

    /// Test that heterogeneous tuple assignability to array is properly checked
    #[test]
    fn test_pipeline_tuple_to_array_heterogeneous() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        // [string, number, boolean]
        let hetero_tuple = interner.tuple(vec![
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

        // string[]
        let string_array = interner.array(TypeId::STRING);

        // Heterogeneous tuple should NOT be assignable to string[]
        assert!(
            !checker.is_assignable(hetero_tuple, string_array),
            "[string, number, boolean] should NOT be assignable to string[]"
        );

        // (string | number | boolean)[]
        let union_array =
            interner.array(interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]));

        // Should be assignable to union array containing all element types
        assert!(
            checker.is_assignable(hetero_tuple, union_array),
            "[string, number, boolean] should be assignable to (string | number | boolean)[]"
        );
    }

    /// Test the lawyer layer's handling of any in tuple-to-array scenarios
    #[test]
    fn test_pipeline_tuple_to_array_with_any() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

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

        // any[]
        let any_array = interner.array(TypeId::ANY);

        // Tuple containing any should be assignable to any[]
        assert!(
            checker.is_assignable(tuple_with_any, any_array),
            "[any, number] should be assignable to any[]"
        );

        // number[]
        let number_array = interner.array(TypeId::NUMBER);

        // [any, number] should be assignable to number[] (any is assignable to everything)
        assert!(
            checker.is_assignable(tuple_with_any, number_array),
            "[any, number] should be assignable to number[]"
        );
    }

    /// Test compat layer handling of empty tuples
    #[test]
    fn test_pipeline_empty_tuple_to_array() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        // [] - empty tuple
        let empty_tuple = interner.tuple(vec![]);

        // number[]
        let number_array = interner.array(TypeId::NUMBER);

        // Empty tuple should be assignable to any array type
        assert!(
            checker.is_assignable(empty_tuple, number_array),
            "[] should be assignable to number[]"
        );

        // string[]
        let string_array = interner.array(TypeId::STRING);

        // Empty tuple should be assignable to any array type
        assert!(
            checker.is_assignable(empty_tuple, string_array),
            "[] should be assignable to string[]"
        );
    }

    /// Test the pipeline with tuple containing optional elements
    #[test]
    fn test_pipeline_tuple_with_optional_to_array() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        // [string, number?] - tuple with optional element
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

        // (string | number | undefined)[]
        let union_array =
            interner.array(interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::UNDEFINED]));

        // Tuple with optional should be assignable to array with union including undefined
        assert!(
            checker.is_assignable(tuple_with_optional, union_array),
            "[string, number?] should be assignable to (string | number | undefined)[]"
        );
    }

    /// Test unknown type in tuple-to-array scenario
    #[test]
    fn test_pipeline_tuple_with_unknown_to_array() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        // [unknown, unknown]
        let unknown_tuple = interner.tuple(vec![
            TupleElement {
                type_id: TypeId::UNKNOWN,
                name: None,
                optional: false,
                rest: false,
            },
            TupleElement {
                type_id: TypeId::UNKNOWN,
                name: None,
                optional: false,
                rest: false,
            },
        ]);

        // unknown[]
        let unknown_array = interner.array(TypeId::UNKNOWN);

        // [unknown, unknown] should be assignable to unknown[]
        assert!(
            checker.is_assignable(unknown_tuple, unknown_array),
            "[unknown, unknown] should be assignable to unknown[]"
        );

        // string[]
        let string_array = interner.array(TypeId::STRING);

        // [unknown, unknown] should NOT be assignable to string[]
        assert!(
            !checker.is_assignable(unknown_tuple, string_array),
            "[unknown, unknown] should NOT be assignable to string[]"
        );
    }
}

/// TypeScript parity tests for tuple/array coercion in function parameters
///
/// These tests verify that our implementation matches TypeScript's behavior
/// for common tuple/array assignment patterns in function calls.
#[cfg(test)]
mod typescript_parity_tuple_array_tests {
    use super::*;

    /// TypeScript: function foo(arr: string[]): void {}
    ///             foo(["a", "b"]);  // OK
    #[test]
    fn test_ts_parity_tuple_literal_to_array_param() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        // ["a", "b"] - tuple of string literals
        let tuple_literal = interner.tuple(vec![
            TupleElement {
                type_id: interner.intern(TypeKey::Literal(LiteralValue::String(
                    interner.intern_string("a"),
                ))),
                name: None,
                optional: false,
                rest: false,
            },
            TupleElement {
                type_id: interner.intern(TypeKey::Literal(LiteralValue::String(
                    interner.intern_string("b"),
                ))),
                name: None,
                optional: false,
                rest: false,
            },
        ]);

        // string[]
        let string_array = interner.array(TypeId::STRING);

        // TypeScript allows this - tuple of string literals is assignable to string[]
        assert!(
            checker.is_assignable(tuple_literal, string_array),
            "TypeScript parity: tuple literal [\"a\", \"b\"] should be assignable to string[]"
        );
    }

    /// TypeScript: function foo(arr: number[]): void {}
    ///             foo([1, "a"]);  // Error!
    #[test]
    fn test_ts_parity_mixed_tuple_to_array_param_fails() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        // [1, "a"] - mixed tuple
        let mixed_tuple = interner.tuple(vec![
            TupleElement {
                type_id: interner.intern(TypeKey::Literal(LiteralValue::Number(OrderedFloat(1.0)))),
                name: None,
                optional: false,
                rest: false,
            },
            TupleElement {
                type_id: interner.intern(TypeKey::Literal(LiteralValue::String(
                    interner.intern_string("a"),
                ))),
                name: None,
                optional: false,
                rest: false,
            },
        ]);

        // number[]
        let number_array = interner.array(TypeId::NUMBER);

        // TypeScript rejects this - string is not assignable to number
        assert!(
            !checker.is_assignable(mixed_tuple, number_array),
            "TypeScript parity: tuple [1, \"a\"] should NOT be assignable to number[]"
        );
    }

    /// TypeScript: function process(data: (string | number)[]): void {}
    ///             process([1, "two", 3]);  // OK
    #[test]
    fn test_ts_parity_mixed_tuple_to_union_array() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        // [1, "two", 3]
        let mixed_tuple = interner.tuple(vec![
            TupleElement {
                type_id: interner.intern(TypeKey::Literal(LiteralValue::Number(OrderedFloat(1.0)))),
                name: None,
                optional: false,
                rest: false,
            },
            TupleElement {
                type_id: interner.intern(TypeKey::Literal(LiteralValue::String(
                    interner.intern_string("two"),
                ))),
                name: None,
                optional: false,
                rest: false,
            },
            TupleElement {
                type_id: interner.intern(TypeKey::Literal(LiteralValue::Number(OrderedFloat(3.0)))),
                name: None,
                optional: false,
                rest: false,
            },
        ]);

        // (string | number)[]
        let union_array = interner.array(interner.union(vec![TypeId::STRING, TypeId::NUMBER]));

        // TypeScript allows this
        assert!(
            checker.is_assignable(mixed_tuple, union_array),
            "TypeScript parity: [1, \"two\", 3] should be assignable to (string | number)[]"
        );
    }

    /// TypeScript: Function parameter contravariance with tuple/array
    ///
    /// In TypeScript with strictFunctionTypes:
    /// - `(items: string[]) => void` IS assignable to `(items: [string, string]) => void`
    ///   because contravariance checks that the target param is assignable to source param:
    ///   [string, string] <: string[] (true - tuple is assignable to array)
    /// - `(items: [string, string]) => void` is NOT assignable to `(items: string[]) => void`
    ///   because contravariance checks: string[] <: [string, string] - FALSE
    ///   (array is NOT assignable to tuple with fixed length)
    #[test]
    fn test_ts_parity_function_param_tuple_array_strictness() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);
        checker.set_strict_function_types(true);

        // (items: string[]) => void
        let callback_array_param = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("items")),
                type_id: interner.array(TypeId::STRING),
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });

        // [string, string]
        let tuple_type = interner.tuple(vec![
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

        // (items: [string, string]) => void
        let callback_tuple_param = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("items")),
                type_id: tuple_type,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });

        // In strict contravariance, we check: target param <: source param
        // For (array) => void assignable to (tuple) => void:
        //   Check: tuple <: array (i.e., [string, string] <: string[]) - TRUE
        // So this SHOULD be assignable
        assert!(
            checker.is_assignable(callback_array_param, callback_tuple_param),
            "TypeScript parity: (items: string[]) => void SHOULD be assignable to (items: [string, string]) => void (contravariance)"
        );

        // For (tuple) => void assignable to (array) => void:
        //   Check: array <: tuple (i.e., string[] <: [string, string]) - FALSE
        // Array is NOT a subtype of tuple (arrays don't have fixed length guarantee)
        // So this should NOT be assignable
        assert!(
            !checker.is_assignable(callback_tuple_param, callback_array_param),
            "TypeScript parity: (items: [string, string]) => void should NOT be assignable to (items: string[]) => void (contravariance)"
        );
    }

    /// TypeScript: const arr: readonly string[] = ["a", "b"];
    ///             arr.push("c");  // Error - push doesn't exist on readonly
    #[test]
    fn test_ts_parity_tuple_to_readonly_array() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        // ["a", "b"]
        let tuple_literal = interner.tuple(vec![
            TupleElement {
                type_id: interner.intern(TypeKey::Literal(LiteralValue::String(
                    interner.intern_string("a"),
                ))),
                name: None,
                optional: false,
                rest: false,
            },
            TupleElement {
                type_id: interner.intern(TypeKey::Literal(LiteralValue::String(
                    interner.intern_string("b"),
                ))),
                name: None,
                optional: false,
                rest: false,
            },
        ]);

        // readonly string[]
        let readonly_string_array =
            interner.intern(TypeKey::ReadonlyType(interner.array(TypeId::STRING)));

        // Tuple should be assignable to readonly array
        assert!(
            checker.is_assignable(tuple_literal, readonly_string_array),
            "TypeScript parity: [\"a\", \"b\"] should be assignable to readonly string[]"
        );
    }

    /// TypeScript: const point: [number, number] = [1, 2];
    ///             const arr: number[] = point;  // OK
    #[test]
    fn test_ts_parity_named_tuple_to_array() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        // [x: number, y: number] - named tuple
        let point_tuple = interner.tuple(vec![
            TupleElement {
                type_id: TypeId::NUMBER,
                name: Some(interner.intern_string("x")),
                optional: false,
                rest: false,
            },
            TupleElement {
                type_id: TypeId::NUMBER,
                name: Some(interner.intern_string("y")),
                optional: false,
                rest: false,
            },
        ]);

        // number[]
        let number_array = interner.array(TypeId::NUMBER);

        // Named tuple should be assignable to array
        assert!(
            checker.is_assignable(point_tuple, number_array),
            "TypeScript parity: [x: number, y: number] should be assignable to number[]"
        );
    }

    /// TypeScript: function spread(...args: string[]): void {}
    ///             spread(...["a", "b"] as const);  // OK
    #[test]
    fn test_ts_parity_spread_tuple_to_rest_param() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        // [string, string]
        let tuple_type = interner.tuple(vec![
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

        // string[] (what rest param becomes)
        let string_array = interner.array(TypeId::STRING);

        // Tuple should be compatible with array (for spread purposes)
        assert!(
            checker.is_assignable(tuple_type, string_array),
            "TypeScript parity: [string, string] should be assignable to string[] (for spread)"
        );
    }
}
