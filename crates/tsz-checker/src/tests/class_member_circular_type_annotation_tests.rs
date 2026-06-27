//! Regression tests for #14819: a class property whose declared **type
//! annotation** references its own type — directly or indirectly — must emit
//! TS2502 (`'name' is referenced directly or indirectly in its own type
//! annotation`), exactly as `tsc` does. tsz already reported this for the
//! variable / interface / type-literal forms; class members were a silent
//! false negative.
//!
//! Detection is symbol/receiver gated: only a `typeof this.`/`typeof Class.`
//! (or `(typeof Class)[K]` / `Class[K]` / `this[K]`) reference whose receiver
//! resolves to the enclosing class counts. An unrelated `obj.member` access that
//! merely shares a name, a reference to a *different* member, or a deferred
//! reference behind a function type or nested type literal must stay clean.
//!
//! Binder names are varied across cases (anti-hardcoding): the logic keys off
//! structure (receiver + resolved member / symbol), never a specific identifier.

use crate::test_utils::{check_source_strict_codes, check_source_strict_messages};

fn ts2502_count(src: &str) -> usize {
    check_source_strict_codes(src)
        .into_iter()
        .filter(|&c| c == 2502)
        .count()
}

fn assert_no_ts2502(src: &str) {
    let codes = check_source_strict_codes(src);
    assert!(
        !codes.contains(&2502),
        "expected no TS2502, got: {codes:?}\n{src}"
    );
}

// ---------------------------------------------------------------------------
// The reported witnesses (#14819).
// ---------------------------------------------------------------------------

#[test]
fn static_string_keyed_typeof_self_is_ts2502() {
    // `static x: typeof D.x;` — string-keyed static, dotted typeof query.
    assert_eq!(ts2502_count("class Down { static x: typeof Down.x; }"), 1);
}

#[test]
fn instance_typeof_this_self_is_ts2502() {
    // `x: typeof this.x;` — instance member via `this`.
    assert_eq!(ts2502_count("class Echo { x: typeof this.x; }"), 1);
}

#[test]
fn static_symbol_keyed_indexed_typeof_self_is_ts2502() {
    // The axis witness: `static [s]: typeof C[typeof s];` — symbol-keyed static,
    // `(typeof Class)[typeof s]` indexed access whose key is the same symbol.
    let src = "declare const s: unique symbol;\n\
               class Cy { static [s]: typeof Cy[typeof s]; }";
    assert_eq!(ts2502_count(src), 1);
}

#[test]
fn static_readonly_with_initializer_self_is_ts2502() {
    // readonly + an initializer must not suppress the circularity diagnostic.
    assert_eq!(
        ts2502_count("class Foxtrot { static readonly y: typeof Foxtrot.y = 0 as any; }"),
        1,
    );
}

#[test]
fn indirect_two_member_cycle_reports_both() {
    // `static a: typeof C.b; static b: typeof C.a;` — an indirect cycle flags
    // every member on it, exactly as `tsc` does.
    assert_eq!(
        ts2502_count("class Golf { static a: typeof Golf.b; static b: typeof Golf.a; }"),
        2,
    );
}

// ---------------------------------------------------------------------------
// The symbol-keyed witness renders the computed name as `[s]` (matches tsc).
// ---------------------------------------------------------------------------

#[test]
fn symbol_keyed_member_message_renders_bracketed_name() {
    let src = "declare const sym: unique symbol;\n\
               class Hotel { static [sym]: typeof Hotel[typeof sym]; }";
    let msg = check_source_strict_messages(src)
        .into_iter()
        .find(|(code, _)| *code == 2502)
        .map(|(_, msg)| msg)
        .expect("expected a TS2502 diagnostic");
    assert!(
        msg.contains("'[sym]'"),
        "TS2502 should name the computed member as '[sym]', got: {msg}"
    );
}

// ---------------------------------------------------------------------------
// Adjacent positive forms (tsz now matches tsc).
// ---------------------------------------------------------------------------

#[test]
fn instance_indexed_access_self_is_ts2502() {
    // Instance side via the class type reference: `item: Store["item"];`.
    assert_eq!(ts2502_count("class Store { item: Store[\"item\"]; }"), 1);
}

#[test]
fn static_parenthesized_typeof_indexed_self_is_ts2502() {
    // Parens around the object type must not hide the self-reference.
    assert_eq!(
        ts2502_count("class Igloo { static item: (typeof Igloo)[\"item\"]; }"),
        1,
    );
}

#[test]
fn self_reference_behind_array_wrapper_is_ts2502() {
    // `typeof C.x[]` is an array of the member's own type — still circular.
    assert_eq!(
        ts2502_count("class Juliet { static x: typeof Juliet.x[]; }"),
        1
    );
}

#[test]
fn self_reference_inside_union_is_ts2502() {
    assert_eq!(
        ts2502_count("class Kilo { static x: typeof Kilo.x | number; }"),
        1,
    );
}

// ---------------------------------------------------------------------------
// Negatives — must NOT fire TS2502.
// ---------------------------------------------------------------------------

#[test]
fn reference_to_different_member_is_clean() {
    // `static p: typeof C.q;` where `q` is a distinct, non-circular member.
    assert_no_ts2502("class Lima { static p: typeof Lima.q; static q: number = 1; }");
}

#[test]
fn unrelated_receiver_with_colliding_name_is_clean() {
    // `typeof other.x` reads an unrelated symbol's `x`, not the member.
    assert_no_ts2502(
        "declare const other: { x: number };\n\
         class Mike { x: typeof other.x; }",
    );
}

#[test]
fn deferred_behind_function_type_is_clean() {
    // A function type is a safe recursion boundary.
    assert_no_ts2502("class November { static x: () => typeof November.x; }");
}

#[test]
fn deferred_inside_nested_type_literal_is_clean() {
    // A nested object type literal defers member resolution (lazy boundary).
    assert_no_ts2502("class Oscar { static x: { y: typeof Oscar.x }; }");
}

#[test]
fn reference_to_other_class_member_is_clean() {
    assert_no_ts2502(
        "class Papa { static v: number = 1; }\n\
         class Quebec { static w: typeof Papa.v; }",
    );
}

#[test]
fn instance_vs_static_mismatch_is_clean() {
    // Instance member referenced through the static side resolves to a
    // different (here absent) member — no self-reference.
    assert_no_ts2502("class Romeo { foo: number = 1; static bar: typeof Romeo.foo; }");
}

#[test]
fn inherited_member_name_is_not_a_self_reference() {
    // `this.x` resolves to the inherited member, not a member declared on the
    // subclass, so the subclass property is not circular.
    assert_no_ts2502(
        "class Sierra { x: number = 1; }\n\
         class Tango extends Sierra { y: typeof this.x; }",
    );
}
