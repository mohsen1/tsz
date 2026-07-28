//! `is_ecmascript_entity_name` is tsz's equivalent of tsc's
//! `parseIsolatedEntityName`: it decides whether a user-supplied compiler-option
//! value (`jsxFactory`, `jsxFragmentFactory`, `reactNamespace`) names something
//! resolvable at all.
//!
//! Callers must treat `false` the way tsc treats `parseIsolatedEntityName`
//! returning `undefined` — resolve nothing, report nothing — leaving TS5067
//! from option validation as the only complaint. Getting this predicate wrong in
//! the permissive direction produces a spurious TS2304 at every JSX tag.

use tsz_scanner::{is_ecmascript_entity_name, is_ecmascript_identifier};

#[test]
fn plain_identifiers_are_entity_names() {
    for s in [
        "h",
        "React",
        "_",
        "$",
        "createElement",
        "_private",
        "$dollar",
        "a1",
    ] {
        assert!(is_ecmascript_identifier(s), "{s} should be an identifier");
        assert!(is_ecmascript_entity_name(s), "{s} should be an entity name");
    }
}

#[test]
fn dotted_qualified_names_are_entity_names() {
    for s in [
        "React.createElement",
        "a.b.c",
        "Element.createElement",
        "_.$.x",
    ] {
        assert!(is_ecmascript_entity_name(s), "{s} should be an entity name");
        assert!(
            !is_ecmascript_identifier(s),
            "{s} is qualified, not a bare identifier"
        );
    }
}

/// The case that produced the spurious TS2304: a space-separated pair has no
/// dot, so splitting on '.' yields the whole invalid string as the "root".
#[test]
fn space_separated_pair_is_not_an_entity_name() {
    assert!(!is_ecmascript_entity_name("id1 id2"));
    assert!(!is_ecmascript_identifier("id1 id2"));
}

#[test]
fn malformed_values_are_rejected() {
    for s in [
        "",      // empty
        ".",     // bare dot
        "a.",    // trailing dot -> empty segment
        ".a",    // leading dot -> empty segment
        "a..b",  // empty interior segment
        "1abc",  // starts with a digit
        "a-b",   // hyphen is not an identifier part
        "a b",   // space
        "a+b",   // operator
        "a.b c", // valid head, invalid tail
        "(a)",   // punctuation
        "a=>b",  // arrow
    ] {
        assert!(!is_ecmascript_entity_name(s), "{s:?} must be rejected");
    }
}

/// Unicode identifiers are legal in ECMAScript and must not be rejected — the
/// predicate delegates to the same tables the scanner uses.
#[test]
fn unicode_identifiers_are_accepted() {
    for s in ["café", "naïve", "Ωmega", "日本語", "_ø.café"] {
        assert!(is_ecmascript_entity_name(s), "{s} should be accepted");
    }
}
