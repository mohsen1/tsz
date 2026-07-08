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

use tsz_checker::test_utils::check_source_strict as check_strict;
use tsz_common::diagnostics::Diagnostic;

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

// ── #15628: union-of-literals sources (tsc `getBaseTypeOfLiteralTypeUnion`) ──

/// An all-unit union property source generalizes member-wise: the union line
/// renders the reduced base (`"x" | "y"` -> `string`).
#[test]
fn union_of_string_literals_property_line_generalizes() {
    let source = r#"
declare const carton: { a: "x" | "y" };
const sinkC: { a: boolean } = carton;
"#;
    let diags = check_strict(source);
    let diag = one(&diags, 2322);
    assert_has_related(diag, "Type 'string' is not assignable to type 'boolean'.");
    assert_no_related(
        diag,
        "Type '\"x\" | \"y\"' is not assignable to type 'boolean'.",
    );
}

/// Mixed-base unit union maps each member through its base and re-unions
/// (`true | 1` -> `number | boolean`).
#[test]
fn mixed_literal_union_property_line_generalizes_to_base_union() {
    let source = r#"
declare const blend: { e: true | 1 };
const sinkE: { e: string } = blend;
"#;
    let diags = check_strict(source);
    let diag = one(&diags, 2322);
    assert_has_related(
        diag,
        "Type 'number | boolean' is not assignable to type 'string'.",
    );
}

/// Preservation gate: a literal-union target keeps the literal-union source.
#[test]
fn union_of_literals_preserved_against_literal_union_target() {
    let source = r#"
declare const duo2: { d: "x" | "y" };
const sinkD: { d: "w" | "z" } = duo2;
"#;
    let diags = check_strict(source);
    let diag = one(&diags, 2322);
    assert_has_related(
        diag,
        "Type '\"x\" | \"y\"' is not assignable to type '\"w\" | \"z\"'.",
    );
}

/// Top-level all-unit union source generalizes too (tsc runs the same
/// `reportRelationError` at every relation line).
#[test]
fn top_level_union_of_literals_generalizes() {
    let source = r#"
declare const pick2: "x" | "y";
const bool2: boolean = pick2;
"#;
    let diags = check_strict(source);
    let diag = one(&diags, 2322);
    assert!(
        diag.message_text == "Type 'string' is not assignable to type 'boolean'.",
        "top-level union line must generalize; got: {}",
        diag.message_text
    );
}

// ── #15628: top-level boolean-literal sources ──

/// A declared boolean-literal source widens to `boolean` against a
/// non-singleton target; string/number literal sources already widened.
#[test]
fn top_level_boolean_literal_source_generalizes() {
    let source = r#"
declare const flag1: true;
const s1: string = flag1;
declare const flag2: false;
const n2: number = flag2;
"#;
    let diags = check_strict(source);
    let texts: Vec<&str> = diags.iter().map(|d| d.message_text.as_str()).collect();
    assert!(
        texts.contains(&"Type 'boolean' is not assignable to type 'string'."),
        "true must widen to boolean; got: {texts:?}"
    );
    assert!(
        texts.contains(&"Type 'boolean' is not assignable to type 'number'."),
        "false must widen to boolean; got: {texts:?}"
    );
}

/// Preservation gate: a boolean-literal target keeps the boolean literal.
#[test]
fn top_level_boolean_literal_preserved_against_literal_target() {
    let source = r#"
declare const flag3: true;
const f3: false = flag3;
"#;
    let diags = check_strict(source);
    let diag = one(&diags, 2322);
    assert_eq!(
        diag.message_text,
        "Type 'true' is not assignable to type 'false'."
    );
}

// ── #15628: enum members at property leaves and the tsc-shaped top-level gate ──

/// Enum-member property leaves generalize to the parent enum against a
/// primitive target (previously leaked the bare member name).
#[test]
fn enum_member_property_leaf_widens_to_parent_enum() {
    let source = r#"
enum Bulk { One = 1, Two = 2 }
declare const crate1: { a: Bulk.One };
const sinkA: { a: string } = crate1;
"#;
    let diags = check_strict(source);
    let diag = one(&diags, 2322);
    assert_has_related(diag, "Type 'Bulk' is not assignable to type 'string'.");
    assert_no_related(diag, "Type 'One' is not assignable to type 'string'.");
}

/// Preservation gate: an enum-member target is singleton-capable, so the
/// source member keeps its qualified spelling at the leaf.
#[test]
fn enum_member_property_leaf_preserved_against_enum_member_target() {
    let source = r#"
enum Left { A = 1, B = 2 }
enum Right { C = 5, D = 6 }
declare const holder: { a: Left.A };
const sinkR: { a: Right.C } = holder;
"#;
    let diags = check_strict(source);
    let diag = one(&diags, 2322);
    assert_has_related(diag, "Type 'Left.A' is not assignable to type 'Right.C'.");
    assert_no_related(diag, "Type 'A' is not assignable to type 'C'.");
}

/// tsc-shaped top-level gate: a literal target preserves the enum member
/// (the older gate widened whenever the target was not enum/union/intersection).
#[test]
fn top_level_enum_member_preserved_against_literal_target() {
    let source = r#"
enum Gauge { Lo = 1, Hi = 2 }
declare const g1: Gauge.Lo;
const z1: "z" = g1;
declare const g2: Gauge.Lo;
const t1: `x${string}` = g2;
declare const g3: Gauge.Lo;
const b1: true = g3;
"#;
    let diags = check_strict(source);
    let texts: Vec<&str> = diags.iter().map(|d| d.message_text.as_str()).collect();
    assert!(
        texts.contains(&"Type 'Gauge.Lo' is not assignable to type '\"z\"'."),
        "literal target preserves the member; got: {texts:?}"
    );
    assert!(
        texts.contains(&"Type 'Gauge.Lo' is not assignable to type '`x${string}`'."),
        "template-literal target preserves the member; got: {texts:?}"
    );
    assert!(
        texts.contains(&"Type 'Gauge.Lo' is not assignable to type 'true'."),
        "boolean-literal target preserves the member; got: {texts:?}"
    );
}

