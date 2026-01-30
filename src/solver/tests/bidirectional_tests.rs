//! Tests for bidirectional type inference and contextual typing
//!
//! These tests verify that contextual types are used properly for type inference
//! in array literals, object literals, return statements, and arrow functions.

#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::solver::{FunctionShape, ParamInfo, PropertyInfo, TupleElement};

    #[test]
    fn test_apply_contextual_literal_to_union() {
        let interner = TypeInterner::new();
        let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
        let literal = interner.literal_string("test");

        // When we have a literal expression and the context is a union,
        // the literal should be preserved (it's more specific than the union)
        let result = apply_contextual_type(&interner, literal, Some(union));
        assert_eq!(result, literal);
    }

    #[test]
    fn test_apply_contextual_unknown_uses_context() {
        let interner = TypeInterner::new();
        let expected = TypeId::NUMBER;

        // When the expression type is unknown, use the contextual type
        let result = apply_contextual_type(&interner, TypeId::UNKNOWN, Some(expected));
        assert_eq!(result, expected);
    }

    #[test]
    fn test_apply_contextual_subtype_preserved() {
        let interner = TypeInterner::new();
        let literal = interner.literal_string("test");

        // When the expression type is a subtype of the contextual type,
        // the expression type should be preserved (it's more specific)
        let result = apply_contextual_type(&interner, literal, Some(TypeId::STRING));
        assert_eq!(result, literal);
    }

    #[test]
    fn test_apply_contextual_subtype_to_base() {
        let interner = TypeInterner::new();

        // Create a literal type and verify it's a subtype of its base type
        let literal = interner.literal_string("test");
        let result = apply_contextual_type(&interner, literal, Some(TypeId::STRING));

        // Literal should be preserved when context is the base type
        assert_eq!(result, literal);
    }

    #[test]
    fn test_contextual_array_element_type() {
        let interner = TypeInterner::new();

        // number[] has element type number
        let number_array = interner.array(TypeId::NUMBER);
        let ctx = ContextualTypeContext::with_expected(&interner, number_array);

        assert_eq!(ctx.get_array_element_type(), Some(TypeId::NUMBER));
    }

    #[test]
    fn test_contextual_tuple_element_type() {
        let interner = TypeInterner::new();

        // [string, number] is a tuple
        let tuple = interner.tuple(vec![
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
        let ctx = ContextualTypeContext::with_expected(&interner, tuple);

        assert_eq!(ctx.get_tuple_element_type(0), Some(TypeId::STRING));
        assert_eq!(ctx.get_tuple_element_type(1), Some(TypeId::NUMBER));
    }

    #[test]
    fn test_contextual_property_type() {
        let interner = TypeInterner::new();

        // { x: number, y: string }
        let obj = interner.object(vec![
            PropertyInfo {
                name: interner.intern_string("x"),
                type_id: TypeId::NUMBER,
                write_type: TypeId::NUMBER,
                optional: false,
                readonly: false,
                is_method: false,
            },
            PropertyInfo {
                name: interner.intern_string("y"),
                type_id: TypeId::STRING,
                write_type: TypeId::STRING,
                optional: false,
                readonly: false,
                is_method: false,
            },
        ]);
        let ctx = ContextualTypeContext::with_expected(&interner, obj);

        assert_eq!(ctx.get_property_type("x"), Some(TypeId::NUMBER));
        assert_eq!(ctx.get_property_type("y"), Some(TypeId::STRING));
    }

    #[test]
    fn test_contextual_function_return_type() {
        let interner = TypeInterner::new();

        // () => string has return type string
        let fn_type = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: TypeId::STRING,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });
        let ctx = ContextualTypeContext::with_expected(&interner, fn_type);

        assert_eq!(ctx.get_return_type(), Some(TypeId::STRING));
    }

    #[test]
    fn test_contextual_function_parameter_type() {
        let interner = TypeInterner::new();

        // (x: number) => void has first parameter type number
        let fn_type = interner.function(FunctionShape {
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
        let ctx = ContextualTypeContext::with_expected(&interner, fn_type);

        assert_eq!(ctx.get_parameter_type(0), Some(TypeId::NUMBER));
    }

    #[test]
    fn test_contextual_for_property_child_context() {
        let interner = TypeInterner::new();

        // { nested: { value: number } }
        let inner = interner.object(vec![PropertyInfo {
            name: interner.intern_string("value"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        }]);
        let outer = interner.object(vec![PropertyInfo {
            name: interner.intern_string("nested"),
            type_id: inner,
            write_type: inner,
            optional: false,
            readonly: false,
            is_method: false,
        }]);

        let ctx = ContextualTypeContext::with_expected(&interner, outer);
        let nested_ctx = ctx.for_property("nested");

        // Child context should have the inner object type as expected
        assert!(nested_ctx.has_context());
        assert_eq!(nested_ctx.get_property_type("value"), Some(TypeId::NUMBER));
    }

    #[test]
    fn test_contextual_for_array_element_child_context() {
        let interner = TypeInterner::new();

        // number[]
        let number_array = interner.array(TypeId::NUMBER);
        let ctx = ContextualTypeContext::with_expected(&interner, number_array);
        let elem_ctx = ctx.for_array_element();

        // Child context should have number as expected type
        assert!(elem_ctx.has_context());
        assert_eq!(elem_ctx.expected(), Some(TypeId::NUMBER));
    }

    #[test]
    fn test_contextual_for_parameter_child_context() {
        let interner = TypeInterner::new();

        // (x: string) => void
        let fn_type = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("x")),
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

        let ctx = ContextualTypeContext::with_expected(&interner, fn_type);
        let param_ctx = ctx.for_parameter(0);

        // Child context should have string as expected type
        assert!(param_ctx.has_context());
        assert_eq!(param_ctx.expected(), Some(TypeId::STRING));
    }

    #[test]
    fn test_contextual_for_return_child_context() {
        let interner = TypeInterner::new();

        // () => number
        let fn_type = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: TypeId::NUMBER,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });

        let ctx = ContextualTypeContext::with_expected(&interner, fn_type);
        let return_ctx = ctx.for_return();

        // Child context should have number as expected type
        assert!(return_ctx.has_context());
        assert_eq!(return_ctx.expected(), Some(TypeId::NUMBER));
    }
}
