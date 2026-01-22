use super::*;

#[test]
fn test_type_id_intrinsics() {
    assert!(TypeId::ANY.is_intrinsic());
    assert!(TypeId::STRING.is_intrinsic());
    assert!(!TypeId(100).is_intrinsic());
    assert!(!TypeId(1000).is_intrinsic());
}

#[test]
fn test_type_id_equality() {
    // O(1) equality check
    let a = TypeId(42);
    let b = TypeId(42);
    let c = TypeId(43);

    assert_eq!(a, b);
    assert_ne!(a, c);
}

#[test]
fn test_ordered_float_hash() {
    use std::collections::HashSet;

    let mut set = HashSet::new();
    set.insert(OrderedFloat(1.5));
    set.insert(OrderedFloat(2.5));
    set.insert(OrderedFloat(1.5)); // duplicate

    assert_eq!(set.len(), 2);
}

#[test]
fn test_type_id_is_error() {
    assert!(TypeId::ERROR.is_error());
    assert!(!TypeId::STRING.is_error());
    assert!(!TypeId::ANY.is_error());
    assert!(!TypeId::NEVER.is_error());
    assert!(!TypeId(100).is_error());
}

#[test]
fn test_type_id_is_any() {
    assert!(TypeId::ANY.is_any());
    assert!(!TypeId::STRING.is_any());
    assert!(!TypeId::ERROR.is_any());
    assert!(!TypeId::UNKNOWN.is_any());
    assert!(!TypeId(100).is_any());
}

#[test]
fn test_type_id_is_unknown() {
    assert!(TypeId::UNKNOWN.is_unknown());
    assert!(!TypeId::STRING.is_unknown());
    assert!(!TypeId::ANY.is_unknown());
    assert!(!TypeId::NEVER.is_unknown());
    assert!(!TypeId(100).is_unknown());
}

#[test]
fn test_type_id_is_never() {
    assert!(TypeId::NEVER.is_never());
    assert!(!TypeId::STRING.is_never());
    assert!(!TypeId::ANY.is_never());
    assert!(!TypeId::UNKNOWN.is_never());
    assert!(!TypeId(100).is_never());
}

#[test]
fn test_intrinsic_kind_to_type_id() {
    assert_eq!(IntrinsicKind::Any.to_type_id(), TypeId::ANY);
    assert_eq!(IntrinsicKind::Unknown.to_type_id(), TypeId::UNKNOWN);
    assert_eq!(IntrinsicKind::Never.to_type_id(), TypeId::NEVER);
    assert_eq!(IntrinsicKind::Void.to_type_id(), TypeId::VOID);
    assert_eq!(IntrinsicKind::Null.to_type_id(), TypeId::NULL);
    assert_eq!(IntrinsicKind::Undefined.to_type_id(), TypeId::UNDEFINED);
    assert_eq!(IntrinsicKind::Boolean.to_type_id(), TypeId::BOOLEAN);
    assert_eq!(IntrinsicKind::Number.to_type_id(), TypeId::NUMBER);
    assert_eq!(IntrinsicKind::String.to_type_id(), TypeId::STRING);
    assert_eq!(IntrinsicKind::Bigint.to_type_id(), TypeId::BIGINT);
    assert_eq!(IntrinsicKind::Symbol.to_type_id(), TypeId::SYMBOL);
    assert_eq!(IntrinsicKind::Object.to_type_id(), TypeId::OBJECT);
}

#[test]
fn test_type_id_intrinsic_constants() {
    // Verify all intrinsic constants are unique
    let intrinsics = [
        TypeId::NONE,
        TypeId::ERROR,
        TypeId::NEVER,
        TypeId::UNKNOWN,
        TypeId::ANY,
        TypeId::VOID,
        TypeId::UNDEFINED,
        TypeId::NULL,
        TypeId::BOOLEAN,
        TypeId::NUMBER,
        TypeId::STRING,
        TypeId::BIGINT,
        TypeId::SYMBOL,
        TypeId::OBJECT,
    ];

    for (i, a) in intrinsics.iter().enumerate() {
        for (j, b) in intrinsics.iter().enumerate() {
            if i != j {
                assert_ne!(
                    a, b,
                    "Intrinsic constants {:?} and {:?} should be unique",
                    a, b
                );
            }
        }
    }

    // Verify all intrinsics are below FIRST_USER threshold
    for id in &intrinsics {
        assert!(
            id.0 < TypeId::FIRST_USER,
            "Intrinsic {:?} should be below FIRST_USER",
            id
        );
    }
}

#[test]
fn test_ordered_float_equality() {
    // Same value
    assert_eq!(OrderedFloat(1.5), OrderedFloat(1.5));
    assert_eq!(OrderedFloat(-0.0), OrderedFloat(-0.0));
    assert_eq!(OrderedFloat(0.0), OrderedFloat(0.0));

    // Different values
    assert_ne!(OrderedFloat(1.5), OrderedFloat(2.5));
    assert_ne!(OrderedFloat(1.0), OrderedFloat(-1.0));

    // Note: 0.0 and -0.0 have different bit representations
    assert_ne!(OrderedFloat(0.0), OrderedFloat(-0.0));
}

#[test]
fn test_ordered_float_nan() {
    // NaN should equal itself (by bit comparison)
    let nan1 = OrderedFloat(f64::NAN);
    let nan2 = OrderedFloat(f64::NAN);
    assert_eq!(nan1, nan2);
}

