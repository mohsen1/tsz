use super::*;

// =================================================================
// Mapped type formatting
// =================================================================

#[test]
fn format_mapped_type_basic() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let mapped = db.mapped(MappedType {
        type_param: TypeParamInfo {
            name: db.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: TypeId::STRING,
        template: TypeId::NUMBER,
        name_type: None,
        readonly_modifier: None,
        optional_modifier: None,
    });
    let result = fmt.format(mapped);
    assert_eq!(result, "{ [x: string]: number; }");
}

#[test]
fn format_mapped_type_with_remove_optional() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let mapped = db.mapped(MappedType {
        type_param: TypeParamInfo {
            name: db.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: TypeId::STRING,
        template: TypeId::NUMBER,
        name_type: None,
        readonly_modifier: None,
        optional_modifier: Some(MappedModifier::Remove),
    });
    let result = fmt.format(mapped);
    assert!(
        result.contains("]-?:"),
        "Expected remove optional modifier '-?', got: {result}"
    );
}

#[test]
fn format_mapped_type_with_remove_readonly() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let mapped = db.mapped(MappedType {
        type_param: TypeParamInfo {
            name: db.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: TypeId::STRING,
        template: TypeId::NUMBER,
        name_type: None,
        readonly_modifier: Some(MappedModifier::Remove),
        optional_modifier: None,
    });
    let result = fmt.format(mapped);
    assert!(
        result.contains("-readonly"),
        "Expected remove readonly modifier, got: {result}"
    );
}

#[test]
fn format_mapped_string_index_signature_like() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let mapped = db.mapped(MappedType {
        type_param: TypeParamInfo {
            name: db.intern_string("P"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: TypeId::STRING,
        template: TypeId::NUMBER,
        name_type: None,
        readonly_modifier: None,
        optional_modifier: None,
    });

    assert_eq!(fmt.format(mapped), "{ [x: string]: number; }");
}

#[test]
fn format_mapped_preserves_key_dependent_template() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);
    let key_name = db.intern_string("P");
    let key_param = db.type_param(TypeParamInfo {
        name: key_name,
        constraint: None,
        default: None,
        is_const: false,
    });
    let mapped = db.mapped(MappedType {
        type_param: TypeParamInfo {
            name: key_name,
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: TypeId::STRING,
        template: key_param,
        name_type: None,
        readonly_modifier: None,
        optional_modifier: None,
    });

    assert_eq!(fmt.format(mapped), "{ [P in string]: P; }");
}

// =================================================================
// Template literal formatting
// =================================================================

#[test]
fn format_template_literal_text_only() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let tl = db.template_literal(vec![TemplateSpan::Text(db.intern_string("hello"))]);
    // Text-only template literals may be simplified by the interner
    // but if they survive, they should format with backticks
    let result = fmt.format(tl);
    assert!(
        result.contains("hello"),
        "Expected 'hello' in template literal, got: {result}"
    );
}

#[test]
fn format_template_literal_with_type_interpolation() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let tl = db.template_literal(vec![
        TemplateSpan::Text(db.intern_string("hello ")),
        TemplateSpan::Type(TypeId::STRING),
    ]);
    let result = fmt.format(tl);
    assert!(
        result.contains("hello "),
        "Expected 'hello ' prefix, got: {result}"
    );
    assert!(
        result.contains("${string}"),
        "Expected '${{string}}' interpolation, got: {result}"
    );
}

#[test]
fn format_template_literal_complex() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let tl = db.template_literal(vec![
        TemplateSpan::Text(db.intern_string("key_")),
        TemplateSpan::Type(TypeId::NUMBER),
        TemplateSpan::Text(db.intern_string("_suffix")),
    ]);
    let result = fmt.format(tl);
    assert!(result.contains("key_"), "Expected 'key_', got: {result}");
    assert!(
        result.contains("${number}"),
        "Expected '${{number}}', got: {result}"
    );
    assert!(
        result.contains("_suffix"),
        "Expected '_suffix', got: {result}"
    );
}

// =================================================================
// String intrinsic formatting
// =================================================================

#[test]
fn format_string_intrinsic_uppercase() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let upper = db.string_intrinsic(StringIntrinsicKind::Uppercase, TypeId::STRING);
    assert_eq!(fmt.format(upper), "Uppercase<string>");
}

#[test]
fn format_string_intrinsic_lowercase() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let lower = db.string_intrinsic(StringIntrinsicKind::Lowercase, TypeId::STRING);
    assert_eq!(fmt.format(lower), "Lowercase<string>");
}

#[test]
fn format_string_intrinsic_capitalize() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let cap = db.string_intrinsic(StringIntrinsicKind::Capitalize, TypeId::STRING);
    assert_eq!(fmt.format(cap), "Capitalize<string>");
}

