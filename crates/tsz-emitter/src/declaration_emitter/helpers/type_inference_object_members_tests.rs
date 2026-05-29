//! Tests for the canonical DTS property-name quoting decision.
//!
//! The structural rule under test: when rendering an inferred object-literal
//! property name in a declaration file, the name is emitted bare iff it is a
//! syntactically valid identifier *and* not a reserved word, independent of
//! whether the source wrote it bare or quoted. When the name must remain
//! quoted, the original source quote character is preserved (a bare name forced
//! to quote falls back to double quotes).

use super::DeclarationEmitter;

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
