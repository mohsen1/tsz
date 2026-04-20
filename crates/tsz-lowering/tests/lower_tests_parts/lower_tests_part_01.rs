#[test]
fn test_lower_type_literal_call_signature_this_param() {
    let (arena, literal_idx) = parse_type_literal("type T = { (this: any, x: string): number; };");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(literal_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Callable(callable_id) => {
            let callable = interner.callable_shape(callable_id);
            assert_eq!(callable.call_signatures.len(), 1);
            let sig = &callable.call_signatures[0];
            assert_eq!(sig.this_type, Some(TypeId::ANY));
            assert_eq!(sig.params.len(), 1);
            assert_eq!(sig.params[0].type_id, TypeId::STRING);
        }
        _ => panic!("Expected Callable type, got {key:?}"),
    }
}
#[test]
fn test_lower_type_literal_call_signature_type_predicate() {
    let (arena, literal_idx) = parse_type_literal("type T = { (x: any): x is string; };");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(literal_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Callable(callable_id) => {
            let callable = interner.callable_shape(callable_id);
            assert_eq!(callable.call_signatures.len(), 1);
            let sig = &callable.call_signatures[0];
            assert_eq!(sig.return_type, TypeId::BOOLEAN);
            let predicate = sig
                .type_predicate
                .as_ref()
                .expect("Expected type predicate");
            assert!(!predicate.asserts);
            match predicate.target {
                TypePredicateTarget::Identifier(atom) => {
                    assert_eq!(interner.resolve_atom(atom).as_str(), "x");
                }
                _ => panic!("Expected identifier predicate target"),
            }
            assert_eq!(predicate.type_id, Some(TypeId::STRING));
        }
        _ => panic!("Expected Callable type, got {key:?}"),
    }
}
#[test]
fn test_lower_type_literal_call_signature_asserts_predicate_without_is() {
    let (arena, literal_idx) = parse_type_literal("type T = { (x: any): asserts x; };");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(literal_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Callable(callable_id) => {
            let callable = interner.callable_shape(callable_id);
            assert_eq!(callable.call_signatures.len(), 1);
            let sig = &callable.call_signatures[0];
            assert_eq!(sig.return_type, TypeId::VOID);
            let predicate = sig
                .type_predicate
                .as_ref()
                .expect("Expected type predicate");
            assert!(predicate.asserts);
            match predicate.target {
                TypePredicateTarget::Identifier(atom) => {
                    assert_eq!(interner.resolve_atom(atom).as_str(), "x");
                }
                _ => panic!("Expected identifier predicate target"),
            }
            assert_eq!(predicate.type_id, None);
        }
        _ => panic!("Expected Callable type, got {key:?}"),
    }
}
#[test]
fn test_lower_type_literal_overloaded_call_signatures() {
    let (arena, literal_idx) =
        parse_type_literal("type T = { (x: string): number; (x: number): string; };");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(literal_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Callable(callable_id) => {
            let callable = interner.callable_shape(callable_id);
            assert_eq!(callable.call_signatures.len(), 2);

            let first = &callable.call_signatures[0];
            assert_eq!(first.params.len(), 1);
            assert_eq!(first.params[0].type_id, TypeId::STRING);
            assert_eq!(first.return_type, TypeId::NUMBER);

            let second = &callable.call_signatures[1];
            assert_eq!(second.params.len(), 1);
            assert_eq!(second.params[0].type_id, TypeId::NUMBER);
            assert_eq!(second.return_type, TypeId::STRING);
        }
        _ => panic!("Expected Callable type, got {key:?}"),
    }
}
#[test]
fn test_lower_type_literal_construct_signature() {
    let (arena, literal_idx) = parse_type_literal("type T = { new (x: string): number; };");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(literal_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Callable(callable_id) => {
            let callable = interner.callable_shape(callable_id);
            assert_eq!(callable.call_signatures.len(), 0);
            assert_eq!(callable.construct_signatures.len(), 1);
        }
        _ => panic!("Expected Callable type, got {key:?}"),
    }
}
#[test]
fn test_lower_type_literal_index_signature() {
    let (arena, literal_idx) =
        parse_type_literal("type T = { [key: string]: number; foo: number; };");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(literal_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::ObjectWithIndex(shape_id) => {
            let shape = interner.object_shape(shape_id);
            assert_eq!(shape.properties.len(), 1);
            assert_eq!(interner.resolve_atom(shape.properties[0].name), "foo");
            let string_index = shape
                .string_index
                .as_ref()
                .expect("Expected string index signature");
            assert_eq!(string_index.key_type, TypeId::STRING);
            assert_eq!(string_index.value_type, TypeId::NUMBER);
        }
        _ => panic!("Expected ObjectWithIndex type, got {key:?}"),
    }
}
#[test]
fn test_lower_type_literal_index_signature_mismatch() {
    let (arena, literal_idx) =
        parse_type_literal("type T = { [key: string]: number; foo: string; };");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(literal_idx);
    assert_ne!(type_id, TypeId::ERROR);
}
#[test]
fn test_lower_interface_index_signature_mismatch() {
    let source = "interface Foo { [key: string]: number; foo: string; }";
    let (arena, declarations) = parse_interface_declarations(source, "Foo");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_interface_declarations(&declarations);
    assert_ne!(type_id, TypeId::ERROR);
}
#[test]
fn test_lower_interface_single_with_two_properties() {
    // Regression test: Single interface with two properties
    let source = "interface Point { x: number; y: number; }";
    let (arena, declarations) = parse_interface_declarations(source, "Point");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_interface_declarations(&declarations);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Object(shape_id) => {
            let shape = interner.object_shape(shape_id);
            assert_eq!(
                shape.properties.len(),
                2,
                "Expected 2 properties, got {}",
                shape.properties.len()
            );

            let mut found_x = None;
            let mut found_y = None;
            for prop in &shape.properties {
                let name = interner.resolve_atom(prop.name);
                match name.as_str() {
                    "x" => found_x = Some(prop),
                    "y" => found_y = Some(prop),
                    other => panic!("Unexpected property name: {other}"),
                }
            }

            let x = found_x.expect("Expected property x");
            let y = found_y.expect("Expected property y");
            assert_eq!(x.type_id, TypeId::NUMBER, "Expected x to be number");
            assert_eq!(y.type_id, TypeId::NUMBER, "Expected y to be number");
        }
        _ => panic!("Expected Object type, got {key:?}"),
    }
}
#[test]
fn test_lower_interface_merges_properties() {
    let source = "interface Foo { a: string; } interface Foo { b?: number; }";
    let (arena, declarations) = parse_interface_declarations(source, "Foo");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_interface_declarations(&declarations);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Object(shape_id) => {
            let shape = interner.object_shape(shape_id);
            let mut found_a = None;
            let mut found_b = None;
            for prop in &shape.properties {
                match interner.resolve_atom(prop.name).as_str() {
                    "a" => found_a = Some(prop),
                    "b" => found_b = Some(prop),
                    _ => {}
                }
            }

            let a = found_a.expect("Expected property a");
            let b = found_b.expect("Expected property b");
            assert_eq!(a.type_id, TypeId::STRING);
            assert!(!a.optional);
            assert_eq!(b.type_id, TypeId::NUMBER);
            assert!(b.optional);
        }
        _ => panic!("Expected Object type, got {key:?}"),
    }
}
#[test]
fn test_lower_interface_conflicting_property_types() {
    let source = "interface Foo { a: string; } interface Foo { a: number; }";
    let (arena, declarations) = parse_interface_declarations(source, "Foo");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_interface_declarations(&declarations);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Object(shape_id) => {
            let shape = interner.object_shape(shape_id);
            let prop = shape
                .properties
                .iter()
                .find(|prop| interner.resolve_atom(prop.name) == "a")
                .expect("Expected property a");
            assert_eq!(prop.type_id, TypeId::ERROR);
        }
        _ => panic!("Expected Object type, got {key:?}"),
    }
}
#[test]
fn test_lower_interface_method_overload_accumulates() {
    let source =
        "interface Foo { bar(x: string): number; } interface Foo { bar(x: number): string; }";
    let (arena, declarations) = parse_interface_declarations(source, "Foo");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_interface_declarations(&declarations);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Object(shape_id) => {
            let shape = interner.object_shape(shape_id);
            let prop = shape
                .properties
                .iter()
                .find(|prop| interner.resolve_atom(prop.name) == "bar")
                .expect("Expected property bar");
            let prop_key = interner.lookup(prop.type_id).expect("Type should exist");
            match prop_key {
                TypeData::Callable(callable_id) => {
                    let callable = interner.callable_shape(callable_id);
                    assert_eq!(callable.call_signatures.len(), 2);
                    let mut combos: Vec<(TypeId, TypeId)> = callable
                        .call_signatures
                        .iter()
                        .map(|sig| (sig.params[0].type_id, sig.return_type))
                        .collect();
                    combos.sort_by_key(|(param, _)| param.0);
                    assert_eq!(
                        combos,
                        vec![
                            (TypeId::NUMBER, TypeId::STRING),
                            (TypeId::STRING, TypeId::NUMBER)
                        ]
                    );
                }
                _ => panic!("Expected Callable type, got {prop_key:?}"),
            }
        }
        _ => panic!("Expected Object type, got {key:?}"),
    }
}

