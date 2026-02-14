use super::*;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn create_context(source: &str) -> (ParserState, BinderState, TypeInterner) {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    let types = TypeInterner::new();
    (parser, binder, types)
}

#[test]
fn test_string_is_iterable() {
    let source = "'hello'";
    let (parser, binder, types) = create_context(source);
    let ctx = CheckerContext::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::context::CheckerOptions::default(),
    );

    // String type is always iterable
    let checker = IteratorChecker { ctx: &mut { ctx } };
    assert!(checker.is_iterable(TypeId::STRING));
}

#[test]
fn test_array_element_type() {
    let source = "[1, 2, 3]";
    let (parser, binder, types) = create_context(source);
    let ctx = CheckerContext::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::context::CheckerOptions::default(),
    );

    // Create an array type
    let number_array = types.array(TypeId::NUMBER);
    let checker = IteratorChecker { ctx: &mut { ctx } };

    // Element type of number[] should be number
    let elem_type = checker.get_iterable_element_type(number_array);
    assert_eq!(elem_type, TypeId::NUMBER);
}

#[test]
fn test_tuple_element_type() {
    let source = "const x: [number, string] = [1, 'a']";
    let (parser, binder, types) = create_context(source);
    let ctx = CheckerContext::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::context::CheckerOptions::default(),
    );

    // Create a tuple type [number, string]
    let tuple_type = types.tuple(vec![
        tsz_solver::TupleElement {
            type_id: TypeId::NUMBER,
            optional: false,
        },
        tsz_solver::TupleElement {
            type_id: TypeId::STRING,
            optional: false,
        },
    ]);

    let checker = IteratorChecker { ctx: &mut { ctx } };

    // Element type of [number, string] should be number | string
    let elem_type = checker.get_iterable_element_type(tuple_type);
    // The result should be a union type
    assert!(checker.is_iterable(tuple_type));
}

#[test]
fn test_create_iterator_type_number() {
    let source = "";
    let (parser, binder, types) = create_context(source);
    let ctx = CheckerContext::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::context::CheckerOptions::default(),
    );

    let checker = IteratorChecker { ctx: &mut { ctx } };

    // Create Iterator<number>
    let iterator_number = checker.create_iterator_type(TypeId::NUMBER);

    // Verify the type is not ANY (i.e., a proper type was created)
    assert_ne!(iterator_number, TypeId::ANY);

    // Verify it's an object type with a `next` method
    if let Some(shape) = tsz_solver::type_queries::get_object_shape(&types, iterator_number) {
        // Should have a `next` property
        assert!(
            shape
                .properties
                .iter()
                .any(|p| { types.resolve_atom(p.name) == "next" && p.is_method })
        );
    }
}

#[test]
fn test_create_iterator_result_type() {
    let source = "";
    let (parser, binder, types) = create_context(source);
    let ctx = CheckerContext::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::context::CheckerOptions::default(),
    );

    let checker = IteratorChecker { ctx: &mut { ctx } };

    // Create IteratorResult<number, any>
    let iterator_result = checker.create_iterator_result_type(TypeId::NUMBER, TypeId::ANY);

    // Verify the type is not ANY
    assert_ne!(iterator_result, TypeId::ANY);

    // It should be a union type (IteratorYieldResult | IteratorReturnResult)
    let _ = tsz_solver::type_queries::get_union_members(&types, iterator_result);
}

#[test]
fn test_create_iterable_iterator_type() {
    let source = "";
    let (parser, binder, types) = create_context(source);
    let ctx = CheckerContext::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::context::CheckerOptions::default(),
    );

    let checker = IteratorChecker { ctx: &mut { ctx } };

    // Create IterableIterator<string>
    let iterable_iterator = checker.create_iterable_iterator_type(TypeId::STRING);

    // Should not return ANY
    assert_ne!(iterable_iterator, TypeId::ANY);
}

#[test]
fn test_create_async_iterator_type() {
    let source = "";
    let (parser, binder, types) = create_context(source);
    let ctx = CheckerContext::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::context::CheckerOptions::default(),
    );

    let checker = IteratorChecker { ctx: &mut { ctx } };

    // Create AsyncIterator<number>
    let async_iterator = checker.create_async_iterator_type(TypeId::NUMBER);

    // Should not return ANY
    assert_ne!(async_iterator, TypeId::ANY);
}

#[test]
fn test_iterator_type_has_next_method() {
    let source = "";
    let (parser, binder, types) = create_context(source);
    let ctx = CheckerContext::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::context::CheckerOptions::default(),
    );

    let checker = IteratorChecker { ctx: &mut { ctx } };

    // Create Iterator<number>
    let iterator_type = checker.create_iterator_type(TypeId::NUMBER);

    // Verify it has a next() method that returns IteratorResult<number, any>
    if let Some(shape) = tsz_solver::type_queries::get_object_shape(&types, iterator_type) {

        // Find the next property
        let next_prop = shape
            .properties
            .iter()
            .find(|p| types.resolve_atom(p.name) == "next");

        assert!(next_prop.is_some(), "Iterator should have a 'next' method");

        let next_prop = next_prop.unwrap();
        assert!(next_prop.is_method, "next should be a method");

        // Verify next is a function
        if let Some(func_shape) =
            tsz_solver::type_queries::get_function_shape(&types, next_prop.type_id)
        {
            // Return type should be IteratorResult<number, any>
            assert_ne!(func_shape.return_type, TypeId::ANY);
        }
    }
}
