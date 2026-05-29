//! Tests for the canonical DTS property-name quoting decision.
//!
//! The structural rule under test: when rendering an inferred object-literal
//! property name in a declaration file, the name is emitted bare iff it is a
//! syntactically valid identifier *and* not a reserved word, independent of
//! whether the source wrote it bare or quoted. When the name must remain
//! quoted, the original source quote character is preserved (a bare name forced
//! to quote falls back to double quotes).

use super::DeclarationEmitter;
use tsz_binder::BinderState;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, ParserState};
use tsz_solver::construction::TypeInterner;
use tsz_solver::{IndexSignature, ObjectFlags, ObjectShape, TypeId};

/// Collect every `COMPUTED_PROPERTY_NAME` node in source order.
fn computed_property_name_nodes(arena: &tsz_parser::parser::NodeArena) -> Vec<NodeIndex> {
    arena
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(idx, node)| {
            (node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME).then_some(NodeIndex(idx as u32))
        })
        .collect()
}

/// Index of the first object-literal expression node, in source order.
fn first_object_literal(arena: &tsz_parser::parser::NodeArena) -> Option<NodeIndex> {
    arena.nodes.iter().enumerate().find_map(|(idx, node)| {
        (node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION).then_some(NodeIndex(idx as u32))
    })
}

// --- can_emit_bare_property_name: the central bare-vs-quoted decision ---

#[test]
fn valid_identifiers_emit_bare_regardless_of_name_choice() {
    // Vary the spelling: plain, leading underscore, leading `$`, the
    // `__proto__` witness, and a mixed case. None are reserved words, so all
    // must be emittable bare.
    for name in ["foo", "_x", "$bar", "__proto__", "_proto__", "fooBar123"] {
        assert!(
            DeclarationEmitter::can_emit_bare_property_name(name),
            "expected `{name}` to be emittable bare",
        );
    }
}

#[test]
fn reserved_words_are_never_bare_regardless_of_keyword_choice() {
    // Vary the reserved word so the rule cannot be a single hardcoded match.
    for name in ["new", "function", "class", "return", "if", "void"] {
        assert!(
            !DeclarationEmitter::can_emit_bare_property_name(name),
            "expected reserved word `{name}` to require quoting",
        );
    }
}

#[test]
fn non_identifier_names_are_never_bare() {
    for name in ["foo bar", "0", "-1", "", "1abc", "a-b"] {
        assert!(
            !DeclarationEmitter::can_emit_bare_property_name(name),
            "expected non-identifier `{name}` to require quoting",
        );
    }
}

// --- format_property_name_literal_text: default (double-quote) rendering ---

#[test]
fn literal_text_quotes_reserved_words_with_double_quotes() {
    // A bare reserved word forced to quote uses double quotes (the `new` and
    // `function` witnesses) regardless of which reserved word it is.
    assert_eq!(
        DeclarationEmitter::format_property_name_literal_text("new"),
        "\"new\"",
    );
    assert_eq!(
        DeclarationEmitter::format_property_name_literal_text("function"),
        "\"function\"",
    );
}

#[test]
fn literal_text_canonicalizes_valid_identifiers_to_bare() {
    // A valid identifier (even the historically-quoted `__proto__`) renders
    // bare. Two different spellings prove this is not hardcoded.
    assert_eq!(
        DeclarationEmitter::format_property_name_literal_text("__proto__"),
        "__proto__",
    );
    assert_eq!(
        DeclarationEmitter::format_property_name_literal_text("___proto__"),
        "___proto__",
    );
}

#[test]
fn literal_text_quotes_non_identifiers_with_double_quotes() {
    assert_eq!(
        DeclarationEmitter::format_property_name_literal_text("foo bar"),
        "\"foo bar\"",
    );
    assert_eq!(
        DeclarationEmitter::format_property_name_literal_text("0"),
        "\"0\"",
    );
}

// --- format_property_name_with_quote: source quote-character preservation ---

#[test]
fn with_quote_preserves_single_quotes_when_quoting_is_required() {
    // `'foo bar'` and `'-1'` came from single-quoted source literals and must
    // stay single-quoted when they remain quoted.
    assert_eq!(
        DeclarationEmitter::format_property_name_with_quote("foo bar", "'"),
        "'foo bar'",
    );
    assert_eq!(
        DeclarationEmitter::format_property_name_with_quote("-1", "'"),
        "'-1'",
    );
}

#[test]
fn with_quote_preserves_double_quotes_when_quoting_is_required() {
    assert_eq!(
        DeclarationEmitter::format_property_name_with_quote("0", "\""),
        "\"0\"",
    );
    assert_eq!(
        DeclarationEmitter::format_property_name_with_quote("foo bar", "\""),
        "\"foo bar\"",
    );
}

#[test]
fn with_quote_emits_bare_even_when_a_quote_char_is_supplied() {
    // The quote character only matters when quoting is required. A valid,
    // non-reserved identifier renders bare even though the source quoted it
    // (the `"__proto__"` -> `__proto__` witness), for both quote characters.
    assert_eq!(
        DeclarationEmitter::format_property_name_with_quote("__proto__", "'"),
        "__proto__",
    );
    assert_eq!(
        DeclarationEmitter::format_property_name_with_quote("__proto__", "\""),
        "__proto__",
    );
}

#[test]
fn with_quote_reserved_word_quotes_with_supplied_char() {
    // A reserved word must be quoted; the supplied quote char is honored.
    assert_eq!(
        DeclarationEmitter::format_property_name_with_quote("new", "'"),
        "'new'",
    );
    assert_eq!(
        DeclarationEmitter::format_property_name_with_quote("new", "\""),
        "\"new\"",
    );
}

