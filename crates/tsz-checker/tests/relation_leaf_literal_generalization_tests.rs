//! Literal-source generalization at nested relation leaves (`tsc`'s
//! `reportRelationError`), issue #15626.
//!
//! When the failing source of a nested `Type 'S' is not assignable to type
//! 'T'.` relation line is a literal type and the target could not hold a
//! top-level singleton type, `tsc` displays the literal's base type
//! (`"no"` -> `string`, `true` -> `boolean`, `E.X` -> `E`). When the target is
//! singleton-capable (a literal, a literal union, a template literal, or a
//! type parameter constrained to one of those), the literal is preserved so
//! the literal-vs-literal comparison stays meaningful.
//!
//! The rule is a property of relation-error display, not of expression
//! freshness: a *variable* whose declared tuple element is `"no"` renders the
//! same generalized leaf as a fresh array-literal argument. Cases below cover
//! the tuple positional chain (both TS2345 call-argument and TS2322
//! initializer entry points), property chains, array element chains, return
//! leaves, and the preservation gates. Binder names vary across cases so the
//! behavior is proven structural.

use tsz_checker::context::CheckerOptions;
use tsz_common::diagnostics::Diagnostic;

fn check_strict(source: &str) -> Vec<Diagnostic> {
    let options = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..Default::default()
    };
    tsz_checker::test_utils::check_source(source, "test.ts", options)
}