#[test]
fn format_string_intrinsic_uncapitalize() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let uncap = db.string_intrinsic(StringIntrinsicKind::Uncapitalize, TypeId::STRING);
    assert_eq!(fmt.format(uncap), "Uncapitalize<string>");
}

// =================================================================
// Error type formatting
// =================================================================

#[test]
fn format_error_type() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);
    assert_eq!(fmt.format(TypeId::ERROR), "error");
}

// =================================================================
// Depth limiting (deeply nested types)
// =================================================================

#[test]
fn format_deeply_nested_array_truncated() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    // Create deeply nested arrays: string[][][][][][]...
    let mut current = TypeId::STRING;
    for _ in 0..10 {
        current = db.array(current);
    }
    let result = fmt.format(current);
    // At some depth, the formatter should produce "..." due to max_depth
    assert!(
        result.contains("..."),
        "Deeply nested type should hit depth limit and show '...', got: {result}"
    );
}

#[test]
fn format_deeply_nested_union_truncated() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    // Create nested unions: wrap in array at each level to increase depth
    let mut current = TypeId::STRING;
    for _ in 0..10 {
        let inner_union = db.union2(current, TypeId::NUMBER);
        current = db.array(inner_union);
    }
    let result = fmt.format(current);
    // Should hit depth limit
    assert!(
        result.contains("..."),
        "Deeply nested type should truncate, got: {result}"
    );
}

#[test]
fn format_deeply_nested_object_uses_object_elision() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let mut current = db.object(vec![PropertyInfo::new(
        db.intern_string("leaf"),
        TypeId::NUMBER,
    )]);
    for i in 0..12 {
        current = db.object(vec![PropertyInfo::new(
            db.intern_string(&format!("p{i}")),
            current,
        )]);
    }

    let result = fmt.format(current);
    assert!(
        result.contains("{ ...; }"),
        "Deeply nested object branches should elide as '{{ ...; }}', got: {result}"
    );
}

// =================================================================
// Special types
// =================================================================

#[test]
fn format_type_parameter() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let tp = db.type_param(TypeParamInfo {
        name: db.intern_string("MyType"),
        constraint: None,
        default: None,
        is_const: false,
    });
    assert_eq!(fmt.format(tp), "MyType");
}

#[test]
fn format_keyof_type() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let keyof = db.keyof(TypeId::STRING);
    assert_eq!(fmt.format(keyof), "keyof string");
}

#[test]
fn format_keyof_intersection_distributes() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let t = db.type_param(TypeParamInfo {
        name: db.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    });
    let u = db.type_param(TypeParamInfo {
        name: db.intern_string("U"),
        constraint: None,
        default: None,
        is_const: false,
    });
    let intersection = db.intersection2(t, u);
    let keyof = db.keyof(intersection);

    assert_eq!(fmt.format(keyof), "keyof T | keyof U");
}

#[test]
fn format_readonly_type() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let ro = db.readonly_type(TypeId::NUMBER);
    assert_eq!(fmt.format(ro), "readonly number");
}

#[test]
fn format_index_access_type() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let idx = db.index_access(TypeId::STRING, TypeId::NUMBER);
    assert_eq!(fmt.format(idx), "string[number]");
}

#[test]
fn format_this_type() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let this = db.this_type();
    assert_eq!(fmt.format(this), "this");
}

#[test]
fn format_infer_type() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let infer = db.infer(TypeParamInfo {
        name: db.intern_string("R"),
        constraint: None,
        default: None,
        is_const: false,
    });
    assert_eq!(fmt.format(infer), "infer R");
}

#[test]
fn format_unique_symbol() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let sym = db.unique_symbol(crate::types::SymbolRef(999));
    assert_eq!(fmt.format(sym), "unique symbol");
}

#[test]
fn format_no_infer_type() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    // NoInfer<T> is transparent in error messages - tsc displays just T
    let no_infer = db.no_infer(TypeId::STRING);
    assert_eq!(fmt.format(no_infer), "string");
}

// =================================================================
// Generic application formatting
// =================================================================

#[test]
fn format_application_single_arg() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let base = db.lazy(crate::def::DefId(100));
    let app = db.application(base, vec![TypeId::NUMBER]);
    let result = fmt.format(app);
    // Without def store, base resolves to "Lazy(100)"
    assert!(
        result.contains("Lazy(100)"),
        "Expected 'Lazy(100)', got: {result}"
    );
    assert!(
        result.contains("<number>"),
        "Expected '<number>', got: {result}"
    );
}

#[test]
fn format_application_two_args() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let base = db.lazy(crate::def::DefId(200));
    let app = db.application(base, vec![TypeId::STRING, TypeId::NUMBER]);
    let result = fmt.format(app);
    assert!(
        result.contains("<string, number>"),
        "Expected '<string, number>', got: {result}"
    );
}

