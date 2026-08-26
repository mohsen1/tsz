use super::*;

#[test]
fn utf16_string_nonclaims_are_identity_free_and_not_interned() {
    let mut store = TypeStore::default();
    let before = store.len();
    let first = store.deferred_utf16_string_literal();
    let second = store.deferred_utf16_string_literal();
    assert_ne!(first, second);
    assert_eq!(store.len(), before + 2);
    assert!(matches!(
        store.kind(first),
        TypeKind::Deferred(DeferredType::Utf16StringLiteral)
    ));
    assert!(matches!(
        store.kind(second),
        TypeKind::Deferred(DeferredType::Utf16StringLiteral)
    ));
    assert_eq!(store.widened_literal_type(first), store.builtins.string);
}

#[test]
fn numeric_recovery_nonclaims_are_identity_free_and_not_interned() {
    let mut store = TypeStore::default();
    let before = store.len();
    let first = store.deferred_numeric_recovery();
    let second = store.deferred_numeric_recovery();
    assert_ne!(first, second);
    assert_eq!(store.len(), before + 2);
    assert!(matches!(
        store.kind(first),
        TypeKind::Deferred(DeferredType::NumericRecovery)
    ));
    assert!(matches!(
        store.kind(second),
        TypeKind::Deferred(DeferredType::NumericRecovery)
    ));
}

fn literal_array(store: &mut TypeStore, value: &str) -> TypeId {
    let literal = store.intern(TypeKind::LiteralString(
        value.to_string(),
        LiteralProvenance::Regular,
    ));
    store.intern(TypeKind::Array(literal))
}

#[test]
fn string_literal_display_escapes_diagnostic_line_breaks_and_delimiters() {
    let mut store = TypeStore::new();
    let literal = store.intern(TypeKind::LiteralString(
        "line\nbreak\r\t\"quoted\"\\tail".to_string(),
        LiteralProvenance::Regular,
    ));
    assert_eq!(
        store.display(literal),
        "\"line\\nbreak\\r\\t\\\"quoted\\\"\\\\tail\"",
    );
}

#[test]
fn string_literal_display_matches_typescript_escape_string_edges() {
    let mut store = TypeStore::new();
    let literal = store.intern(TypeKind::LiteralString(
        "\0x\u{0}7\u{8}\u{b}\u{c}\u{e}\u{1f}\u{85}\u{2028}\u{2029}\u{7f}\\\"'é😀".to_string(),
        LiteralProvenance::Regular,
    ));
    assert_eq!(
        store.display(literal),
        "\"\\0x\\x007\\b\\v\\f\\u000E\\u001F\\u0085\\u2028\\u2029\u{7f}\\\\\\\"'é😀\"",
    );
}

#[test]
fn union_order_follows_typed_structure_not_allocation_or_input_order() {
    let mut reverse_allocation = TypeStore::new();
    let reverse_b = literal_array(&mut reverse_allocation, "b");
    let reverse_a = literal_array(&mut reverse_allocation, "a");
    let reverse_union = reverse_allocation.union([reverse_b, reverse_a], UnionPolicy::Canonical);

    let mut forward_allocation = TypeStore::new();
    let forward_a = literal_array(&mut forward_allocation, "a");
    let forward_b = literal_array(&mut forward_allocation, "b");
    let forward_union = forward_allocation.union([forward_b, forward_a], UnionPolicy::Canonical);

    assert_eq!(
        reverse_allocation.display(reverse_union),
        "\"a\"[] | \"b\"[]"
    );
    assert_eq!(
        reverse_allocation.display(reverse_union),
        forward_allocation.display(forward_union)
    );
}

fn deferred_conditional(store: &mut TypeStore, value: &str) -> TypeId {
    let check = literal_array(store, value);
    store.intern(TypeKind::Deferred(DeferredType::Conditional {
        check,
        extends: store.builtins.string,
        when_true: store.builtins.number,
        when_false: store.builtins.boolean,
    }))
}

#[test]
fn deferred_union_order_preserves_variant_field_and_nested_type_order() {
    let mut reverse_allocation = TypeStore::new();
    let reverse_b = deferred_conditional(&mut reverse_allocation, "b");
    let reverse_a = deferred_conditional(&mut reverse_allocation, "a");
    let reverse_union = reverse_allocation.union([reverse_b, reverse_a], UnionPolicy::Canonical);

    let mut forward_allocation = TypeStore::new();
    let forward_a = deferred_conditional(&mut forward_allocation, "a");
    let forward_b = deferred_conditional(&mut forward_allocation, "b");
    let forward_union = forward_allocation.union([forward_b, forward_a], UnionPolicy::Canonical);

    assert_eq!(
        reverse_allocation.display(reverse_union),
        "\"a\"[] extends string ? number : boolean | \"b\"[] extends string ? number : boolean"
    );
    assert_eq!(
        reverse_allocation.display(reverse_union),
        forward_allocation.display(forward_union)
    );
}