// ============================================================================
// Template Literal Edge Case Tests
// ============================================================================
#[test]
fn test_template_literal_empty_string() {
    let (arena, template_idx) = parse_template_literal_type("type T = ``;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(template_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    // Empty template literal is collapsed to empty string literal
    match key {
        TypeData::Literal(LiteralValue::String(atom)) => {
            assert_eq!(interner.resolve_atom(atom), "");
        }
        _ => panic!("Expected empty string Literal type, got {key:?}"),
    }
}
#[test]
fn test_template_literal_single_text_span() {
    let (arena, template_idx) = parse_template_literal_type("type T = `hello`;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(template_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    // Text-only templates are collapsed to string literals
    match key {
        TypeData::Literal(LiteralValue::String(atom)) => {
            assert_eq!(interner.resolve_atom(atom), "hello");
        }
        _ => panic!("Expected string Literal type, got {key:?}"),
    }
}
#[test]
fn test_template_literal_multiple_interpolations() {
    let (arena, template_idx) =
        parse_template_literal_type("type T = `${string}-${number}-${boolean}`;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(template_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::TemplateLiteral(spans) => {
            let spans = interner.template_list(spans);
            assert_eq!(spans.len(), 5); // type, text, type, text, type

            assert!(matches!(spans[0], TemplateSpan::Type(TypeId::STRING)));
            if let TemplateSpan::Text(atom) = &spans[1] {
                assert_eq!(interner.resolve_atom(*atom), "-");
            } else {
                panic!("Expected text span");
            }
            assert!(matches!(spans[2], TemplateSpan::Type(TypeId::NUMBER)));
            if let TemplateSpan::Text(atom) = &spans[3] {
                assert_eq!(interner.resolve_atom(*atom), "-");
            } else {
                panic!("Expected text span");
            }
            assert!(matches!(spans[4], TemplateSpan::Type(TypeId::BOOLEAN)));
        }
        _ => panic!("Expected TemplateLiteral type, got {key:?}"),
    }
}
#[test]
fn test_template_literal_consecutive_text_normalization() {
    let (arena, template_idx) = parse_template_literal_type("type T = `hello${string}world`;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(template_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::TemplateLiteral(spans) => {
            let spans = interner.template_list(spans);
            // Should have 3 spans: "hello", string, "world"
            assert_eq!(spans.len(), 3);

            if let TemplateSpan::Text(atom) = &spans[0] {
                assert_eq!(interner.resolve_atom(*atom), "hello");
            } else {
                panic!("Expected text span");
            }

            if let TemplateSpan::Type(t) = spans[1] {
                assert_eq!(t, TypeId::STRING);
            } else {
                panic!("Expected type span");
            }

            if let TemplateSpan::Text(atom) = &spans[2] {
                assert_eq!(interner.resolve_atom(*atom), "world");
            } else {
                panic!("Expected text span");
            }
        }
        _ => panic!("Expected TemplateLiteral type, got {key:?}"),
    }
}
#[test]
fn test_template_literal_only_interpolation() {
    let (arena, template_idx) = parse_template_literal_type("type T = `${string}`;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(template_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::TemplateLiteral(spans) => {
            let spans = interner.template_list(spans);
            assert_eq!(spans.len(), 1);
            assert!(matches!(spans[0], TemplateSpan::Type(TypeId::STRING)));
        }
        _ => panic!("Expected TemplateLiteral type, got {key:?}"),
    }
}
#[test]
fn test_template_literal_trailing_text() {
    let (arena, template_idx) = parse_template_literal_type("type T = `${string}!`;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(template_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::TemplateLiteral(spans) => {
            let spans = interner.template_list(spans);
            assert_eq!(spans.len(), 2);
            assert!(matches!(spans[0], TemplateSpan::Type(TypeId::STRING)));
            if let TemplateSpan::Text(atom) = &spans[1] {
                assert_eq!(interner.resolve_atom(*atom), "!");
            } else {
                panic!("Expected text span");
            }
        }
        _ => panic!("Expected TemplateLiteral type, got {key:?}"),
    }
}
#[test]
fn test_template_literal_leading_text() {
    let (arena, template_idx) = parse_template_literal_type("type T = `!${string}`;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(template_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::TemplateLiteral(spans) => {
            let spans = interner.template_list(spans);
            assert_eq!(spans.len(), 2);
            if let TemplateSpan::Text(atom) = &spans[0] {
                assert_eq!(interner.resolve_atom(*atom), "!");
            } else {
                panic!("Expected text span");
            }
            assert!(matches!(spans[1], TemplateSpan::Type(TypeId::STRING)));
        }
        _ => panic!("Expected TemplateLiteral type, got {key:?}"),
    }
}
#[test]
fn test_template_literal_escape_sequences() {
    let (arena, template_idx) = parse_template_literal_type(r#"type T = `hello\nworld`;"#);
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(template_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    // Text-only templates are collapsed to string literals
    match key {
        TypeData::Literal(LiteralValue::String(atom)) => {
            // The escape sequence should be processed
            let text = interner.resolve_atom(atom);
            assert_eq!(text, "hello\nworld");
        }
        _ => panic!("Expected string Literal type, got {key:?}"),
    }
}
#[test]
fn test_template_literal_escape_dollar_brace() {
    let (arena, template_idx) = parse_template_literal_type(r#"type T = `hello\${string}`;"#);
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(template_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    // Text-only templates are collapsed to string literals
    match key {
        TypeData::Literal(LiteralValue::String(atom)) => {
            // The escaped ${ should become literal ${ (not an interpolation)
            let text = interner.resolve_atom(atom);
            assert_eq!(text, "hello${string}");
        }
        _ => panic!("Expected string Literal type, got {key:?}"),
    }
}
#[test]
fn test_template_literal_with_union() {
    let (arena, template_idx) =
        parse_template_literal_type("type T = `prefix-${\"a\" | \"b\"}-suffix`;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(template_idx);
    // Should not exceed expansion limit and create a union
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Union(list_id) => {
            let members = interner.type_list(list_id);
            // Should have expanded to "prefix-a-suffix" | "prefix-b-suffix"
            assert_eq!(members.len(), 2);
        }
        _ => panic!("Expected Union type, got {key:?}"),
    }
}
#[test]
fn test_template_literal_with_multiple_unions() {
    // Test Cartesian product: `${"a" | "b"}-${"x" | "y"}` should produce 4 combinations
    let interner = TypeInterner::new();

    let a = interner.literal_string("a");
    let b = interner.literal_string("b");
    let union1 = interner.union(vec![a, b]);

    let x = interner.literal_string("x");
    let y = interner.literal_string("y");
    let union2 = interner.union(vec![x, y]);

    let spans = vec![
        TemplateSpan::Type(union1),
        TemplateSpan::Text(interner.intern_string("-")),
        TemplateSpan::Type(union2),
    ];

    let type_id = interner.template_literal(spans);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Union(list_id) => {
            let members = interner.type_list(list_id);
            // Should have 4 combinations: "a-x", "a-y", "b-x", "b-y"
            assert_eq!(members.len(), 4);

            // Verify all expected strings are present
            let mut strings: Vec<String> = members
                .iter()
                .filter_map(|&m| match interner.lookup(m) {
                    Some(TypeData::Literal(LiteralValue::String(atom))) => {
                        Some(interner.resolve_atom(atom))
                    }
                    _ => None,
                })
                .collect();
            strings.sort();
            assert_eq!(strings, vec!["a-x", "a-y", "b-x", "b-y"]);
        }
        _ => panic!("Expected Union type, got {key:?}"),
    }
}
#[test]
fn test_template_literal_single_string_literal() {
    // Template literal with single string literal interpolation should collapse to string literal
    let interner = TypeInterner::new();

    let a = interner.literal_string("hello");
    let spans = vec![
        TemplateSpan::Text(interner.intern_string("prefix-")),
        TemplateSpan::Type(a),
        TemplateSpan::Text(interner.intern_string("-suffix")),
    ];

    let type_id = interner.template_literal(spans);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Literal(LiteralValue::String(atom)) => {
            let text = interner.resolve_atom(atom);
            assert_eq!(text, "prefix-hello-suffix");
        }
        _ => panic!("Expected Literal type, got {key:?}"),
    }
}
#[test]
fn test_template_literal_only_texts_becomes_literal() {
    // Template literal with only text spans should collapse to string literal
    let interner = TypeInterner::new();

    let spans = vec![
        TemplateSpan::Text(interner.intern_string("hello")),
        TemplateSpan::Text(interner.intern_string(" ")),
        TemplateSpan::Text(interner.intern_string("world")),
    ];

    let type_id = interner.template_literal(spans);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Literal(LiteralValue::String(atom)) => {
            let text = interner.resolve_atom(atom);
            assert_eq!(text, "hello world");
        }
        _ => panic!("Expected Literal type, got {key:?}"),
    }
}
#[test]
fn test_template_literal_with_non_string_literal_stays_template() {
    // Template literal with non-expandable types should stay as template literal
    let interner = TypeInterner::new();

    let spans = vec![
        TemplateSpan::Text(interner.intern_string("prefix-")),
        TemplateSpan::Type(TypeId::STRING), // string primitive, not expandable
        TemplateSpan::Text(interner.intern_string("-suffix")),
    ];

    let type_id = interner.template_literal(spans);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::TemplateLiteral(_) => {
            // Expected: remains as template literal type
        }
        _ => panic!("Expected TemplateLiteral type, got {key:?}"),
    }
}
#[test]
fn test_template_literal_normalization_merges_consecutive_texts() {
    let interner = TypeInterner::new();

    // Create spans with consecutive text that should be merged
    let spans = vec![
        TemplateSpan::Text(interner.intern_string("hello")),
        TemplateSpan::Text(interner.intern_string(" ")),
        TemplateSpan::Text(interner.intern_string("world")),
    ];

    let type_id = interner.template_literal(spans);
    let key = interner.lookup(type_id).expect("Type should exist");
    // Text-only templates are collapsed to string literals
    match key {
        TypeData::Literal(LiteralValue::String(atom)) => {
            // After normalization and expansion, text-only becomes string literal
            assert_eq!(interner.resolve_atom(atom), "hello world");
        }
        _ => panic!("Expected string Literal type, got {key:?}"),
    }
}
#[test]
fn test_template_literal_interpolation_positions() {
    let interner = TypeInterner::new();

    let spans = vec![
        TemplateSpan::Text(interner.intern_string("prefix")),
        TemplateSpan::Type(TypeId::STRING),
        TemplateSpan::Text(interner.intern_string("-")),
        TemplateSpan::Type(TypeId::NUMBER),
    ];

    let type_id = interner.template_literal(spans);
    let positions = interner.template_literal_interpolation_positions(type_id);

    assert_eq!(positions, vec![1, 3]); // Type spans at indices 1 and 3
}
#[test]
fn test_template_literal_get_span() {
    let interner = TypeInterner::new();

    let spans = vec![
        TemplateSpan::Text(interner.intern_string("hello")),
        TemplateSpan::Type(TypeId::STRING),
    ];

    let type_id = interner.template_literal(spans);

    let span_0 = interner.template_literal_get_span(type_id, 0);
    assert!(span_0.is_some());
    assert!(span_0.unwrap().is_text());

    let span_1 = interner.template_literal_get_span(type_id, 1);
    assert!(span_1.is_some());
    assert!(span_1.unwrap().is_type());

    let span_2 = interner.template_literal_get_span(type_id, 2);
    assert!(span_2.is_none()); // Out of bounds
}
#[test]
fn test_template_literal_span_count() {
    let interner = TypeInterner::new();

    let spans = vec![
        TemplateSpan::Text(interner.intern_string("hello")),
        TemplateSpan::Type(TypeId::STRING),
        TemplateSpan::Text(interner.intern_string("world")),
    ];

    let type_id = interner.template_literal(spans);
    assert_eq!(interner.template_literal_span_count(type_id), 3);
}
#[test]
fn test_template_literal_is_text_only() {
    let interner = TypeInterner::new();

    // Text only
    let spans_text_only = vec![TemplateSpan::Text(interner.intern_string("hello"))];
    let type_id_text_only = interner.template_literal(spans_text_only);
    assert!(interner.template_literal_is_text_only(type_id_text_only));

    // With interpolation
    let spans_with_type = vec![
        TemplateSpan::Text(interner.intern_string("hello")),
        TemplateSpan::Type(TypeId::STRING),
    ];
    let type_id_with_type = interner.template_literal(spans_with_type);
    assert!(!interner.template_literal_is_text_only(type_id_with_type));

    // Non-template literal type
    assert!(!interner.template_literal_is_text_only(TypeId::STRING));
}

