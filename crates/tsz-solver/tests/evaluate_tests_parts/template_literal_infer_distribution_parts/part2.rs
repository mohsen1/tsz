#[test]
fn test_template_literal_single_text() {
    // `hello` just text, no interpolations
    let interner = TypeInterner::new();

    let template =
        interner.template_literal(vec![TemplateSpan::Text(interner.intern_string("hello"))]);

    // Should be equivalent to "hello" literal
    let result = evaluate_type(&interner, template);
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_template_literal_pattern_infer_numeric() {
    // `id_${infer N extends number}` - infer from numeric pattern
    let interner = TypeInterner::new();

    let n_name = interner.intern_string("N");
    let infer_n = interner.intern(TypeData::Infer(TypeParamInfo {
        name: n_name,
        constraint: Some(TypeId::NUMBER),
        default: None,
        is_const: false,
    }));

    let extends_template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("id_")),
        TemplateSpan::Type(infer_n),
    ]);

    // Test matching against "id_42"
    let lit_id_42 = interner.literal_string("id_42");

    let cond = ConditionalType {
        check_type: lit_id_42,
        extends_type: extends_template,
        true_type: infer_n,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Should infer something or at least not error
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_template_literal_multiple_adjacent_types() {
    // `${A}${B}${C}` - multiple type interpolations
    let interner = TypeInterner::new();

    let lit_x = interner.literal_string("x");
    let lit_y = interner.literal_string("y");
    let lit_z = interner.literal_string("z");

    let template = interner.template_literal(vec![
        TemplateSpan::Type(lit_x),
        TemplateSpan::Type(lit_y),
        TemplateSpan::Type(lit_z),
    ]);

    // With optimization, string literal interpolations are expanded and concatenated
    // So `${"x"}${"y"}${"z"}` becomes "xyz" string literal
    match interner.lookup(template) {
        Some(TypeData::Literal(LiteralValue::String(atom))) => {
            let s = interner.resolve_atom_ref(atom);
            assert_eq!(s.as_ref(), "xyz", "Expected concatenated string literal");
        }
        _ => panic!(
            "Expected string literal for concatenated string interpolations, got {:?}",
            interner.lookup(template)
        ),
    }
}

#[test]
fn test_template_literal_union_in_middle() {
    // `pre_${"a"|"b"|"c"}_suf` - union in middle position
    let interner = TypeInterner::new();

    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let lit_c = interner.literal_string("c");
    let union = interner.union(vec![lit_a, lit_b, lit_c]);

    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("pre_")),
        TemplateSpan::Type(union),
        TemplateSpan::Text(interner.intern_string("_suf")),
    ]);

    // With optimization, template literals with expandable unions become a union of string literals
    // `pre_${"a"|"b"|"c"}_suf` becomes "pre_a_suf" | "pre_b_suf" | "pre_c_suf"
    match interner.lookup(template) {
        Some(TypeData::Union(members_id)) => {
            let members = interner.type_list(members_id);
            assert_eq!(members.len(), 3, "Expected 3 members in union");
        }
        _ => panic!(
            "Expected Union type for template with union interpolation, got {:?}",
            interner.lookup(template)
        ),
    }
}

#[test]
fn test_template_literal_bigint_type() {
    // `value_${bigint}` - template with bigint
    let interner = TypeInterner::new();

    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("value_")),
        TemplateSpan::Type(TypeId::BIGINT),
    ]);

    match interner.lookup(template) {
        Some(TypeData::TemplateLiteral(_)) => (),
        _ => panic!("Expected TemplateLiteral type"),
    }
}