#[test]
fn construct_query_identity_and_traversal_include_argument_types() {
    let mut store = TypeStore::new();
    let callee = store.builtins.any;
    let string = store.builtins.string;
    let number = store.builtins.number;
    let never_array = store.intern(TypeKind::Array(store.builtins.never));
    let construct = |store: &mut TypeStore, argument| {
        store.intern(TypeKind::Deferred(DeferredType::Construct {
            callee,
            type_arguments: vec![string, number],
            arguments: vec![argument],
        }))
    };
    let valid = construct(&mut store, never_array);
    let invalid = construct(&mut store, number);
    assert_ne!(valid, invalid);

    let TypeKind::Deferred(query) = store.kind(valid) else {
        panic!("expected deferred construct query");
    };
    let mut children = Vec::new();
    TypeStore::push_deferred_children(query, &mut children);
    assert_eq!(children, [callee, string, number, never_array]);
}

#[test]
fn canonical_union_reduces_literal_families_and_dominant_members() {
    let mut store = TypeStore::new();
    let string_literal = store.intern(TypeKind::LiteralString(
        "value".to_string(),
        LiteralProvenance::Regular,
    ));
    let true_literal = store.intern(TypeKind::LiteralBoolean(true, LiteralProvenance::Regular));
    let false_literal = store.intern(TypeKind::LiteralBoolean(false, LiteralProvenance::Regular));
    let never = store.builtins.never;
    let string = store.builtins.string;
    let boolean = store.builtins.boolean;
    let any = store.builtins.any;
    let unknown = store.builtins.unknown;

    assert_eq!(
        store.union([never, string_literal], UnionPolicy::Canonical),
        string_literal
    );
    assert_eq!(
        store.union([string_literal, string], UnionPolicy::Canonical),
        string
    );
    assert_eq!(
        store.union([true_literal, false_literal], UnionPolicy::Canonical),
        boolean
    );
    assert_eq!(
        store.union([string_literal, any], UnionPolicy::Canonical),
        any
    );
    assert_eq!(store.union([any, unknown], UnionPolicy::Canonical), any);
}

#[test]
fn numeric_order_is_value_order_and_authored_structural_order_is_explicit() {
    let mut store = TypeStore::new();
    let ten = store.numeric_literal("10", LiteralProvenance::Regular);
    let two = store.numeric_literal("2", LiteralProvenance::Regular);
    let numeric = store.union([ten, two], UnionPolicy::Canonical);
    assert_eq!(store.display(numeric), "2 | 10");

    let exponent = store.numeric_literal("1e3", LiteralProvenance::Regular);
    let unsafe_integer = store.numeric_literal("9007199254740993", LiteralProvenance::Regular);
    let rounded_integer = store.numeric_literal("9007199254740992", LiteralProvenance::Regular);
    assert_eq!(store.display(exponent), "1000");
    assert_eq!(unsafe_integer, rounded_integer);

    let format_edges = [
        ("0.1", "0.1"),
        ("0.0001", "0.0001"),
        ("1.25", "1.25"),
        ("1e-7", "1e-7"),
        ("1e-6", "0.000001"),
        ("1e20", "100000000000000000000"),
        ("1e21", "1e+21"),
        ("1000000000000000000001", "1e+21"),
    ];
    for (source, expected) in format_edges {
        let literal = store.numeric_literal(source, LiteralProvenance::Regular);
        assert_eq!(store.display(literal), expected, "source: {source}");
    }

    let radix_edges = [
        ("0x20000000000001", "9007199254740992"),
        ("0x20000000000000", "9007199254740992"),
        ("0b1010", "10"),
        ("0o12", "10"),
        ("0x10000000000000000", "18446744073709552000"),
        ("0x20000000000000000", "36893488147419103000"),
        ("0xfffffffffffffffff", "295147905179352830000"),
    ];
    for (source, expected) in radix_edges {
        let literal = store.numeric_literal(source, LiteralProvenance::Regular);
        assert_eq!(store.display(literal), expected, "source: {source}");
    }

    let before_invalid = store.len();
    assert!(
        store
            .try_numeric_literal("not-a-number", LiteralProvenance::Regular)
            .is_err()
    );
    assert_eq!(
        store.numeric_literal("not-a-number", LiteralProvenance::Regular),
        store.builtins.error
    );
    assert_eq!(store.len(), before_invalid);

    let string = store.builtins.string;
    let left = store.object(vec![Property {
        name: "left".to_string(),
        ty: string,
        optional: false,
        readonly: false,
    }]);
    let right = store.object(vec![Property {
        name: "right".to_string(),
        ty: string,
        optional: false,
        readonly: false,
    }]);
    let authored = store.union(
        [right, left, right],
        UnionPolicy::PreserveAuthoredStructuralOrder,
    );
    assert_eq!(
        store.display(authored),
        "{ right: string; } | { left: string; }"
    );
}