fn one(diags: &[Diagnostic], code: u32) -> &Diagnostic {
    let matches: Vec<&Diagnostic> = diags.iter().filter(|d| d.code == code).collect();
    assert_eq!(
        matches.len(),
        1,
        "expected exactly one TS{code}, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
    matches[0]
}

fn assert_has_related(diag: &Diagnostic, expected: &str) {
    assert!(
        diag.related_information
            .iter()
            .any(|r| r.message_text == expected),
        "expected related line {expected:?}; related: {:?}",
        diag.related_information
            .iter()
            .map(|r| &r.message_text)
            .collect::<Vec<_>>()
    );
}

fn assert_no_related(diag: &Diagnostic, unexpected: &str) {
    assert!(
        diag.related_information
            .iter()
            .all(|r| r.message_text != unexpected),
        "unexpected related line {unexpected:?}; related: {:?}",
        diag.related_information
            .iter()
            .map(|r| &r.message_text)
            .collect::<Vec<_>>()
    );
}

/// The #15626 repro: a fresh array-literal call argument against a middle-rest
/// tuple parameter renders the widened element in the positional-chain leaf.
#[test]
fn call_argument_middle_rest_tuple_chain_leaf_widens_fresh_literal() {
    let source = r#"
function grab(items: [number, ...boolean[], string]) {}
grab([1, "no", "s"]);
"#;
    let diags = check_strict(source);
    let diag = one(&diags, 2345);
    assert_has_related(
        diag,
        "Type at position 1 in source is not compatible with type at position 1 in target.",
    );
    assert_has_related(diag, "Type 'string' is not assignable to type 'boolean'.");
    assert_no_related(diag, "Type '\"no\"' is not assignable to type 'boolean'.");
}

/// Initializer entry point (already correct before the fix): the same chain
/// leaf renders the widened element.
#[test]
fn initializer_middle_rest_tuple_chain_leaf_widens_fresh_literal() {
    let source = r#"
const slots: [number, ...boolean[], string] = [1, "no", "s"];
"#;
    let diags = check_strict(source);
    let diag = one(&diags, 2322);
    assert_has_related(diag, "Type 'string' is not assignable to type 'boolean'.");
    assert_no_related(diag, "Type '\"no\"' is not assignable to type 'boolean'.");
}

/// The generalization is a relation-display rule, not a freshness rule: a
/// variable whose *declared* tuple element is the literal renders the same
/// generalized leaf (verified against `tsc 6.0.2`).
#[test]
fn declared_tuple_variable_source_chain_leaf_also_generalizes() {
    let source = r#"
declare const packed: [number, "no", string];
function feed(cells: [number, ...boolean[], string]) {}
feed(packed);
"#;
    let diags = check_strict(source);
    let diag = one(&diags, 2345);
    assert_has_related(diag, "Type 'string' is not assignable to type 'boolean'.");
    assert_no_related(diag, "Type '\"no\"' is not assignable to type 'boolean'.");
}

/// Multi-element span aligned to a middle rest slot: the plural positional
/// line keeps its shape and the leaf generalizes.
#[test]
fn variadic_span_chain_leaf_generalizes() {
    let source = r#"
function quilt(zz: [string, ...number[], boolean]) {}
quilt(["a", "b", "c", true]);
"#;
    let diags = check_strict(source);
    let diag = one(&diags, 2345);
    assert_has_related(
        diag,
        "Type at positions 1 through 2 in source is not compatible with type at position 1 in target.",
    );
    assert_has_related(diag, "Type 'string' is not assignable to type 'number'.");
}

/// Preservation gate: a literal-typed rest slot is singleton-capable, so the
/// source literal survives in the leaf.
#[test]
fn literal_rest_slot_preserves_source_literal() {
    let source = r#"
declare const duo: [number, "no"];
function pin(x: [number, ..."yes"[]]) {}
pin(duo);
"#;
    let diags = check_strict(source);
    let diag = one(&diags, 2345);
    assert_has_related(diag, "Type '\"no\"' is not assignable to type '\"yes\"'.");
    assert_no_related(diag, "Type 'string' is not assignable to type '\"yes\"'.");
}

/// Property-chain leaf (TS2322): a declared string-literal property widens
/// against a non-singleton target property.
#[test]
fn property_chain_leaf_generalizes_string_literal() {
    let source = r#"
declare const wrap: { flag: "no" };
const sink: { flag: boolean } = wrap;
"#;
    let diags = check_strict(source);
    let diag = one(&diags, 2322);
    assert_has_related(diag, "Type 'string' is not assignable to type 'boolean'.");
    assert_no_related(diag, "Type '\"no\"' is not assignable to type 'boolean'.");
}

/// Property-chain leaf (TS2345 hand-rolled related-info arm): same rule on the
/// call-argument surface, including through a type-parameter target whose
/// constraint is a plain primitive.
#[test]
fn call_argument_property_leaf_generalizes_through_primitive_constraint() {
    let source = r#"
function lodge<W extends boolean>(x: { knob: W }) {}
declare const dial: { knob: "no" };
lodge(dial);
"#;
    let diags = check_strict(source);
    let diag = one(&diags, 2345);
    assert_has_related(diag, "Type 'string' is not assignable to type 'boolean'.");
}

/// Preservation gate: a type parameter constrained to a literal union is
/// singleton-capable, so the source literal survives.
#[test]
fn singleton_constrained_type_parameter_preserves_literal() {
    let source = r#"
function tune<Q extends "aa" | "bb">(x: { pitch: Q }) {}
declare const chord: { pitch: "no" };
tune(chord);
"#;
    let diags = check_strict(source);
    let diag = one(&diags, 2345);
    assert_has_related(
        diag,
        "Type '\"no\"' is not assignable to type '\"aa\" | \"bb\"'.",
    );
}

/// Collapsed dotted property chain (`a.b`) leaf generalizes a number literal.
#[test]
fn dotted_property_chain_leaf_generalizes_number_literal() {
    let source = r#"
declare const deep: { a: { b: 5 } };
const basin: { a: { b: string } } = deep;
"#;
    let diags = check_strict(source);
    let diag = one(&diags, 2322);
    assert_has_related(diag, "Type 'number' is not assignable to type 'string'.");
    assert_no_related(diag, "Type '5' is not assignable to type 'string'.");
}

/// Array element chain leaf generalizes.
#[test]
fn array_element_chain_leaf_generalizes() {
    let source = r#"
declare const noes: "no"[];
const bools: boolean[] = noes;
"#;
    let diags = check_strict(source);
    let diag = one(&diags, 2322);
    assert_has_related(diag, "Type 'string' is not assignable to type 'boolean'.");
    assert_no_related(diag, "Type '\"no\"' is not assignable to type 'boolean'.");
}

/// Boolean and bigint literal property leaves generalize to their bases.
#[test]
fn boolean_and_bigint_literal_property_leaves_generalize() {
    let source = r#"
declare const lit: { on: true };
const strung: { on: string } = lit;
declare const big: { n: 1n };
const booled: { n: boolean } = big;
"#;
    let diags = check_strict(source);
    let texts: Vec<&str> = diags
        .iter()
        .flat_map(|d| d.related_information.iter())
        .map(|r| r.message_text.as_str())
        .collect();
    assert!(
        texts.contains(&"Type 'boolean' is not assignable to type 'string'."),
        "boolean literal leaf must widen; related: {texts:?}"
    );
    assert!(
        texts.contains(&"Type 'bigint' is not assignable to type 'boolean'."),
        "bigint literal leaf must widen; related: {texts:?}"
    );
}

/// Preservation gate: a union target containing a same-base literal is
/// singleton-capable, so the number literal survives.
#[test]
fn union_target_with_singleton_preserves_number_literal() {
    let source = r#"
declare const five: { a: 5 };
const mixed: { a: 6 | string } = five;
"#;
    let diags = check_strict(source);
    let diag = one(&diags, 2322);
    assert_has_related(diag, "Type '5' is not assignable to type 'string | 6'.");
}

/// Enum members generalize to their parent enum in the positional-chain leaf
/// (tsc `getBaseTypeOfLiteralType` EnumLike branch).
#[test]
fn enum_member_tuple_chain_leaf_widens_to_parent_enum() {
    let source = r#"
enum Gear { Low = 1 }
declare const kit: [number, Gear.Low, string];
function stow(x: [number, ...string[], string]) {}
stow(kit);
"#;
    let diags = check_strict(source);
    let diag = one(&diags, 2345);
    assert_has_related(diag, "Type 'Gear' is not assignable to type 'string'.");
}

/// Callback return leaf under TS2345 generalizes the literal return type.
#[test]
fn callback_return_leaf_generalizes() {
    let source = r#"
declare function pump(cb: () => boolean): void;
declare const feedCb: () => "no";
pump(feedCb);
"#;
    let diags = check_strict(source);
    let diag = one(&diags, 2345);
    assert_has_related(diag, "Type 'string' is not assignable to type 'boolean'.");
    assert_no_related(diag, "Type '\"no\"' is not assignable to type 'boolean'.");
}

/// Member-return frame (`The types returned by 'm()' ...`) leaf generalizes.
#[test]
fn member_return_frame_leaf_generalizes() {
    let source = r#"
declare const gadget: { spin: () => "no" };
const port: { spin: () => boolean } = gadget;
"#;
    let diags = check_strict(source);
    let diag = one(&diags, 2322);
    assert_has_related(diag, "Type 'string' is not assignable to type 'boolean'.");
    assert_no_related(diag, "Type '\"no\"' is not assignable to type 'boolean'.");
}