// =============================================================================
// Interface Merge Ordering Tests
// =============================================================================

/// Helper to find all interface declarations for a given name in the arena
fn find_interface_declarations(arena: &NodeArena, name: &str) -> Vec<NodeIndex> {
    let mut decls = Vec::new();
    for i in 0..arena.len() {
        let idx = NodeIndex(i as u32);
        if let Some(node) = arena.get(idx)
            && node.kind == syntax_kind_ext::INTERFACE_DECLARATION
            && let Some(interface) = arena.get_interface(node)
            && let Some(name_node) = arena.get(interface.name)
            && let Some(id_data) = arena.get_identifier(name_node)
            && id_data.escaped_text == name
        {
            decls.push(idx);
        }
    }
    decls
}

/// TypeScript's interface merging puts later declarations' method overloads first.
/// This is critical for overload resolution: e.g., `PromiseConstructor`'s tuple overload
/// from es2015.promise.d.ts (later) should be tried before the Iterable overload from
/// es2015.iterable.d.ts (earlier).
#[test]
fn test_merged_interface_method_overloads_later_first() {
    // Two interface declarations for Foo, each with a method bar(...)
    // Declaration 1 has bar(x: string): string
    // Declaration 2 has bar(x: number): number
    // After merging, bar's overloads should be [number->number, string->string]
    // (later declaration first)
    let source = r#"
interface Foo {
    bar(x: string): string;
}
interface Foo {
    bar(x: number): number;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let arena = std::mem::take(&mut parser.arena);
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let decls = find_interface_declarations(&arena, "Foo");
    assert_eq!(decls.len(), 2, "Should find 2 interface declarations");

    let type_id = lowering.lower_interface_declarations(&decls);

    // The result should be an Object or Callable type with the bar method
    let type_data = interner.lookup(type_id).expect("Type should exist");
    match type_data {
        TypeData::Callable(callable_shape_id) => {
            let callable = interner.callable_shape(callable_shape_id);
            // bar should have 2 call signatures
            assert_eq!(callable.call_signatures.len(), 2, "Should have 2 overloads");
            // The second declaration's overload (number->number) should be first
            let first_sig = &callable.call_signatures[0];
            assert_eq!(
                first_sig.return_type,
                TypeId::NUMBER,
                "First overload should be from later declaration (number->number)"
            );
            let second_sig = &callable.call_signatures[1];
            assert_eq!(
                second_sig.return_type,
                TypeId::STRING,
                "Second overload should be from earlier declaration (string->string)"
            );
        }
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            let shape = interner.object_shape(shape_id);
            // Find the bar property
            let bar_prop = shape
                .properties
                .iter()
                .find(|p| interner.resolve_atom(p.name) == "bar")
                .expect("Should have bar property");
            // bar should be a callable with 2 overloads
            let bar_data = interner
                .lookup(bar_prop.type_id)
                .expect("bar type should exist");
            match bar_data {
                TypeData::Callable(callable_shape_id) => {
                    let callable = interner.callable_shape(callable_shape_id);
                    assert_eq!(callable.call_signatures.len(), 2, "Should have 2 overloads");
                    // Later declaration's overload should be first
                    let first_sig = &callable.call_signatures[0];
                    assert_eq!(
                        first_sig.return_type,
                        TypeId::NUMBER,
                        "First overload should be from later declaration (number->number)"
                    );
                    let second_sig = &callable.call_signatures[1];
                    assert_eq!(
                        second_sig.return_type,
                        TypeId::STRING,
                        "Second overload should be from earlier declaration (string->string)"
                    );
                }
                _ => panic!("Expected Callable type for bar, got {bar_data:?}"),
            }
        }
        _ => panic!("Expected Object or Callable type, got {type_data:?}"),
    }
}

