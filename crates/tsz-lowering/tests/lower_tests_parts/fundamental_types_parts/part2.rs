#[test]
fn test_lower_template_literal_type_spans() {
    let (arena, template_idx) = parse_template_literal_type("type T = `hello${string}world`;");

    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(template_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::TemplateLiteral(spans) => {
            let spans = interner.template_list(spans);
            assert_eq!(spans.len(), 3);
            match spans[0] {
                TemplateSpan::Text(atom) => assert_eq!(interner.resolve_atom(atom), "hello"),
                _ => panic!("Expected head text span"),
            }
            match spans[1] {
                TemplateSpan::Type(t) => assert_eq!(t, TypeId::STRING),
                _ => panic!("Expected type span"),
            }
            match spans[2] {
                TemplateSpan::Text(atom) => assert_eq!(interner.resolve_atom(atom), "world"),
                _ => panic!("Expected tail text span"),
            }
        }
        _ => panic!("Expected TemplateLiteral type, got {key:?}"),
    }
}

#[test]
fn test_lower_mapped_type_modifiers_and_constraint() {
    let (arena, mapped_idx) = parse_mapped_type("type T = { readonly [K in string]?: number };");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(mapped_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Mapped(mapped_id) => {
            let mapped = interner.mapped_type(mapped_id);
            assert_eq!(interner.resolve_atom(mapped.type_param.name), "K");
            assert_eq!(mapped.constraint, TypeId::STRING);
            assert_eq!(mapped.template, TypeId::NUMBER);
            assert_eq!(mapped.readonly_modifier, Some(MappedModifier::Add));
            assert_eq!(mapped.optional_modifier, Some(MappedModifier::Add));
        }
        _ => panic!("Expected Mapped type, got {key:?}"),
    }
}

#[test]
fn test_lower_mapped_type_remove_modifiers() {
    let (arena, mapped_idx) = parse_mapped_type("type T = { -readonly [K in string]-?: number };");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(mapped_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Mapped(mapped_id) => {
            let mapped = interner.mapped_type(mapped_id);
            assert_eq!(mapped.readonly_modifier, Some(MappedModifier::Remove));
            assert_eq!(mapped.optional_modifier, Some(MappedModifier::Remove));
        }
        _ => panic!("Expected Mapped type, got {key:?}"),
    }
}

#[test]
fn test_lower_type_literal_object_properties() {
    let (arena, literal_idx) =
        parse_type_literal("type T = { readonly foo?: string; bar: number; };");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(literal_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Object(shape_id) => {
            let shape = interner.object_shape(shape_id);
            let foo = shape
                .properties
                .iter()
                .find(|prop| interner.resolve_atom(prop.name) == "foo")
                .expect("Expected foo property");
            assert_eq!(foo.type_id, TypeId::STRING);
            assert!(foo.optional);
            assert!(foo.readonly);

            let bar = shape
                .properties
                .iter()
                .find(|prop| interner.resolve_atom(prop.name) == "bar")
                .expect("Expected bar property");
            assert_eq!(bar.type_id, TypeId::NUMBER);
            assert!(!bar.optional);
            assert!(!bar.readonly);
        }
        _ => panic!("Expected Object type, got {key:?}"),
    }
}

#[test]
fn test_lower_type_literal_nested_object() {
    let (arena, literal_idx) =
        parse_type_alias_type_node("type T = { config: { enabled: boolean; retries?: number }; };");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(literal_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Object(shape_id) => {
            let shape = interner.object_shape(shape_id);
            let config = shape
                .properties
                .iter()
                .find(|prop| interner.resolve_atom(prop.name) == "config")
                .expect("Expected config property");

            match interner.lookup(config.type_id) {
                Some(TypeData::Object(nested_id)) => {
                    let nested = interner.object_shape(nested_id);
                    let enabled = nested
                        .properties
                        .iter()
                        .find(|prop| interner.resolve_atom(prop.name) == "enabled")
                        .expect("Expected enabled property");
                    assert_eq!(enabled.type_id, TypeId::BOOLEAN);
                    assert!(!enabled.optional);

                    let retries = nested
                        .properties
                        .iter()
                        .find(|prop| interner.resolve_atom(prop.name) == "retries")
                        .expect("Expected retries property");
                    assert_eq!(retries.type_id, TypeId::NUMBER);
                    assert!(retries.optional);
                }
                other => panic!("Expected nested Object type, got {other:?}"),
            }
        }
        _ => panic!("Expected Object type, got {key:?}"),
    }
}
