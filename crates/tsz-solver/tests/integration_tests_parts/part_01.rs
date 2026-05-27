mod typescript_quirks_tests {
    use super::*;

    /// Test suite for SOLVER.md Section 8.2.C: The Void Exception
    ///
    /// TypeScript allows `() => void` to match `() => T` for any T because
    /// the caller promises to ignore the return value. This is an intentional
    /// unsoundness for practical callback compatibility.
    ///
    /// See: <https://github.com/microsoft/TypeScript/issues/25274>
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
        let obj_type = interner.object(vec![PropertyInfo::new(
            interner.intern_string("name"),
            TypeId::STRING,
        )]);

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
        let animal_type = interner.object(vec![PropertyInfo::new(
            interner.intern_string("name"),
            TypeId::STRING,
        )]);

        let cat_type = interner.object(vec![
            PropertyInfo::new(interner.intern_string("name"), TypeId::STRING),
            PropertyInfo::new(interner.intern_string("meow"), TypeId::BOOLEAN),
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
        let animal_type = interner.object(vec![PropertyInfo::new(
            interner.intern_string("name"),
            TypeId::STRING,
        )]);

        let cat_type = interner.object(vec![
            PropertyInfo::new(interner.intern_string("name"), TypeId::STRING),
            PropertyInfo::new(interner.intern_string("meow"), TypeId::BOOLEAN),
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
                type_id: interner.intern(TypeData::Literal(LiteralValue::String(
                    interner.intern_string("a"),
                ))),
                name: None,
                optional: false,
                rest: false,
            },
            TupleElement {
                type_id: interner.intern(TypeData::Literal(LiteralValue::String(
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
                type_id: interner
                    .intern(TypeData::Literal(LiteralValue::Number(OrderedFloat(1.0)))),
                name: None,
                optional: false,
                rest: false,
            },
            TupleElement {
                type_id: interner.intern(TypeData::Literal(LiteralValue::String(
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
                type_id: interner
                    .intern(TypeData::Literal(LiteralValue::Number(OrderedFloat(1.0)))),
                name: None,
                optional: false,
                rest: false,
            },
            TupleElement {
                type_id: interner.intern(TypeData::Literal(LiteralValue::String(
                    interner.intern_string("two"),
                ))),
                name: None,
                optional: false,
                rest: false,
            },
            TupleElement {
                type_id: interner
                    .intern(TypeData::Literal(LiteralValue::Number(OrderedFloat(3.0)))),
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
                type_id: interner.intern(TypeData::Literal(LiteralValue::String(
                    interner.intern_string("a"),
                ))),
                name: None,
                optional: false,
                rest: false,
            },
            TupleElement {
                type_id: interner.intern(TypeData::Literal(LiteralValue::String(
                    interner.intern_string("b"),
                ))),
                name: None,
                optional: false,
                rest: false,
            },
        ]);

        // readonly string[]
        let readonly_string_array =
            interner.intern(TypeData::ReadonlyType(interner.array(TypeId::STRING)));

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
mod homomorphic_mapped_type_tests {
    use super::*;

    /// Test that Pick<TP, keyof TP> is identical to TP for redeclaration checks.
    ///
    /// This is a full integration test that exercises the complete path:
    /// 1. Application evaluation (Pick generic instantiation)
    /// 2. `KeyOf` expansion in type arguments
    /// 3. Homomorphic mapped type detection (post-instantiation form)
    /// 4. Optional modifier preservation from source properties
    /// 5. Bidirectional subtype check for redeclaration identity
    #[test]
    fn test_pick_all_keys_identical_to_source() {
        let interner = TypeInterner::new();

        let key_a = interner.intern_string("a");
        let key_b = interner.intern_string("b");

        // TP = { a?: number, b?: string }
        let tp = interner.object(vec![
            PropertyInfo {
                name: key_a,
                type_id: TypeId::NUMBER,
                write_type: TypeId::NUMBER,
                optional: true,
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
            PropertyInfo {
                name: key_b,
                type_id: TypeId::STRING,
                write_type: TypeId::STRING,
                optional: true,
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

        // Construct Pick<TP, keyof TP> as a mapped type: { [P in keyof TP]: TP[P] }
        let keyof_tp = interner.keyof(tp);

        let key_param = TypeParamInfo {
            name: interner.intern_string("P"),
            constraint: Some(keyof_tp),
            default: None,
            is_const: false,
        };
        let key_param_id = interner.intern(TypeData::TypeParameter(key_param));
        let index_access = interner.intern(TypeData::IndexAccess(tp, key_param_id));

        let mapped = MappedType {
            type_param: key_param,
            constraint: keyof_tp,
            name_type: None,
            template: index_access,
            readonly_modifier: None,
            optional_modifier: None,
        };

        let pick_result = evaluate_mapped(&interner, &mapped);

        // Bidirectional subtype: TP ≡ Pick<TP, keyof TP>
        let mut checker = SubtypeChecker::new(&interner);
        assert!(
            checker.is_subtype_of(tp, pick_result),
            "TP should be subtype of Pick<TP, keyof TP>"
        );
        assert!(
            checker.is_subtype_of(pick_result, tp),
            "Pick<TP, keyof TP> should be subtype of TP"
        );
    }
}