// =============================================================================
// Advanced Type Lowering Tests
// =============================================================================
#[test]
fn test_lower_nested_generics() {
    // Map<string, Map<number, boolean>> - nested generic type application
    let (arena, type_idx) =
        parse_type_alias_type_node("type T = Map<string, Map<number, boolean>>;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    // Should be an Application type with nested Application as an argument
    match key {
        TypeData::Application(app_id) => {
            let app = interner.type_application(app_id);
            assert_eq!(app.args.len(), 2);
            // First arg should be STRING
            assert_eq!(app.args[0], TypeId::STRING);
            // Second arg should be another Application (Map<number, boolean>)
            match interner.lookup(app.args[1]) {
                Some(TypeData::Application(_)) => {} // Expected nested Application
                other => panic!("Expected nested Application type, got {other:?}"),
            }
        }
        _ => panic!("Expected Application type, got {key:?}"),
    }
}
#[test]
fn test_lower_type_with_multiple_type_params() {
    // T<T1, T2, T3> - generic type with 3 type arguments
    let (arena, type_idx) = parse_type_alias_type_node("type X = Record<string, number, boolean>;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Application(app_id) => {
            let app = interner.type_application(app_id);
            assert_eq!(app.args.len(), 3);
            assert_eq!(app.args[0], TypeId::STRING);
            assert_eq!(app.args[1], TypeId::NUMBER);
            assert_eq!(app.args[2], TypeId::BOOLEAN);
        }
        _ => panic!("Expected Application type, got {key:?}"),
    }
}
#[test]
fn test_lower_mapped_type_keyof() {
    // { [K in keyof T]: T[K] } - mapped type with keyof and indexed access
    let (arena, mapped_idx) = parse_mapped_type("type T<U> = { [K in keyof U]: U[K] };");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(mapped_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Mapped(mapped_id) => {
            let mapped = interner.mapped_type(mapped_id);
            assert_eq!(interner.resolve_atom(mapped.type_param.name), "K");
            // constraint should be KeyOf type
            match interner.lookup(mapped.constraint) {
                Some(TypeData::KeyOf(_)) => {} // Expected
                other => panic!("Expected KeyOf constraint, got {other:?}"),
            }
            // template should be IndexAccess type
            match interner.lookup(mapped.template) {
                Some(TypeData::IndexAccess(_, _)) => {} // Expected
                other => panic!("Expected IndexAccess template, got {other:?}"),
            }
        }
        _ => panic!("Expected Mapped type, got {key:?}"),
    }
}
#[test]
fn test_lower_conditional_type_infer() {
    // T extends Array<infer U> ? U : T - conditional with infer in array
    let (arena, type_idx) =
        parse_type_alias_type_node("type Unwrap<T> = T extends Array<infer U> ? U : T;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Conditional(cond_id) => {
            let cond = interner.conditional_type(cond_id);
            // extends_type should be an Array with infer U as element
            match interner.lookup(cond.extends_type) {
                Some(TypeData::Array(elem)) => match interner.lookup(elem) {
                    Some(TypeData::Infer(info)) => {
                        assert_eq!(interner.resolve_atom(info.name), "U");
                    }
                    other => panic!("Expected Infer type in array element, got {other:?}"),
                },
                other => panic!("Expected Array type in extends, got {other:?}"),
            }
        }
        _ => panic!("Expected Conditional type, got {key:?}"),
    }
}
#[test]
fn test_lower_template_literal_type() {
    // `on${string}` - template literal with interpolation
    let (arena, template_idx) = parse_template_literal_type("type T = `on${string}`;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(template_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::TemplateLiteral(spans) => {
            let spans = interner.template_list(spans);
            assert_eq!(spans.len(), 2);
            // First span: "on"
            if let TemplateSpan::Text(atom) = &spans[0] {
                assert_eq!(interner.resolve_atom(*atom), "on");
            } else {
                panic!("Expected text span");
            }
            // Second span: string type
            assert!(matches!(spans[1], TemplateSpan::Type(TypeId::STRING)));
        }
        _ => panic!("Expected TemplateLiteral type, got {key:?}"),
    }
}
#[test]
fn test_lower_index_access_type() {
    // T['key'] - indexed access type
    let (arena, type_idx) = parse_type_alias_type_node("type V<T> = T['key'];");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::IndexAccess(_obj_type, index_type) => {
            // Index should be a string literal "key"
            match interner.lookup(index_type) {
                Some(TypeData::Literal(LiteralValue::String(atom))) => {
                    assert_eq!(interner.resolve_atom(atom), "key");
                }
                other => panic!("Expected string literal index, got {other:?}"),
            }
        }
        _ => panic!("Expected IndexAccess type, got {key:?}"),
    }
}
#[test]
fn test_lower_keyof_type() {
    // keyof { a: string; b: number } - keyof type operator on concrete type
    // Note: The lowering produces a KeyOf type; evaluation to union happens in solver
    let (arena, type_idx) = parse_type_alias_type_node("type K = keyof { a: string; b: number };");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    // Lowering produces a KeyOf type; the solver evaluates it to union of literals
    match key {
        TypeData::KeyOf(inner) => {
            // Inner should be an Object type with properties a and b
            match interner.lookup(inner) {
                Some(TypeData::Object(_)) => {} // Expected
                other => panic!("Expected Object type for inner, got {other:?}"),
            }
        }
        _ => panic!("Expected KeyOf type, got {key:?}"),
    }
}
#[test]
fn test_lower_tuple_type() {
    // [string, number, boolean] - tuple with 3 elements
    let (arena, tuple_idx) = parse_tuple_type("type T = [string, number, boolean];");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(tuple_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Tuple(elements) => {
            let elements = interner.tuple_list(elements);
            assert_eq!(elements.len(), 3);
            assert_eq!(elements[0].type_id, TypeId::STRING);
            assert_eq!(elements[1].type_id, TypeId::NUMBER);
            assert_eq!(elements[2].type_id, TypeId::BOOLEAN);
        }
        _ => panic!("Expected Tuple type, got {key:?}"),
    }
}
#[test]
fn test_lower_tuple_with_rest() {
    // [string, ...number[]] - tuple with rest element
    let (arena, tuple_idx) = parse_tuple_type("type T = [string, ...number[]];");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(tuple_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Tuple(elements) => {
            let elements = interner.tuple_list(elements);
            assert_eq!(elements.len(), 2);

            // First element: string
            assert_eq!(elements[0].type_id, TypeId::STRING);
            assert!(!elements[0].rest);

            // Second element: rest number[]
            assert!(elements[1].rest);
            match interner.lookup(elements[1].type_id) {
                Some(TypeData::Array(elem)) => assert_eq!(elem, TypeId::NUMBER),
                other => panic!("Expected Array type for rest element, got {other:?}"),
            }
        }
        _ => panic!("Expected Tuple type, got {key:?}"),
    }
}
#[test]
fn test_lower_optional_property() {
    // { name?: string } - object with optional property
    let (arena, literal_idx) = parse_type_literal("type T = { name?: string };");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(literal_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Object(shape_id) => {
            let shape = interner.object_shape(shape_id);
            let prop = shape
                .properties
                .iter()
                .find(|p| interner.resolve_atom(p.name) == "name")
                .expect("Expected name property");
            assert_eq!(prop.type_id, TypeId::STRING);
            assert!(prop.optional);
        }
        _ => panic!("Expected Object type, got {key:?}"),
    }
}
#[test]
fn test_lower_readonly_property() {
    // { readonly id: number } - object with readonly property
    let (arena, literal_idx) = parse_type_literal("type T = { readonly id: number };");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(literal_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Object(shape_id) => {
            let shape = interner.object_shape(shape_id);
            let prop = shape
                .properties
                .iter()
                .find(|p| interner.resolve_atom(p.name) == "id")
                .expect("Expected id property");
            assert_eq!(prop.type_id, TypeId::NUMBER);
            assert!(prop.readonly);
        }
        _ => panic!("Expected Object type, got {key:?}"),
    }
}
#[test]
fn test_lower_intersection_type() {
    // { a: string } & { b: number } - intersection of two object types
    // Note: The lowering normalizes object intersections into a merged Object type
    let (arena, type_idx) = parse_type_alias_type_node("type T = { a: string } & { b: number };");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Object(shape_id) => {
            // The two objects should be merged into one with both properties
            let shape = interner.object_shape(shape_id);
            let prop_a = shape
                .properties
                .iter()
                .find(|p| interner.resolve_atom(p.name) == "a");
            let prop_b = shape
                .properties
                .iter()
                .find(|p| interner.resolve_atom(p.name) == "b");
            assert!(prop_a.is_some(), "Should have property 'a'");
            assert!(prop_b.is_some(), "Should have property 'b'");
        }
        _ => panic!("Expected merged Object type, got {key:?}"),
    }
}
#[test]
fn test_lower_parenthesized_type() {
    // (string | number) - parenthesized union type
    let (arena, type_idx) = parse_type_alias_type_node("type T = (string | number);");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    // Parentheses are typically transparent - should get the inner type (union)
    match key {
        TypeData::Union(members) => {
            let members = interner.type_list(members);
            assert_eq!(members.as_ref(), [TypeId::STRING, TypeId::NUMBER]);
        }
        _ => panic!("Expected Union type, got {key:?}"),
    }
}