// --- is_non_nameable_computed_member_key: nameable-vs-index-signature gate ---
//
// Structural rule under test: a computed object-literal key is "nameable" (can
// be reproduced as a `.d.ts` property name) iff its expression is a string or
// numeric literal, a `+`/`-`-prefixed numeric literal, or an entity name that
// resolves to a single literal value / enum member. Everything else — a member
// access (`this.a`), a call, or a non-literal value reference — is
// non-nameable, and tsc represents it through a synthesized index signature.

#[test]
fn computed_member_access_keys_are_non_nameable_regardless_of_base_or_member_name() {
    // Vary both the base object and the accessed property so the rule cannot be
    // a single hardcoded spelling: `this.a`, `this.zzz`, and `obj.k` all share
    // the same non-nameable structure (a property-access computed key).
    for source in [
        "const v = { [this.a]: 1 };",
        "const v = { [this.zzz]: 1 };",
        "declare const obj: any; const v = { [obj.k]: 1 };",
    ] {
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file(&parser.arena, root);
        let interner = TypeInterner::new();
        let type_cache = crate::type_cache_view::TypeCacheView::default();
        let emitter =
            DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);

        let computed = computed_property_name_nodes(&parser.arena);
        assert_eq!(computed.len(), 1, "expected one computed key in `{source}`");
        assert!(
            emitter.is_non_nameable_computed_member_key(computed[0]),
            "expected computed member-access key in `{source}` to be non-nameable",
        );
    }
}

#[test]
fn computed_literal_keys_are_nameable() {
    // Control: string-literal, numeric-literal, and negative-numeric-literal
    // computed keys are all reproducible property names and must NOT be treated
    // as index signatures. Vary the literal spelling.
    for source in [
        "const v = { [\"name\"]: 1 };",
        "const v = { [42]: 1 };",
        "const v = { [-1]: 1 };",
    ] {
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file(&parser.arena, root);
        let interner = TypeInterner::new();
        let type_cache = crate::type_cache_view::TypeCacheView::default();
        let emitter =
            DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);

        let computed = computed_property_name_nodes(&parser.arena);
        assert_eq!(computed.len(), 1, "expected one computed key in `{source}`");
        assert!(
            !emitter.is_non_nameable_computed_member_key(computed[0]),
            "expected computed literal key in `{source}` to be nameable",
        );
    }
}

#[test]
fn bare_identifier_property_names_are_not_computed_and_stay_nameable() {
    // A bare (non-computed) property name is never a computed key, so the gate
    // must report nameable for it regardless of the chosen identifier.
    for source in ["const v = { a: 1 };", "const v = { longerName: 1 };"] {
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file(&parser.arena, root);
        let interner = TypeInterner::new();
        let type_cache = crate::type_cache_view::TypeCacheView::default();
        let emitter =
            DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);

        let object_idx = first_object_literal(&parser.arena).expect("object literal");
        let object_node = parser.arena.get(object_idx).expect("object node");
        let object = parser.arena.get_literal_expr(object_node).expect("literal");
        let member_idx = object.elements.nodes[0];
        let member_node = parser.arena.get(member_idx).expect("member node");
        let name_idx = emitter
            .object_literal_member_name_idx(member_node)
            .expect("member name");
        assert!(
            !emitter.is_non_nameable_computed_member_key(name_idx),
            "expected bare property name in `{source}` to be nameable",
        );
    }
}

// --- object_literal_synthesized_index_signature_entries: solver-derived sig ---

#[test]
fn synthesized_index_signature_uses_solver_key_and_value_types() {
    // When the object literal's solver type carries a number index signature,
    // the emitter renders it from the solver key/value types (not the source
    // slice). The default parameter name for a number key is `x`, matching tsc.
    let source = "const v = { [this.a]: \"\" };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
    let object_type = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::default(),
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        symbol: None,
    });

    let object_idx = first_object_literal(&parser.arena).expect("object literal");
    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache.node_types.insert(object_idx.0, object_type);
    let emitter = DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);

    let entries = emitter
        .object_literal_synthesized_index_signature_entries(object_idx)
        .expect("synthesized index signature entries");
    assert_eq!(entries, vec!["[x: number]: string".to_string()]);
}

#[test]
fn synthesized_string_index_signature_uses_key_param_name_and_readonly() {
    // A string index signature defaults to the `key` parameter name and honors
    // the readonly flag — keyed on the structural index info, not the spelling.
    let source = "const v = { [this.a]: 1 };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
    let object_type = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::default(),
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::BOOLEAN,
            readonly: true,
            param_name: None,
        }),
        number_index: None,
        symbol: None,
    });

    let object_idx = first_object_literal(&parser.arena).expect("object literal");
    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache.node_types.insert(object_idx.0, object_type);
    let emitter = DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);

    let entries = emitter
        .object_literal_synthesized_index_signature_entries(object_idx)
        .expect("synthesized index signature entries");
    assert_eq!(entries, vec!["readonly [key: string]: boolean".to_string()]);
}

#[test]
fn synthesized_index_signature_absent_without_solver_index_info() {
    // No index signature on the solver type => no synthesized entries; the
    // caller stays on its existing path.
    let source = "const v = { [this.a]: 1 };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
    let object_type = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::default(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
        symbol: None,
    });

    let object_idx = first_object_literal(&parser.arena).expect("object literal");
    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache.node_types.insert(object_idx.0, object_type);
    let emitter = DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);

    assert!(
        emitter
            .object_literal_synthesized_index_signature_entries(object_idx)
            .is_none(),
        "expected no synthesized index signature when solver type has none",
    );
}
