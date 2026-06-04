#[test]
fn test_symbol_property_access_methods() {
    use tsz_solver::operations::property::{PropertyAccessEvaluator, PropertyAccessResult};

    // Test accessing methods on symbol type
    let types = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&types);

    // toString and valueOf should use the symbol apparent method return types.
    let result_to_string = evaluator.resolve_property_access(TypeId::SYMBOL, "toString");
    match result_to_string {
        PropertyAccessResult::Success {
            type_id: prop_type, ..
        } => {
            let Some(TypeData::Function(shape_id)) = types.lookup(prop_type) else {
                panic!("Expected symbol.toString to resolve to function type");
            };
            let shape = types.function_shape(shape_id);
            assert_eq!(shape.return_type, TypeId::STRING);
        }
        _ => panic!("Expected Success for symbol.toString, got: {result_to_string:?}"),
    }

    let result_value_of = evaluator.resolve_property_access(TypeId::SYMBOL, "valueOf");
    match result_value_of {
        PropertyAccessResult::Success {
            type_id: prop_type, ..
        } => {
            let Some(TypeData::Function(shape_id)) = types.lookup(prop_type) else {
                panic!("Expected symbol.valueOf to resolve to function type");
            };
            let shape = types.function_shape(shape_id);
            assert_eq!(shape.return_type, TypeId::SYMBOL);
        }
        _ => panic!("Expected Success for symbol.valueOf, got: {result_value_of:?}"),
    }
}

#[test]
fn test_symbol_property_not_found() {
    use tsz_solver::operations::property::{PropertyAccessEvaluator, PropertyAccessResult};

    // Test accessing non-existent property on symbol type
    let types = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&types);
    let name_atom = types.intern_string("nonexistent");

    let result = evaluator.resolve_property_access(TypeId::SYMBOL, "nonexistent");
    match result {
        PropertyAccessResult::PropertyNotFound {
            type_id,
            property_name,
        } => {
            assert_eq!(type_id, TypeId::SYMBOL);
            assert_eq!(property_name, name_atom);
        }
        _ => panic!("Expected PropertyNotFound for unknown property, got: {result:?}"),
    }
}

#[test]
fn test_property_access_from_index_signature_4111() {
    let source = r#"
interface StringMap {
    [key: string]: number;
}
const obj: StringMap = {} as any;
const val = obj.someProperty;
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let opts = crate::checker::context::CheckerOptions {
        jsx_factory: "React.createElement".to_string(),
        jsx_fragment_factory: "React.Fragment".to_string(),
        no_property_access_from_index_signature: true,
        ..Default::default()
    };
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        opts,
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&4111),
        "Expected error 4111 for property access from index signature, got: {codes:?}"
    );
}

#[test]
fn test_explicit_property_no_error_4111() {
    let source = r#"
interface MixedType {
    explicitProp: string;
    [key: string]: string | number;
}
const obj: MixedType = {} as any;
const val = obj.explicitProp;
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let opts = crate::checker::context::CheckerOptions {
        jsx_factory: "React.createElement".to_string(),
        jsx_fragment_factory: "React.Fragment".to_string(),
        no_property_access_from_index_signature: true,
        ..Default::default()
    };
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        opts,
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&4111),
        "Should not have error 4111 for explicit property"
    );
}

/// TODO: Property access from index signature on mixed unions incorrectly emits TS4111.
/// When a union has one member with an explicit property and another with an index
/// signature, tsc does NOT emit TS4111 for the explicit property. Currently we do emit it.
/// When this is fixed, update to assert !codes.contains(&4111).
#[test]
fn test_union_with_index_signature_4111() {
    let source = r#"
type Mixed = { x: number } | { [key: string]: number };
const obj: Mixed = {} as any;
const val = obj.x;
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let opts = crate::checker::context::CheckerOptions {
        jsx_factory: "React.createElement".to_string(),
        jsx_fragment_factory: "React.Fragment".to_string(),
        no_property_access_from_index_signature: true,
        ..Default::default()
    };
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        opts,
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    // Should NOT emit 4111 when union member has explicit property 'x'.
    // Previously incorrectly emitted TS4111 for mixed union with index signature;
    // fixed by preserving union index access diagnostics.
    assert!(
        !codes.contains(&4111),
        "Expected no TS4111 when union member has explicit property 'x', got: {codes:?}"
    );
}