#[test]
fn test_ordered_float_infinity() {
    let pos_inf = OrderedFloat(f64::INFINITY);
    let neg_inf = OrderedFloat(f64::NEG_INFINITY);

    assert_eq!(pos_inf, OrderedFloat(f64::INFINITY));
    assert_eq!(neg_inf, OrderedFloat(f64::NEG_INFINITY));
    assert_ne!(pos_inf, neg_inf);
}

// ============================================================================
// Template Literal Tests
// ============================================================================

#[test]
fn test_template_span_is_text() {
    let interner = crate::interner::ShardedInterner::new();
    let atom = interner.intern("hello");
    let text_span = TemplateSpan::Text(atom);
    assert!(text_span.is_text());
    assert!(!text_span.is_type());
}

#[test]
fn test_template_span_is_type() {
    let type_span = TemplateSpan::Type(TypeId::STRING);
    assert!(type_span.is_type());
    assert!(!type_span.is_text());
}

#[test]
fn test_template_span_as_text() {
    let interner = crate::interner::ShardedInterner::new();
    let atom = interner.intern("hello");
    let text_span = TemplateSpan::Text(atom);
    assert_eq!(text_span.as_text(), Some(atom));
    assert_eq!(text_span.as_type(), None);
}

#[test]
fn test_template_span_as_type() {
    let type_span = TemplateSpan::Type(TypeId::STRING);
    assert_eq!(type_span.as_type(), Some(TypeId::STRING));
    assert_eq!(type_span.as_text(), None);
}

#[test]
fn test_template_span_type_from_id() {
    let span = TemplateSpan::type_from_id(TypeId::NUMBER);
    assert!(span.is_type());
    assert_eq!(span.as_type(), Some(TypeId::NUMBER));
}

#[test]
fn test_process_template_escape_sequences_backslash_dollar() {
    // \${ should become $ (not an interpolation marker)
    let result = process_template_escape_sequences("\\${");
    assert_eq!(result, "${");
}

#[test]
fn test_process_template_escape_sequences_double_backslash() {
    let result = process_template_escape_sequences("\\\\");
    assert_eq!(result, "\\");
}

#[test]
fn test_process_template_escape_sequences_newline() {
    let result = process_template_escape_sequences("\\n");
    assert_eq!(result, "\n");
}

#[test]
fn test_process_template_escape_sequences_carriage_return() {
    let result = process_template_escape_sequences("\\r");
    assert_eq!(result, "\r");
}

#[test]
fn test_process_template_escape_sequences_tab() {
    let result = process_template_escape_sequences("\\t");
    assert_eq!(result, "\t");
}

#[test]
fn test_process_template_escape_sequences_backspace() {
    let result = process_template_escape_sequences("\\b");
    assert_eq!(result, "\x08");
}

#[test]
fn test_process_template_escape_sequences_form_feed() {
    let result = process_template_escape_sequences("\\f");
    assert_eq!(result, "\x0c");
}

#[test]
fn test_process_template_escape_sequences_vertical_tab() {
    let result = process_template_escape_sequences("\\v");
    assert_eq!(result, "\x0b");
}

#[test]
fn test_process_template_escape_sequences_null() {
    let result = process_template_escape_sequences("\\0");
    assert_eq!(result, "\0");
}

#[test]
fn test_process_template_escape_sequences_hex() {
    let result = process_template_escape_sequences("\\x41");
    assert_eq!(result, "A");
}

#[test]
fn test_process_template_escape_sequences_unicode_4_digit() {
    let result = process_template_escape_sequences("\\u0041");
    assert_eq!(result, "A");
}

#[test]
fn test_process_template_escape_sequences_unicode_braced() {
    let result = process_template_escape_sequences("\\u{41}");
    assert_eq!(result, "A");
}

#[test]
fn test_process_template_escape_sequences_unicode_emoji() {
    let result = process_template_escape_sequences("\\u{1F600}");
    assert_eq!(result, "😀");
}

#[test]
fn test_process_template_escape_sequences_mixed() {
    let result = process_template_escape_sequences("hello\\nworld\\t!");
    assert_eq!(result, "hello\nworld\t!");
}

#[test]
fn test_process_template_escape_sequences_unknown_escape() {
    // Unknown escape sequences should preserve the backslash
    let result = process_template_escape_sequences("\\z");
    assert_eq!(result, "\\z");
}

#[test]
fn test_process_template_escape_sequences_trailing_backslash() {
    let result = process_template_escape_sequences("abc\\");
    assert_eq!(result, "abc\\");
}

#[test]
fn test_process_template_escape_sequences_in_template_literal() {
    let result = process_template_escape_sequences("prefix-\\${string}-suffix");
    assert_eq!(result, "prefix-${string}-suffix");
}

#[test]
fn test_process_template_escape_sequences_empty_string() {
    let result = process_template_escape_sequences("");
    assert_eq!(result, "");
}

#[test]
fn test_process_template_escape_sequences_no_escapes() {
    let result = process_template_escape_sequences("hello world");
    assert_eq!(result, "hello world");
}

#[test]
fn test_process_template_escape_sequences_multiple_interpolation_markers() {
    let result = process_template_escape_sequences("\\$\\$\\$");
    assert_eq!(result, "$$$");
}
