use super::*;
use crate::TypeInterner;

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
                assert_ne!(a, b, "Intrinsic constants {a:?} and {b:?} should be unique");
            }
        }
    }

    // Verify all intrinsics are below FIRST_USER threshold
    for id in &intrinsics {
        assert!(
            id.0 < TypeId::FIRST_USER,
            "Intrinsic {id:?} should be below FIRST_USER"
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

#[test]
fn test_normalize_display_property_order_preserves_declaration_sequence() {
    let interner = tsz_common::interner::ShardedInterner::new();
    let default_name = interner.intern("default");
    let configs_name = interner.intern("configs");

    let mut props = vec![
        PropertyInfo {
            name: configs_name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            declaration_order: 2,
            ..PropertyInfo::new(configs_name, TypeId::STRING)
        },
        PropertyInfo {
            name: default_name,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            declaration_order: 1,
            ..PropertyInfo::new(default_name, TypeId::NUMBER)
        },
    ];

    normalize_display_property_order(&mut props);

    assert_eq!(props[0].name, default_name);
    assert_eq!(props[1].name, configs_name);
    assert_eq!(props[0].declaration_order, 1);
    assert_eq!(props[1].declaration_order, 2);
}

#[test]
fn test_normalize_display_property_order_prioritizes_explicit_order_before_unset_members() {
    let interner = tsz_common::interner::ShardedInterner::new();
    let default_name = interner.intern("default");
    let configs_name = interner.intern("configs");

    let mut props = vec![
        PropertyInfo {
            name: configs_name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            declaration_order: 0,
            ..PropertyInfo::new(configs_name, TypeId::STRING)
        },
        PropertyInfo {
            name: default_name,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            declaration_order: 1,
            ..PropertyInfo::new(default_name, TypeId::NUMBER)
        },
    ];

    normalize_display_property_order(&mut props);

    assert_eq!(props[0].name, default_name);
    assert_eq!(props[1].name, configs_name);
    assert_eq!(props[0].declaration_order, 1);
    assert_eq!(props[1].declaration_order, 2);
}

#[test]
fn test_merge_display_properties_for_intersection_preserves_first_seen_order() {
    let interner = TypeInterner::new();
    let default_name = interner.intern_string("default");
    let configs_name = interner.intern_string("configs");

    let mk_prop = |name, type_id, declaration_order| PropertyInfo {
        name,
        type_id,
        write_type: type_id,
        declaration_order,
        ..PropertyInfo::new(name, type_id)
    };

    let left = interner.object(vec![
        mk_prop(configs_name, TypeId::STRING, 2),
        mk_prop(default_name, TypeId::NUMBER, 1),
    ]);
    interner.store_display_properties(
        left,
        vec![
            mk_prop(default_name, TypeId::NUMBER, 1),
            mk_prop(configs_name, TypeId::STRING, 2),
        ],
    );

    let right = interner.object(vec![
        mk_prop(configs_name, TypeId::STRING, 2),
        mk_prop(default_name, TypeId::NUMBER, 1),
    ]);
    interner.store_display_properties(
        right,
        vec![
            mk_prop(default_name, TypeId::NUMBER, 1),
            mk_prop(configs_name, TypeId::STRING, 2),
        ],
    );

    let merged = merge_display_properties_for_intersection(&interner, &[left, right]);

    assert_eq!(merged.len(), 2);
    assert_eq!(merged[0].name, default_name);
    assert_eq!(merged[1].name, configs_name);
    assert_eq!(merged[0].declaration_order, 1);
    assert_eq!(merged[1].declaration_order, 2);
}

#[test]
fn test_merge_display_properties_for_intersection_keeps_left_to_right_member_sequence() {
    let interner = TypeInterner::new();
    let b_name = interner.intern_string("b");
    let c_name = interner.intern_string("c");
    let a_name = interner.intern_string("a");

    let mk_prop = |name, type_id, declaration_order| PropertyInfo {
        name,
        type_id,
        write_type: type_id,
        declaration_order,
        ..PropertyInfo::new(name, type_id)
    };

    let left = interner.object(vec![
        mk_prop(b_name, TypeId::STRING, 2),
        mk_prop(c_name, TypeId::BOOLEAN, 3),
    ]);
    interner.store_display_properties(
        left,
        vec![
            mk_prop(b_name, TypeId::STRING, 2),
            mk_prop(c_name, TypeId::BOOLEAN, 3),
        ],
    );

    let right = interner.object(vec![mk_prop(a_name, TypeId::NUMBER, 1)]);
    interner.store_display_properties(right, vec![mk_prop(a_name, TypeId::NUMBER, 1)]);

    let merged = merge_display_properties_for_intersection(&interner, &[left, right]);

    assert_eq!(merged.len(), 3);
    assert_eq!(merged[0].name, b_name);
    assert_eq!(merged[1].name, c_name);
    assert_eq!(merged[2].name, a_name);
    assert_eq!(merged[0].declaration_order, 1);
    assert_eq!(merged[1].declaration_order, 2);
    assert_eq!(merged[2].declaration_order, 3);
}

/// Regression: `interner.object(...)` sorts properties by atom for
/// canonical interning, but `declaration_order` is captured before that
/// sort. Sorting the canonical shape's properties by `declaration_order`
/// must therefore recover source declaration order — the invariant the
/// JSX synthesized-source-type display in
/// `crates/tsz-checker/src/checkers/jsx/{spread.rs,props/resolution.rs}`
/// relies on to mirror tsc's per-attribute ordering.
///
/// Atom IDs come from a sharded interner whose shard index is hash-derived,
/// so insertion order does NOT determine atom order across two strings in
/// different shards. This test picks the source-declaration order based on
/// the interner's actual atom layout, then asserts:
///   1. `shape.properties` is in atom order (canonical interning).
///   2. Sorting `shape.properties` by `declaration_order` recovers the
///      source order we asked for.
#[test]
fn shape_properties_atom_sorted_yet_recover_source_order_via_declaration_order() {
    let interner = TypeInterner::new();

    let y_name = interner.intern_string("y");
    let x_name = interner.intern_string("x");

    // Build the object so the property with the LARGER atom is declared
    // first. With that setup, `shape.properties` (atom-sorted) lists the
    // declared-second property first — i.e., the buggy order — and only a
    // declaration-order sort recovers the source order.
    let (first_name, second_name) = if x_name.0 > y_name.0 {
        (x_name, y_name)
    } else {
        (y_name, x_name)
    };
    let mk = |name, type_id, declaration_order| PropertyInfo {
        name,
        type_id,
        write_type: type_id,
        declaration_order,
        ..PropertyInfo::new(name, type_id)
    };
    let obj = interner.object(vec![
        mk(first_name, TypeId::STRING, 1),
        mk(second_name, TypeId::STRING, 2),
    ]);

    let shape = match interner.lookup(obj) {
        Some(TypeData::Object(shape_id)) => interner.object_shape(shape_id),
        other => panic!("expected object, got {other:?}"),
    };
    assert_ne!(
        first_name.0, second_name.0,
        "test setup: distinct property names"
    );
    let canonical_order: Vec<_> = shape.properties.iter().map(|p| p.name).collect();
    let expected_canonical = if first_name.0 < second_name.0 {
        vec![first_name, second_name]
    } else {
        vec![second_name, first_name]
    };
    assert_eq!(
        canonical_order, expected_canonical,
        "shape.properties is atom-sorted (canonical interning)"
    );
    assert_ne!(
        canonical_order,
        vec![first_name, second_name],
        "test setup must put the buggy atom-order at variance with source order"
    );

    let mut by_decl: Vec<&PropertyInfo> = shape.properties.iter().collect();
    by_decl.sort_by_key(|p| p.declaration_order);
    let display_order: Vec<_> = by_decl.iter().map(|p| p.name).collect();
    assert_eq!(
        display_order,
        vec![first_name, second_name],
        "sorting shape.properties by declaration_order recovers source declaration order"
    );
}

// ============================================================================
// Template Literal Tests
// ============================================================================

#[test]
fn test_template_span_is_text() {
    let interner = tsz_common::interner::ShardedInterner::new();
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
    let interner = tsz_common::interner::ShardedInterner::new();
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