/// TS2345 call-argument surface uses the same gate: an enum-member argument
/// widens to the parent enum against a primitive parameter and keeps its
/// spelling against a literal parameter.
#[test]
fn call_argument_enum_member_uses_singleton_gate() {
    let source = r#"
enum Cargo { A = 1, B = 2 }
declare const load: Cargo.A;
declare function sipX(x: boolean): void;
sipX(load);
declare const load2: Cargo.A;
declare function pinX(x: "z"): void;
pinX(load2);
"#;
    let diags = check_strict(source);
    let texts: Vec<&str> = diags.iter().map(|d| d.message_text.as_str()).collect();
    assert!(
        texts.contains(
            &"Argument of type 'Cargo' is not assignable to parameter of type 'boolean'."
        ),
        "primitive parameter widens the member; got: {texts:?}"
    );
    assert!(
        texts.contains(
            &"Argument of type 'Cargo.A' is not assignable to parameter of type '\"z\"'."
        ),
        "literal parameter preserves the member; got: {texts:?}"
    );
}

/// A single-member enum's member type IS the enum type in tsc, so it renders
/// as the bare enum name at every surface.
#[test]
fn single_member_enum_member_displays_as_enum_name() {
    let source = r#"
enum Solo { Only = 1 }
enum Pair { L = 1, R = 2 }
declare const s0: Solo.Only;
const sp: Pair.L = s0;
declare const s1: { a: Solo.Only };
const so: { a: string } = s1;
"#;
    let diags = check_strict(source);
    let texts: Vec<String> = diags
        .iter()
        .flat_map(|d| {
            std::iter::once(d.message_text.clone())
                .chain(d.related_information.iter().map(|r| r.message_text.clone()))
        })
        .collect();
    assert!(
        texts
            .iter()
            .any(|t| t == "Type 'Solo' is not assignable to type 'Pair.L'."),
        "single-member enum renders as the enum name; got: {texts:?}"
    );
    // The relation LEAF also renders the identity form. (The *object property
    // display* `{ a: Solo.Only; }` still shows the annotation spelling — that
    // provenance path is a separate display owner, tracked in #15628's
    // remaining notes.)
    assert!(
        texts
            .iter()
            .any(|t| t == "Type 'Solo' is not assignable to type 'string'."),
        "the lone member's leaf renders the enum name; got: {texts:?}"
    );
}

/// tsc gate detail: `boolean` inside a union target flattens to `true | false`
/// (both units), so the union preserves a literal source.
#[test]
fn boolean_in_union_target_preserves_literal_source() {
    let source = r#"
declare const five2: 5;
const mix1: string | boolean = five2;
declare const five3: 5;
const mix2: string | symbol = five3;
"#;
    let diags = check_strict(source);
    let texts: Vec<&str> = diags.iter().map(|d| d.message_text.as_str()).collect();
    assert!(
        texts.contains(&"Type '5' is not assignable to type 'string | boolean'."),
        "boolean member makes the union singleton-capable; got: {texts:?}"
    );
    assert!(
        texts.contains(&"Type 'number' is not assignable to type 'string | symbol'."),
        "no singleton capacity without the boolean member; got: {texts:?}"
    );
}

// ── #15628: deferred instantiable targets answer through their constraints ──

/// A deferred conditional-alias target whose default constraint contains
/// units preserves the literal source; an all-primitive constraint widens it.
#[test]
fn deferred_conditional_target_answers_through_constraint() {
    let source = r#"
type PickU<T> = T extends string ? "a" | "b" : number;
type PickP<T> = T extends string ? string : number;
function scope<T>(x: T) {
  const u: PickU<T> = "no" as const;
  const p: PickP<T> = "no" as const;
}
"#;
    let diags = check_strict(source);
    let texts: Vec<&str> = diags.iter().map(|d| d.message_text.as_str()).collect();
    assert!(
        texts.contains(&"Type '\"no\"' is not assignable to type 'PickU<T>'."),
        "unit-bearing constraint preserves the literal; got: {texts:?}"
    );
    assert!(
        texts.contains(&"Type 'string' is not assignable to type 'PickP<T>'."),
        "all-primitive constraint widens the literal; got: {texts:?}"
    );
}

/// An indexed-access target in a generic body answers through the evaluated
/// key-space union: unit-bearing property types preserve, primitives widen.
#[test]
fn deferred_indexed_access_target_answers_through_constraint() {
    let source = r#"
interface KnobsU { mode: "on" | "off"; size: number }
interface KnobsP { mode: string; size: number }
declare const feedU: { p: "no" };
function genU<K extends keyof KnobsU>(k: K) {
  const q: { p: KnobsU[K] } = feedU;
}
function genP<K extends keyof KnobsP>(k: K) {
  const q2: { p: KnobsP[K] } = feedU;
}
"#;
    let diags = check_strict(source);
    let leaves: Vec<&str> = diags
        .iter()
        .flat_map(|d| d.related_information.iter())
        .map(|r| r.message_text.as_str())
        .collect();
    assert!(
        leaves.contains(&"Type '\"no\"' is not assignable to type 'KnobsU[K]'."),
        "unit-bearing key space preserves the literal; got: {leaves:?}"
    );
    assert!(
        leaves.contains(&"Type 'string' is not assignable to type 'KnobsP[K]'."),
        "all-primitive key space widens the literal; got: {leaves:?}"
    );
}
