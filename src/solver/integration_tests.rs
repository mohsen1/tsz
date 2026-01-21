//! Integration tests for solver strictness improvements.
//!
//! This module tests the comprehensive solver improvements made in SOLV-15, SOLV-18, and SOLV-19:
//! - Generic type constraints (SOLV-15): Using constraints instead of falling back to Any
//! - Tuple type subtyping (SOLV-18): Covariant tuple subtyping with proper length handling
//! - Function type variance (SOLV-19): Proper contravariance for parameter types
//!
//! These integration tests verify TS2322 and TS7006 error detection improves with strictness.

use super::*;
use crate::solver::types::*;

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
    #[ignore = "Generic constraint usage in strict subtyping not fully implemented"]
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
    #[ignore = "Multiple generic constraint combination not fully implemented"]
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

        // string[] (readonly array)
        let array_type = interner.tuple(vec![TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: true,
        }]);

        // Tuple should NOT be assignable to array (different element types)
        assert!(!checker.is_assignable(tuple_type, array_type));
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

    #[test]
    #[ignore = "Strict any propagation not fully implemented"]
    fn test_strict_mode_any_does_not_suppress_errors() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);

        // Enable strict any propagation
        checker.set_strict_any_propagation(true);

        // Create object with 'any' property
        let obj_with_any = interner.object(vec![PropertyInfo {
            name: interner.intern_string("value"),
            type_id: TypeId::ANY,
            write_type: TypeId::ANY,
            optional: false,
            readonly: false,
            is_method: false,
        }]);

        // Create object with 'number' property
        let obj_with_number = interner.object(vec![PropertyInfo {
            name: interner.intern_string("value"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        }]);

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
        let obj_with_any = interner.object(vec![PropertyInfo {
            name: interner.intern_string("value"),
            type_id: TypeId::ANY,
            write_type: TypeId::ANY,
            optional: false,
            readonly: false,
            is_method: false,
        }]);

        // Create object with 'number' property
        let obj_with_number = interner.object(vec![PropertyInfo {
            name: interner.intern_string("value"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        }]);

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
    #[ignore = "Strict any propagation in complex structures not fully implemented"]
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