#[test]
fn test_checker_lowers_full_source_file() {
    use tsz_solver::TypeData;

    let source = r#"
interface Foo { x: number; }
type Bar = Foo | string;
type Baz = [string, number];
type Qux = { [key: string]: Foo };
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );

    let foo_sym = binder.file_locals.get("Foo").expect("Foo should exist");
    let bar_sym = binder.file_locals.get("Bar").expect("Bar should exist");
    let baz_sym = binder.file_locals.get("Baz").expect("Baz should exist");
    let qux_sym = binder.file_locals.get("Qux").expect("Qux should exist");

    let foo_type = checker.get_type_of_symbol(foo_sym);
    let foo_key = types.lookup(foo_type).expect("Foo type should exist");
    match foo_key {
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            let shape = types.object_shape(shape_id);
            let prop = shape
                .properties
                .iter()
                .find(|prop| types.resolve_atom(prop.name) == "x")
                .expect("Expected property x");
            assert_eq!(prop.type_id, TypeId::NUMBER);
        }
        _ => panic!("Expected Foo to be Object type, got {foo_key:?}"),
    }

    let bar_type = checker.get_type_of_symbol(bar_sym);
    let bar_key = types.lookup(bar_type).expect("Bar type should exist");
    match bar_key {
        TypeData::Union(members) => {
            let members = types.type_list(members);
            assert_eq!(members.len(), 2);
            assert!(members.contains(&TypeId::STRING));
            // The non-string member may be a lazy type reference to Foo
            // (TypeData::Lazy) or the resolved Object type. Either is valid.
            let non_string_member = *members.iter().find(|&&m| m != TypeId::STRING).unwrap();
            if non_string_member != foo_type {
                // If not the same TypeId, verify it's a lazy reference (unevaluated Foo)
                let member_key = types
                    .lookup(non_string_member)
                    .expect("member type should exist");
                assert!(
                    matches!(member_key, TypeData::Lazy(_)),
                    "Non-string member should be foo_type or a Lazy reference, got {member_key:?}"
                );
            }
        }
        _ => panic!("Expected Bar to be Union type, got {bar_key:?}"),
    }

    let baz_type = checker.get_type_of_symbol(baz_sym);
    let baz_key = types.lookup(baz_type).expect("Baz type should exist");
    match baz_key {
        TypeData::Tuple(elements) => {
            let elements = types.tuple_list(elements);
            assert_eq!(elements.len(), 2);
            assert_eq!(elements[0].type_id, TypeId::STRING);
            assert_eq!(elements[1].type_id, TypeId::NUMBER);
        }
        _ => panic!("Expected Baz to be Tuple type, got {baz_key:?}"),
    }

    let qux_type = checker.get_type_of_symbol(qux_sym);
    let qux_key = types.lookup(qux_type).expect("Qux type should exist");
    match qux_key {
        TypeData::ObjectWithIndex(shape_id) => {
            let shape = types.object_shape(shape_id);
            let string_index = shape
                .string_index
                .as_ref()
                .expect("Expected string index signature");
            assert_eq!(string_index.key_type, TypeId::STRING);
            let value_key = types
                .lookup(string_index.value_type)
                .expect("Index value type should exist");
            match value_key {
                TypeData::Lazy(_def_id) => {} // Phase 4.2: Now uses Lazy(DefId) instead of Ref(SymbolRef)
                _ => panic!("Expected Foo lazy type, got {value_key:?}"),
            }
        }
        _ => panic!("Expected Qux to be ObjectWithIndex type, got {qux_key:?}"),
    }
}
