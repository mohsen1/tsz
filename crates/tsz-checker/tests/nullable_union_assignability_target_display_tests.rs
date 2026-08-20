//! tsc-parity for the diagnostic display of a **nullable-union assignability
//! target** (`T | null`, `T | undefined`, `T | null | undefined`).
//!
//! `tsc` collapses a nullable-union target to its non-nullish part **only when
//! a single real member survives** the strip: `string | undefined` renders as
//! `string`, and a fresh literal source widens against it (`5` → `number`).
//! When two or more non-nullish members remain, `tsc` keeps the *full* union —
//! nullish members included — on the target line and preserves the literal
//! source. The rule is uniform across TS2322 (declaration, assignment, return,
//! property target) and TS2345 (argument), independent of whether the surviving
//! members are primitive or object-like.
//!
//! tsz previously over-reduced: `strip_nullish_for_assignability_display`
//! re-unioned the non-nullish members whenever *any* survived, dropping the
//! nullish member(s) even for the ≥2-member case. The fix restricts the
//! display collapse to a single survivor at the shared helper, so every
//! nullable-target display surface is corrected at once. This is display-only:
//! the assignability relation runs against the full declared union regardless.
//!
//! Binder names vary across cases (anti-hardcoding): the behavior is
//! structural, not name-driven.

use tsz_checker::test_utils::check_source_code_messages as get_diagnostics;
use tsz_checker::test_utils::check_source_diagnostics;

fn message(source: &str, code: u32) -> String {
    let diags = get_diagnostics(source);
    diags
        .iter()
        .find(|(c, _)| *c == code)
        .map(|(_, m)| m.clone())
        .unwrap_or_else(|| panic!("expected TS{code}; got: {diags:?}"))
}

/// The head message of the first `TS{code}` diagnostic plus every line of its
/// elaboration chain (`related_information`), joined with newlines, so a fence
/// can assert on the nested leaf a member mismatch drills to.
fn message_with_chain(source: &str, code: u32) -> String {
    let diags = check_source_diagnostics(source);
    let diag = diags
        .iter()
        .find(|d| d.code == code)
        .unwrap_or_else(|| panic!("expected TS{code}; got: {diags:?}"));
    let mut lines = vec![diag.message_text.clone()];
    lines.extend(
        diag.related_information
            .iter()
            .map(|related| related.message_text.clone()),
    );
    lines.join("\n")
}

// =====================================================================
// Must KEEP the full union (≥2 real members survive the nullish strip).
// =====================================================================

/// (a) `string | number | undefined` target keeps `undefined` on the target
/// line — two real members survive, so tsc does not drill to a single member.
#[test]
fn two_primitive_members_with_undefined_keeps_full_union() {
    let msg = message("const alpha: string | number | undefined = true;\n", 2322);
    assert!(
        msg.contains("string | number | undefined"),
        "target must keep the full nullable union, got: {msg}"
    );
}

/// (b) Assignment-expression position (`x = true`) keeps `... | null`.
#[test]
fn assignment_expression_target_keeps_null_member() {
    let msg = message("let beta: string | number | null;\nbeta = true;\n", 2322);
    assert!(
        msg.contains("string | number | null"),
        "assignment target must keep the `| null` member, got: {msg}"
    );
}

/// (c) Both `null` and `undefined` present with ≥2 real members: keep both.
#[test]
fn both_nullish_members_with_two_reals_kept() {
    let msg = message(
        "const gamma: string | number | undefined | null = true;\n",
        2322,
    );
    assert!(
        msg.contains("null") && msg.contains("undefined"),
        "both nullish members must remain on the target, got: {msg}"
    );
    assert!(
        msg.contains("string") && msg.contains("number"),
        "the real members must remain, got: {msg}"
    );
}

/// (d) Literal-union members (`"a" | "b" | undefined`) keep `undefined`; the
/// literal source is *not* widened (tsc keeps the full union in this branch).
#[test]
fn string_literal_union_with_undefined_keeps_full_union() {
    let msg = message("const delta: \"a\" | \"b\" | undefined = 5;\n", 2322);
    assert!(
        msg.contains("\"a\"") && msg.contains("\"b\"") && msg.contains("undefined"),
        "string-literal union target must keep every member incl. undefined, got: {msg}"
    );
}

/// (e) Numeric-literal union keeps `undefined` alongside both literals.
#[test]
fn numeric_literal_union_with_undefined_keeps_full_union() {
    let msg = message("const epsilon: 1 | 2 | undefined = 3;\n", 2322);
    assert!(
        msg.contains('1') && msg.contains('2') && msg.contains("undefined"),
        "numeric-literal union target must keep every member incl. undefined, got: {msg}"
    );
}

/// (f) Property target through an interface annotation keeps the full union.
#[test]
fn interface_property_target_keeps_full_union() {
    let msg = message(
        "interface Payload { field: string | number | undefined }\nconst zeta: Payload = { field: true };\n",
        2322,
    );
    assert!(
        msg.contains("string | number | undefined"),
        "interface property target must keep the full nullable union, got: {msg}"
    );
}

/// (g) Optional property (`field?: string | number`) synthesizes `| undefined`
/// on the target; the ≥2-member union is preserved.
#[test]
fn optional_interface_property_target_keeps_synthesized_undefined() {
    let msg = message(
        "interface Config { field?: string | number }\nconst eta: Config = { field: true };\n",
        2322,
    );
    assert!(
        msg.contains("string | number | undefined"),
        "optional property target must render the synthesized nullable union, got: {msg}"
    );
}

/// (h) Return-position target keeps the full union.
#[test]
fn arrow_return_target_keeps_full_union() {
    let msg = message(
        "const theta = (): string | number | undefined => true;\n",
        2322,
    );
    assert!(
        msg.contains("string | number | undefined"),
        "return-position target must keep the full nullable union, got: {msg}"
    );
}

/// (i) Object-like members (`{ a } | { b } | undefined`) keep `undefined` too.
#[test]
fn object_union_target_keeps_full_union() {
    let msg = message(
        "const iota: { a: number } | { b: string } | undefined = 5;\n",
        2322,
    );
    assert!(
        msg.contains("undefined"),
        "object-union target must keep the `undefined` member, got: {msg}"
    );
    assert!(
        msg.contains("{ a: number; }") && msg.contains("{ b: string; }"),
        "both object members must remain, got: {msg}"
    );
}

// =====================================================================
// Negative controls: tsz already correct, must stay unchanged.
// =====================================================================

/// Single real member: the collapse to `string` (and source widening `5` →
/// `number`) is preserved.
#[test]
fn single_member_with_undefined_still_collapses_to_string() {
    let msg = message("const solo: string | undefined = 5;\n", 2322);
    assert!(
        msg.contains("type 'string'"),
        "single-member nullable target must collapse to `string`, got: {msg}"
    );
    assert!(
        !msg.contains("undefined"),
        "single-member collapse must drop the nullish member, got: {msg}"
    );
}

/// Single object member collapses in the declaration position too — one real
/// member survives the strip, so the `undefined` is dropped from the target.
#[test]
fn single_object_member_declaration_collapses() {
    let msg = message("const soleObj: { a: number } | undefined = 5;\n", 2322);
    assert!(
        msg.contains("{ a: number; }"),
        "single-member object target must collapse to the bare object type, got: {msg}"
    );
    assert!(
        !msg.contains("undefined"),
        "single-member collapse must drop `undefined`, got: {msg}"
    );
}

/// No nullish member: a plain `string | number` union is never touched.
#[test]
fn union_without_nullish_unchanged() {
    let msg = message("const noNullish: string | number = true;\n", 2322);
    assert!(
        msg.contains("string | number"),
        "a non-nullable union must render unchanged, got: {msg}"
    );
}

/// Argument multi-member: parameter line keeps the full nullable union.
#[test]
fn argument_multi_member_keeps_full_union() {
    let msg = message(
        "declare function take(p: string | number | undefined): void;\ntake(true);\n",
        2345,
    );
    assert!(
        msg.contains("string | number | undefined"),
        "multi-member argument target must keep the full union, got: {msg}"
    );
}

// =====================================================================
// Generic (type-parameter) sources: tsc NEVER collapses the nullish
// members against a generic source, even when a single real member
// survives. A generic operand's relation to a union defers to its
// constraint instead of walking the union's constituents, so the message
// keeps the full declared union; a constrained parameter drills its
// constraint against the *stripped* member one level deeper instead.
// All expectations oracle-pinned against tsc 6.0.2 `--strict`.
// =====================================================================

/// Unconstrained type-parameter source at a member leaf keeps `| undefined`:
/// `Type 'TVal' is not assignable to type 'string | undefined'.`
#[test]
fn unconstrained_param_member_leaf_keeps_undefined_member() {
    let msg = message_with_chain(
        "function fold<TVal>(box: { m: TVal }) {\n  const sink: { m: string | undefined } = box;\n}\n",
        2322,
    );
    assert!(
        msg.contains("not assignable to type 'string | undefined'"),
        "type-param member leaf must keep the full nullable union, got: {msg}"
    );
}

/// Same rule for `| null`, renamed binders (anti-hardcoding).
#[test]
fn unconstrained_param_member_leaf_keeps_null_member() {
    let msg = message_with_chain(
        "function grab<TValue>(x: { prop: TValue }) {\n  const y: { prop: number | null } = x;\n}\n",
        2322,
    );
    assert!(
        msg.contains("not assignable to type 'number | null'"),
        "type-param member leaf must keep the `| null` member, got: {msg}"
    );
}

/// Top-level (non-member) type-parameter source keeps the full union on the
/// head line: `Type 'Item' is not assignable to type 'string | undefined'.`
#[test]
fn top_level_param_source_keeps_full_union() {
    let msg = message(
        "function pick<Item>(x: Item) {\n  const y: string | undefined = x;\n}\n",
        2322,
    );
    assert!(
        msg.contains("type 'string | undefined'"),
        "top-level type-param source must keep the full union, got: {msg}"
    );
}

/// A *constrained* (non-nullable) parameter still keeps the union at its own
/// leaf — the strip belongs to the constraint drill one level deeper.
#[test]
fn constrained_param_member_leaf_keeps_full_union() {
    let msg = message_with_chain(
        "function nab<Cnt extends number>(x: { m: Cnt }) {\n  const y: { m: string | undefined } = x;\n}\n",
        2322,
    );
    assert!(
        msg.contains("not assignable to type 'string | undefined'"),
        "constrained type-param leaf must keep the full nullable union, got: {msg}"
    );
}

/// Intersection of two type parameters keeps the full union.
#[test]
fn param_intersection_member_leaf_keeps_full_union() {
    let msg = message_with_chain(
        "function mix<A extends number, B extends number>(x: { m: A & B }) {\n  const y: { m: string | undefined } = x;\n}\n",
        2322,
    );
    assert!(
        msg.contains("not assignable to type 'string | undefined'"),
        "param-intersection leaf must keep the full nullable union, got: {msg}"
    );
}

/// Intersection of a type parameter with a concrete object keeps the union.
#[test]
fn param_and_concrete_intersection_keeps_full_union() {
    let msg = message_with_chain(
        "function meld<Obj extends object>(x: { m: Obj & { a: 1 } }) {\n  const y: { m: string | undefined } = x;\n}\n",
        2322,
    );
    assert!(
        msg.contains("not assignable to type 'string | undefined'"),
        "param-and-concrete intersection leaf must keep the full union, got: {msg}"
    );
}

/// TS2345 argument position follows the same rule as TS2322.
#[test]
fn argument_position_param_source_keeps_full_union() {
    let msg = message_with_chain(
        "declare function sinkFn(v: { m: string | undefined }): void;\nfunction pass<Elem>(x: { m: Elem }) {\n  sinkFn(x);\n}\n",
        2345,
    );
    assert!(
        msg.contains("not assignable to type 'string | undefined'"),
        "TS2345 type-param member leaf must keep the full union, got: {msg}"
    );
}

/// Two-level property path (`a.b`) keeps the union at the drilled leaf.
#[test]
fn nested_property_path_param_leaf_keeps_full_union() {
    let msg = message_with_chain(
        "function dig<Leaf>(x: { a: { b: Leaf } }) {\n  const y: { a: { b: string | undefined } } = x;\n}\n",
        2322,
    );
    assert!(
        msg.contains("not assignable to type 'string | undefined'"),
        "nested property leaf must keep the full nullable union, got: {msg}"
    );
}

/// Positive control: a CONCRETE member source still collapses the target
/// (`Type 'number' is not assignable to type 'string'.`).
#[test]
fn concrete_member_source_still_collapses() {
    let msg = message_with_chain(
        "function flat(x: { m: number }) {\n  const y: { m: string | undefined } = x;\n}\n",
        2322,
    );
    assert!(
        msg.contains("Type 'number' is not assignable to type 'string'."),
        "concrete member source must still collapse the nullable target, got: {msg}"
    );
}

/// Positive control: a literal member source collapses AND widens
/// (`Type 'string' is not assignable to type 'number'.`).
#[test]
fn literal_member_source_still_collapses_and_widens() {
    let msg = message_with_chain(
        "function lit(x: { m: \"abc\" }) {\n  const y: { m: number | undefined } = x;\n}\n",
        2322,
    );
    assert!(
        msg.contains("Type 'string' is not assignable to type 'number'."),
        "literal member source must still collapse and widen, got: {msg}"
    );
}

// =====================================================================
// Deferred constraint-relative sources beyond a bare type parameter.
//
// A source whose relation to a union defers to its base constraint — a
// type-parameter-mentioning indexed access (`T[K]`), a bare `keyof T`, or a
// distributive conditional — keeps the FULL nullable union on its pair's line,
// at the top-level head and at the property-mismatch drill leaf alike. tsc
// keeps the as-written operand there and walks the constraint one level deeper;
// the solver's evaluated nested reason instead carries a best-matching-member
// collapse (`... vs string`), which the drill-leaf fix overrides back to the
// raw pair. A fully concrete operand carries no type parameter, evaluates
// before display, and still collapses.
//
// All expectations oracle-pinned against the pinned typescript@7.0.2
// (`--strict`). Binder names vary across cases (anti-hardcoding).
// =====================================================================

/// Generic-base indexed access annotated at top level keeps the full union on
/// the head line: `Type 'TD[KD]' is not assignable to type 'string | undefined'.`
#[test]
fn top_level_indexed_access_source_keeps_full_union() {
    let msg = message(
        "function whee<TD extends { d: number }, KD extends keyof TD>(x: TD[KD]) {\n  const y: string | undefined = x;\n}\n",
        2322,
    );
    assert!(
        msg.contains("type 'string | undefined'"),
        "top-level indexed-access source must keep the full union, got: {msg}"
    );
}

/// Same rule for `| null` with a concrete base and generic index, renamed
/// binders (anti-hardcoding): the head keeps `number | null`.
#[test]
fn concrete_base_generic_index_keeps_null_member() {
    let msg = message(
        "interface Rows { first: string; second: string }\nfunction nulled<KR extends keyof Rows>(x: Rows[KR]) {\n  const y: number | null = x;\n}\n",
        2322,
    );
    assert!(
        msg.contains("type 'number | null'"),
        "generic-index indexed-access head must keep the `| null` member, got: {msg}"
    );
}

/// TS2345 head with a generic-base indexed-access argument keeps the union:
/// `Argument of type 'TE[KE]' is not assignable to parameter of type
/// 'string | undefined'.`
#[test]
fn ts2345_head_indexed_access_source_keeps_full_union() {
    let msg = message(
        "declare function gulp(v: string | undefined): void;\nfunction pipe<TE extends { e: number }, KE extends keyof TE>(x: TE[KE]) {\n  gulp(x);\n}\n",
        2345,
    );
    assert!(
        msg.contains("type 'string | undefined'"),
        "TS2345 indexed-access head must keep the full union, got: {msg}"
    );
}

/// Both nullish members survive against a deferred indexed-access source.
/// (Member ORDER within the rendered union is a separate live work item —
/// assert membership, not order.)
#[test]
fn indexed_access_source_keeps_both_nullish_members() {
    let msg = message_with_chain(
        "function both<TB extends { a: number }, KB extends keyof TB>(x: { m: TB[KB] }) {\n  const y: { m: string | undefined | null } = x;\n}\n",
        2322,
    );
    assert!(
        msg.contains("null") && msg.contains("undefined") && msg.contains("string"),
        "both nullish members must survive against an indexed-access source, got: {msg}"
    );
}

/// Concrete base with a *generic index* (`Obj[KP]`) is still deferred: the head
/// line keeps the union.
#[test]
fn concrete_base_generic_index_head_keeps_full_union() {
    let msg = message(
        "interface Obj { a: number; b: number }\nfunction idx<KP extends keyof Obj>(x: Obj[KP]) {\n  const y: string | undefined = x;\n}\n",
        2322,
    );
    assert!(
        msg.contains("type 'string | undefined'"),
        "generic-index indexed-access head must keep the full union, got: {msg}"
    );
}

/// A type-parameter-mentioning indexed-access member-leaf source keeps the
/// union at the drill leaf (was `Type 'TBox[KKey]' ... to type 'string'`).
#[test]
fn indexed_access_member_leaf_drill_keeps_full_union() {
    let msg = message_with_chain(
        "function dig<TBox, KKey extends keyof TBox>(x: { m: TBox[KKey] }) {\n  const y: { m: string | undefined } = x;\n}\n",
        2322,
    );
    assert!(
        msg.contains("Type 'TBox[KKey]' is not assignable to type 'string | undefined'."),
        "indexed-access member-leaf drill must keep the full nullable union, got: {msg}"
    );
}

/// Bare `keyof T` member-leaf source keeps `| undefined` on the drill leaf
/// (the solver's best-matching-member reason had collapsed it to `string`).
#[test]
fn keyof_member_leaf_source_keeps_full_union() {
    let msg = message_with_chain(
        "function fold<TObj>(box: { m: keyof TObj }) {\n  const sink: { m: string | undefined } = box;\n}\n",
        2322,
    );
    assert!(
        msg.contains("Type 'keyof TObj' is not assignable to type 'string | undefined'."),
        "keyof member-leaf drill must keep the full nullable union, got: {msg}"
    );
}

/// Same rule for `| null`, renamed binders.
#[test]
fn keyof_member_leaf_source_keeps_null_member() {
    let msg = message_with_chain(
        "function grab<TValue>(x: { prop: keyof TValue }) {\n  const y: { prop: number | null } = x;\n}\n",
        2322,
    );
    assert!(
        msg.contains("Type 'keyof TValue' is not assignable to type 'number | null'."),
        "keyof member-leaf drill must keep the `| null` member, got: {msg}"
    );
}

/// Top-level (non-member) `keyof T` source keeps the full union on the head.
#[test]
fn top_level_keyof_source_keeps_full_union() {
    let msg = message(
        "function pick<Item>(x: keyof Item) {\n  const y: string | undefined = x;\n}\n",
        2322,
    );
    assert!(
        msg.contains("type 'string | undefined'"),
        "top-level keyof source must keep the full union, got: {msg}"
    );
}

/// Top-level `keyof T`, `| null`, renamed binder.
#[test]
fn top_level_keyof_source_keeps_null_member() {
    let msg = message(
        "function nab<Row>(x: keyof Row) {\n  const y: string | null = x;\n}\n",
        2322,
    );
    assert!(
        msg.contains("type 'string | null'"),
        "top-level keyof source must keep the `| null` member, got: {msg}"
    );
}

/// Negative control: a fully CONCRETE indexed access (`Conc["a"]` = `number`)
/// carries no type parameter, evaluates before display, and still collapses.
#[test]
fn concrete_indexed_access_member_source_still_collapses() {
    let msg = message_with_chain(
        "interface Conc { a: number }\nfunction flat(x: { m: Conc[\"a\"] }) {\n  const y: { m: string | undefined } = x;\n}\n",
        2322,
    );
    assert!(
        msg.contains("Type 'number' is not assignable to type 'string'."),
        "concrete indexed-access member source must still collapse, got: {msg}"
    );
}

/// The property-drill leaf of a *generic-base* indexed access (both operands
/// still carry a free type parameter) keeps the deferred `T[K]` identity at its
/// own pair, then walks the constraint one step per line — the base constraint
/// `{ a: number }` on `TBox` does not change the walk (`keyof TBox` still
/// expands to its `string | number | symbol` key space, not `"a"`), so this is
/// byte-identical to the unconstrained witness. tsc never collapses to a
/// concrete `number` here; `TBox[string]` stays deferred and keeps the full
/// nullable union to the leaf.
#[test]
fn deferred_generic_index_access_member_source_keeps_pair_identity() {
    let msg = message_with_chain(
        "function dig<TBox extends { a: number }, KKey extends keyof TBox>(x: { m: TBox[KKey] }) {\n  const y: { m: string | undefined } = x;\n}\n",
        2322,
    );
    assert_eq!(
        msg,
        "Type '{ m: TBox[KKey]; }' is not assignable to type '{ m: string | undefined; }'.\n\
         Types of property 'm' are incompatible.\n\
         Type 'TBox[KKey]' is not assignable to type 'string | undefined'.\n\
         Type 'TBox[keyof TBox]' is not assignable to type 'string | undefined'.\n\
         Type 'TBox[string] | TBox[number] | TBox[symbol]' is not assignable to type 'string | undefined'.\n\
         Type 'TBox[string]' is not assignable to type 'string | undefined'.",
    );
}

/// Expression-typed indexed access on a *concrete* receiver (`bag[k]`, not a
/// declared `Bag[KSel]` annotation) keeps the deferred `Bag[KSel]` identity —
/// mirrors `concrete_base_generic_index_head_keeps_full_union` above but for
/// the EXPRESSION form (#17718 witness 2). Before this fix the element-access
/// expression's own type eagerly resolved to the union of member value types,
/// so the pair collapsed to `Type 'number' is not assignable to type 'string
/// | undefined'.`. Oracle-verified (typescript@7.0.2) head: `Type 'Bag[KSel]'
/// is not assignable to type 'string | undefined'.`; the oracle's deeper
/// constraint-walk elaboration line (`Type 'number' is not assignable to type
/// 'string'.`) is the same documented residual as witness 1's drill leaf
/// (#17718) and not asserted here.
#[test]
fn concrete_receiver_expression_indexed_access_keeps_full_union() {
    let msg = message(
        "interface Bag { one: number; two: number }\nfunction pick<KSel extends keyof Bag>(x: Bag, k: KSel) {\n  const y: string | undefined = x[k];\n}\n",
        2322,
    );
    assert_eq!(
        msg, "Type 'Bag[KSel]' is not assignable to type 'string | undefined'.",
        "concrete-receiver expression-typed indexed access must keep the deferred pair, got: {msg}"
    );
}

/// Same rule, TS2345 argument position.
#[test]
fn concrete_receiver_expression_indexed_access_argument_keeps_full_union() {
    let msg = message(
        "interface Bag { one: number; two: number }\ndeclare function eat(v: string | undefined): void;\nfunction pick<KSel extends keyof Bag>(x: Bag, k: KSel) {\n  eat(x[k]);\n}\n",
        2345,
    );
    assert_eq!(
        msg,
        "Argument of type 'Bag[KSel]' is not assignable to parameter of type 'string | undefined'.",
        "concrete-receiver expression-typed indexed access argument must keep the deferred pair, got: {msg}"
    );
}

/// Renamed binders (anti-hardcoding: the behavior is structural, not
/// name-driven) and a `| null` target instead of `| undefined`.
#[test]
fn concrete_receiver_expression_indexed_access_renamed_binders_null_variant() {
    let msg = message(
        "interface Wares { p: number; q: number }\nfunction grab<KW extends keyof Wares>(goods: Wares, key: KW) {\n  const y: string | null = goods[key];\n}\n",
        2322,
    );
    assert_eq!(
        msg, "Type 'Wares[KW]' is not assignable to type 'string | null'.",
        "renamed-binder concrete-receiver expression indexed access must keep the deferred pair, got: {msg}"
    );
}

/// Negative control: a genuinely generic receiver (`x: T`) still goes through
/// the type-parameter deferral path unaffected — the concrete-receiver branch
/// must not fire when the receiver is itself a type parameter. This is
/// #17718 witness 2's own repro; tsc's oracle output is the head plus the
/// 3-line constraint walk (`indexed_access_constraint_display_walk`, wired to
/// this top-level expression-source call site — see #17718's 2026-08-19
/// 23:47Z comment), oracle-verified via `scripts/conformance/oracle.sh`.
#[test]
fn generic_receiver_expression_indexed_access_still_defers_via_type_param_path() {
    let msg = message_with_chain(
        "function pick<T, K extends keyof T>(x: T, k: K) {\n  const y: string | undefined = x[k];\n}\n",
        2322,
    );
    assert_eq!(
        msg,
        "Type 'T[K]' is not assignable to type 'string | undefined'.\n\
         Type 'T[keyof T]' is not assignable to type 'string | undefined'.\n\
         Type 'T[string] | T[number] | T[symbol]' is not assignable to type 'string | undefined'.\n\
         Type 'T[string]' is not assignable to type 'string | undefined'.",
        "generic-receiver expression indexed access must keep its own (unrelated) deferral path, got: {msg}"
    );
}

/// Negative control: a literal-key expression access on a concrete receiver
/// (`bag["one"]`, not a generic index) must still resolve eagerly — the new
/// branch is gated on the index being generic, not merely absent from the
/// existing `is_index_access_type(raw_object_type)` case.
#[test]
fn concrete_receiver_literal_key_expression_access_still_resolves_eagerly() {
    let msg = message(
        "interface Bag { one: number; two: number }\nfunction pick(x: Bag) {\n  const y: string | undefined = x[\"one\"];\n}\n",
        2322,
    );
    assert_eq!(
        msg, "Type 'number' is not assignable to type 'string'.",
        "literal-key element access on a concrete receiver must still resolve eagerly, got: {msg}"
    );
}

/// Same rule, renamed binders (anti-hardcoding: the behavior is structural,
/// not name-driven), TS2345 argument position. The argument-drill renderer is a
/// separate "no source elaboration" path that used to keep only the as-written
/// pair; it now routes a deferred, constraint-relative property source through
/// the same shared property-drill leaf the declaration-position (TS2322) walk
/// hangs on, so the full `TRow[keyof TRow]` -> distribute -> `TRow[string]`
/// chain renders here too (byte-identical to tsc, modulo the leading
/// indentation the helper strips). A `{ z: number }` base constraint does not
/// change the walk (`keyof TRow` still expands to its key space, not `"z"`).
#[test]
fn deferred_generic_index_access_member_source_keeps_pair_identity_renamed_ts2345() {
    let msg = message_with_chain(
        "declare function gulp(v: { n: string | undefined }): void;\nfunction pipe<TRow extends { z: number }, KCol extends keyof TRow>(x: { n: TRow[KCol] }) {\n  gulp(x);\n}\n",
        2345,
    );
    assert_eq!(
        msg,
        "Argument of type '{ n: TRow[KCol]; }' is not assignable to parameter of type '{ n: string | undefined; }'.\n\
         Types of property 'n' are incompatible.\n\
         Type 'TRow[KCol]' is not assignable to type 'string | undefined'.\n\
         Type 'TRow[keyof TRow]' is not assignable to type 'string | undefined'.\n\
         Type 'TRow[string] | TRow[number] | TRow[symbol]' is not assignable to type 'string | undefined'.\n\
         Type 'TRow[string]' is not assignable to type 'string | undefined'.",
    );
}

/// TS2345 argument, concrete-base generic index (`Obj[KP]`): the walk
/// concretizes the object in a single step to the resolved value type `number`,
/// and the target collapses to its single real member `string`. Companion to the
/// generic-base argument walk above, exercising the concrete short-circuit on
/// the argument surface.
#[test]
fn concrete_base_argument_member_drill_walks_to_resolved_value_type_ts2345() {
    let msg = message_with_chain(
        "interface Obj { a: number; b: number }\ndeclare function sink(v: { m: string | undefined }): void;\nfunction idx<KP extends keyof Obj>(x: { m: Obj[KP] }) {\n  sink(x);\n}\n",
        2345,
    );
    assert_eq!(
        msg,
        "Argument of type '{ m: Obj[KP]; }' is not assignable to parameter of type '{ m: string | undefined; }'.\n\
         Types of property 'm' are incompatible.\n\
         Type 'Obj[KP]' is not assignable to type 'string | undefined'.\n\
         Type 'number' is not assignable to type 'string'.",
    );
}

/// Nested dotted-path drill (`{ outer: { m: T[K] } }`): tsc collapses the
/// two-link property run into a single `The types of 'outer.m' are incompatible
/// between these types.` header and then walks the deferred constraint one step
/// per line beneath it — the same walk the single-property leaf synthesizes. The
/// dotted-path collapse funnels its folded leaf through the shared
/// `push_property_chain_leaf`, so the walk hangs there too.
#[test]
fn nested_dotted_path_member_drill_emits_full_constraint_walk() {
    let msg = message_with_chain(
        "function dig<TBox, KKey extends keyof TBox>(x: { outer: { m: TBox[KKey] } }) {\n  const y: { outer: { m: string | undefined } } = x;\n}\n",
        2322,
    );
    assert_eq!(
        msg,
        "Type '{ outer: { m: TBox[KKey]; }; }' is not assignable to type '{ outer: { m: string | undefined; }; }'.\n\
         The types of 'outer.m' are incompatible between these types.\n\
         Type 'TBox[KKey]' is not assignable to type 'string | undefined'.\n\
         Type 'TBox[keyof TBox]' is not assignable to type 'string | undefined'.\n\
         Type 'TBox[string] | TBox[number] | TBox[symbol]' is not assignable to type 'string | undefined'.\n\
         Type 'TBox[string]' is not assignable to type 'string | undefined'.",
    );
}

/// Nested dotted-path drill, concrete base (`{ outer: { m: Obj[KP] } }`): the
/// folded-leaf walk concretizes `Obj` in a single step to `number` and collapses
/// the target to `string`, mirroring the concrete short-circuit on the
/// single-property and argument surfaces.
#[test]
fn nested_dotted_path_concrete_base_member_drill_walks_to_resolved_value_type() {
    let msg = message_with_chain(
        "interface Obj { a: number; b: number }\nfunction idx<KP extends keyof Obj>(x: { outer: { m: Obj[KP] } }) {\n  const y: { outer: { m: string | undefined } } = x;\n}\n",
        2322,
    );
    assert_eq!(
        msg,
        "Type '{ outer: { m: Obj[KP]; }; }' is not assignable to type '{ outer: { m: string | undefined; }; }'.\n\
         The types of 'outer.m' are incompatible between these types.\n\
         Type 'Obj[KP]' is not assignable to type 'string | undefined'.\n\
         Type 'number' is not assignable to type 'string'.",
    );
}

/// A bare `keyof T` source keeps ONLY its leaf line on the argument and
/// dotted-path surfaces too (the walk is intentionally empty for it — the
/// `string | number | symbol` key space renders through the `PropertyKey`
/// display alias, a separate printer residual). This pins that the newly-wired
/// surfaces do not diverge from the single-property leaf's bare-`keyof`
/// behavior.
#[test]
fn keyof_argument_and_dotted_path_sources_keep_leaf_without_walk() {
    let arg = message_with_chain(
        "declare function sink(v: { m: string | undefined }): void;\nfunction fold<TObj>(box: { m: keyof TObj }) {\n  sink(box);\n}\n",
        2345,
    );
    assert_eq!(
        arg,
        "Argument of type '{ m: keyof TObj; }' is not assignable to parameter of type '{ m: string | undefined; }'.\n\
         Types of property 'm' are incompatible.\n\
         Type 'keyof TObj' is not assignable to type 'string | undefined'.",
    );
    let dotted = message_with_chain(
        "function fold<TObj>(box: { outer: { m: keyof TObj } }) {\n  const sink: { outer: { m: string | undefined } } = box;\n}\n",
        2322,
    );
    assert_eq!(
        dotted,
        "Type '{ outer: { m: keyof TObj; }; }' is not assignable to type '{ outer: { m: string | undefined; }; }'.\n\
         The types of 'outer.m' are incompatible between these types.\n\
         Type 'keyof TObj' is not assignable to type 'string | undefined'.",
    );
}

/// Negative control: a *concrete-base, generic-index* member source
/// (`Obj[KP]`) is also deferred — the constraint drill only concretizes
/// `Obj`, not the still-generic key — and keeps the full pair at the
/// property-drill leaf too, matching the existing top-level behavior
/// (`concrete_base_generic_index_head_keeps_full_union`).
#[test]
fn concrete_base_generic_index_member_source_keeps_pair_identity() {
    let msg = message_with_chain(
        "interface Obj { a: number; b: number }\nfunction idx<KP extends keyof Obj>(x: { m: Obj[KP] }) {\n  const y: { m: string | undefined } = x;\n}\n",
        2322,
    );
    assert!(
        msg.contains("Type 'Obj[KP]' is not assignable to type 'string | undefined'."),
        "concrete-base generic-index member source must keep its own identity, got: {msg}"
    );
}

// =====================================================================
// Generic-alias application over a deferred indexed access (#17718 witness 3).
// =====================================================================

/// A generic alias whose body is a bare indexed access over its own type
/// parameters (`type Pluck<TSrc, KSel extends keyof TSrc> = TSrc[KSel]`)
/// carries the same deferred-relation behavior as the bare body when the
/// application's arguments are themselves still generic: tsc keeps the full
/// declared target union on the head line, drilling one level deeper before
/// it collapses. tsz previously ignored the alias wrapper entirely and
/// collapsed at the head.
#[test]
fn generic_alias_application_of_deferred_indexed_access_keeps_full_union() {
    let msg = message(
        r#"
type Pluck<TSrc, KSel extends keyof TSrc> = TSrc[KSel];
function alias2<TSrc, KSel extends keyof TSrc>(x: Pluck<TSrc, KSel>) {
  const y: string | undefined = x;
}
"#,
        2322,
    );
    assert!(
        msg.contains("'string | undefined'"),
        "generic alias application of a deferred indexed access must keep the full union, got: {msg}"
    );
}

/// Negative control: once every type argument the alias's body indexes with
/// is concrete, the application evaluates like a bare concrete indexed
/// access and still collapses.
#[test]
fn concrete_alias_instantiation_of_indexed_access_still_collapses() {
    let msg = message(
        r#"
type Pluck<TSrc, KSel extends keyof TSrc> = TSrc[KSel];
interface Bag { one: number; two: number }
function alias2(x: Pluck<Bag, "one">) {
  const y: string | undefined = x;
}
"#,
        2322,
    );
    assert!(
        !msg.contains("| undefined"),
        "fully concrete alias instantiation must collapse like a bare indexed access, got: {msg}"
    );
}

/// Anti-hardcoding: a differently-named alias and differently-named enclosing
/// type parameters must behave identically — the match is positional against
/// the alias's own declared parameters, not by name.
#[test]
fn renamed_binder_generic_alias_application_keeps_full_union() {
    let msg = message(
        r#"
type Grab<A, B extends keyof A> = A[B];
function alias3<Src, Sel extends keyof Src>(x: Grab<Src, Sel>) {
  const y: string | undefined = x;
}
"#,
        2322,
    );
    assert!(
        msg.contains("'string | undefined'"),
        "renamed-binder generic alias application must keep the full union, got: {msg}"
    );
}

/// Only one of the two application arguments is still generic (the object
/// side resolved to a concrete interface, the index side stays an unresolved
/// type parameter): the result is still a deferred indexed access, matching
/// the pre-existing bare-`Obj[K]`-with-generic-index rule.
#[test]
fn partially_generic_alias_application_keeps_full_union() {
    let msg = message(
        r#"
type Pluck<TSrc, KSel extends keyof TSrc> = TSrc[KSel];
interface Bag { one: number; two: number }
function alias4<KSel extends keyof Bag>(x: Pluck<Bag, KSel>) {
  const y: string | undefined = x;
}
"#,
        2322,
    );
    assert!(
        msg.contains("'string | undefined'"),
        "partially generic alias application must keep the full union, got: {msg}"
    );
}

// =====================================================================
// Constraint-walk elaboration (#17718 residual): tsc walks a deferred
// constraint-relative source one step per line beneath the as-written
// operand. Oracle-pinned against typescript@7.0.2 --strict (byte-for-byte,
// modulo the leading indentation `message_with_chain` strips).
// =====================================================================

/// Witness 1 (generic-base member drill): `TBox[KKey]` walks
/// `TBox[KKey]` -> `TBox[keyof TBox]` -> distributed union -> first member.
#[test]
fn generic_base_member_drill_emits_full_constraint_walk() {
    let msg = message_with_chain(
        "function dig<TBox, KKey extends keyof TBox>(x: { m: TBox[KKey] }) {\n  const y: { m: string | undefined } = x;\n}\n",
        2322,
    );
    assert_eq!(
        msg,
        "Type '{ m: TBox[KKey]; }' is not assignable to type '{ m: string | undefined; }'.\n\
         Types of property 'm' are incompatible.\n\
         Type 'TBox[KKey]' is not assignable to type 'string | undefined'.\n\
         Type 'TBox[keyof TBox]' is not assignable to type 'string | undefined'.\n\
         Type 'TBox[string] | TBox[number] | TBox[symbol]' is not assignable to type 'string | undefined'.\n\
         Type 'TBox[string]' is not assignable to type 'string | undefined'.",
    );
}

/// Witness 1, `| null` target, renamed binder: the deferred leaf keeps the
/// full `number | null` union (source stays generic through the whole walk).
#[test]
fn generic_base_member_drill_null_target_keeps_full_union_through_walk() {
    let msg = message_with_chain(
        "function dug<TBox, KKey extends keyof TBox>(x: { m: TBox[KKey] }) {\n  const y: { m: number | null } = x;\n}\n",
        2322,
    );
    assert_eq!(
        msg,
        "Type '{ m: TBox[KKey]; }' is not assignable to type '{ m: number | null; }'.\n\
         Types of property 'm' are incompatible.\n\
         Type 'TBox[KKey]' is not assignable to type 'number | null'.\n\
         Type 'TBox[keyof TBox]' is not assignable to type 'number | null'.\n\
         Type 'TBox[string] | TBox[number] | TBox[symbol]' is not assignable to type 'number | null'.\n\
         Type 'TBox[string]' is not assignable to type 'number | null'.",
    );
}

/// A bare `keyof T` member-drill source keeps ONLY its as-written leaf line —
/// the constraint walk is intentionally not synthesized for it. tsc walks
/// `keyof TObj` -> `string | number | symbol` -> `number`, but tsz renders that
/// key space through the `PropertyKey` display alias, so the intermediate would
/// diverge; expanding the alias in that position is a separate printer fix. The
/// indexed-access sources below never hit the alias (they distribute per key).
#[test]
fn keyof_member_drill_source_keeps_leaf_without_walk() {
    let msg = message_with_chain(
        "function fold<TObj>(box: { m: keyof TObj }) {\n  const sink: { m: string | undefined } = box;\n}\n",
        2322,
    );
    assert_eq!(
        msg,
        "Type '{ m: keyof TObj; }' is not assignable to type '{ m: string | undefined; }'.\n\
         Types of property 'm' are incompatible.\n\
         Type 'keyof TObj' is not assignable to type 'string | undefined'.",
    );
}

/// Concrete-base generic-index member drill: `Obj[KP]` concretizes the object
/// in a single step to the resolved value type `number`, target collapses to
/// `string`. (Companion to `concrete_base_generic_index_member_source_keeps_pair_identity`,
/// which fences the as-written first line.)
#[test]
fn concrete_base_member_drill_walks_to_resolved_value_type() {
    let msg = message_with_chain(
        "interface Obj { a: number; b: number }\nfunction idx<KP extends keyof Obj>(x: { m: Obj[KP] }) {\n  const y: { m: string | undefined } = x;\n}\n",
        2322,
    );
    assert_eq!(
        msg,
        "Type '{ m: Obj[KP]; }' is not assignable to type '{ m: string | undefined; }'.\n\
         Types of property 'm' are incompatible.\n\
         Type 'Obj[KP]' is not assignable to type 'string | undefined'.\n\
         Type 'number' is not assignable to type 'string'.",
    );
}

/// #17718 witness 2: a plain expression-level `x[k]` access on a still-generic
/// receiver (`T extends Wares`, `K extends keyof T`) keeps the deferred
/// `T[K]` pair on the TS2322 head, matching tsc's oracle output for this
/// witness. tsz previously eagerly resolved through `K`'s (already-reduced)
/// `keyof T` constraint to `Wares`'s concrete property union, rendering
/// `number` instead of `T[K]`. tsc also walks the operand's constraint one
/// step per elaboration line beneath the head
/// (`indexed_access_constraint_display_walk`, wired to this top-level
/// expression-source case alongside the property-drill leaf — see #17718's
/// 2026-08-19 23:47Z comment); oracle-verified via `scripts/conformance/oracle.sh`.
#[test]
fn expression_indexed_access_generic_receiver_keeps_deferred_pair() {
    let msg = message_with_chain(
        "interface Wares { p: number; q: number }\nfunction pick<T extends Wares, K extends keyof T>(x: T, k: K) {\n  const y: string | undefined = x[k];\n}\n",
        2322,
    );
    assert_eq!(
        msg,
        "Type 'T[K]' is not assignable to type 'string | undefined'.\n\
         Type 'T[keyof T]' is not assignable to type 'string | undefined'.\n\
         Type 'T[string] | T[number] | T[symbol]' is not assignable to type 'string | undefined'.\n\
         Type 'T[string]' is not assignable to type 'string | undefined'."
    );
}

/// Same structural shape with renamed binders (anti-hardcoding): the
/// behavior is structural, not tied to the `T`/`K`/`Wares` spelling.
#[test]
fn expression_indexed_access_generic_receiver_keeps_deferred_pair_renamed_binders() {
    let msg = message_with_chain(
        "interface Bag { a: number; b: number }\nfunction grab<TSrc extends Bag, KSel extends keyof TSrc>(obj: TSrc, sel: KSel) {\n  const out: string | undefined = obj[sel];\n}\n",
        2322,
    );
    assert_eq!(
        msg,
        "Type 'TSrc[KSel]' is not assignable to type 'string | undefined'.\n\
         Type 'TSrc[keyof TSrc]' is not assignable to type 'string | undefined'.\n\
         Type 'TSrc[string] | TSrc[number] | TSrc[symbol]' is not assignable to type 'string | undefined'.\n\
         Type 'TSrc[string]' is not assignable to type 'string | undefined'."
    );
}

/// A CONCRETE receiver (not a type parameter) indexed by a generic key also
/// keeps the deferred `Wares3[K]` identity on the head, matching tsc's
/// oracle output — the concrete-receiver sibling of the generic-receiver
/// case above (#17718 witness 2's own target; see
/// `concrete_receiver_expression_indexed_access_keeps_full_union` and its
/// siblings for the fuller adjacent matrix). Previously pinned as a negative
/// control asserting the pre-fix eager-resolve behavior (`Type 'number' is
/// not assignable to type 'string'.`); oracle-reverified against pinned
/// typescript@7.0.2 and flipped to the correct expectation.
///
/// tsc also emits a second, indented elaboration line (`Type 'number' is not
/// assignable to type 'string'.`) beneath the head, produced by walking the
/// deferred operand's constraint one step to its concrete value type
/// (`indexed_access_constraint_display_walk`, already used by the
/// property-drill leaf since #17750; wired to this top-level expression-
/// source head too — see #17718's 2026-08-19 23:47Z comment).
#[test]
fn expression_indexed_access_concrete_receiver_also_keeps_deferred_pair() {
    let msg = message_with_chain(
        "interface Wares3 { p: number; q: number }\nfunction pick3<K extends keyof Wares3>(x: Wares3, k: K) {\n  const y: string = x[k];\n}\n",
        2322,
    );
    assert_eq!(
        msg,
        "Type 'Wares3[K]' is not assignable to type 'string'.\nType 'number' is not assignable to type 'string'."
    );
}

// =====================================================================
// #17718: the constraint-walk elaboration beneath a **top-level** (non-
// property) deferred-indexed-access head must nest at the header's own
// child depth, not one level deeper. `message_with_chain` above strips the
// leading indentation, so the depth is asserted directly here against
// `related_information`. A top-level TS2322 header is the diagnostic's main
// message; per the renderer's `2 * (depth + 1)`-space rule its first child
// sits at depth 0 (2 spaces). `push_deferred_constraint_walk_steps` had used
// the property-drill child depth (`base_depth + 1`), over-indenting the whole
// walk by one level for these plain top-level heads — tsc renders the first
// step at 2 spaces, tsz rendered it at 4. Oracle-verified against pinned
// typescript@7.0.2 via `scripts/conformance/oracle.sh`.
// =====================================================================

/// The `related_information` chain of the first `TS{code}` diagnostic as
/// `(depth, message)` pairs, so a fence can assert the elaboration nesting
/// (which `message_with_chain` flattens away).
fn chain_depths(source: &str, code: u32) -> Vec<(u8, String)> {
    let diags = check_source_diagnostics(source);
    let diag = diags
        .iter()
        .find(|d| d.code == code)
        .unwrap_or_else(|| panic!("expected TS{code}; got: {diags:?}"));
    diag.related_information
        .iter()
        .map(|related| (related.depth, related.message_text.clone()))
        .collect()
}

#[test]
fn top_level_generic_indexed_access_walk_nests_at_header_child_depth() {
    // Generic receiver `x: T`, key `k: K`: the constraint walk's three steps
    // hang directly beneath the top-level head, starting at depth 0.
    let chain = chain_depths(
        "function pick<T, K extends keyof T>(x: T, k: K) {\n  const y: string | undefined = x[k];\n}\n",
        2322,
    );
    assert_eq!(
        chain,
        vec![
            (
                0,
                "Type 'T[keyof T]' is not assignable to type 'string | undefined'.".to_string()
            ),
            (
                1,
                "Type 'T[string] | T[number] | T[symbol]' is not assignable to type 'string | undefined'.".to_string()
            ),
            (
                2,
                "Type 'T[string]' is not assignable to type 'string | undefined'.".to_string()
            ),
        ],
        "top-level generic indexed-access walk must nest at the header's child depth (0, 1, 2)"
    );
}

#[test]
fn top_level_concrete_receiver_indexed_access_walk_nests_at_header_child_depth() {
    // Concrete receiver `x: Wares3` (the IntrinsicTypeMismatch catch-all path):
    // the single concrete walk step sits at depth 0 beneath the head.
    let chain = chain_depths(
        "interface Wares3 { p: number; q: number }\nfunction pick3<K extends keyof Wares3>(x: Wares3, k: K) {\n  const y: string = x[k];\n}\n",
        2322,
    );
    assert_eq!(
        chain,
        vec![(
            0,
            "Type 'number' is not assignable to type 'string'.".to_string()
        )],
        "concrete-receiver indexed-access walk step must sit at the header's child depth (0)"
    );
}

#[test]
fn top_level_indexed_access_walk_depth_is_binder_name_independent() {
    // Renamed binders (anti-hardcoding): the nesting depth is structural, not
    // keyed on the type-parameter spelling.
    let chain = chain_depths(
        "function grab<Src, Sel extends keyof Src>(bag: Src, sel: Sel) {\n  const out: string | undefined = bag[sel];\n}\n",
        2322,
    );
    let depths: Vec<u8> = chain.iter().map(|(depth, _)| *depth).collect();
    assert_eq!(
        depths,
        vec![0, 1, 2],
        "walk nesting depth must be independent of binder names, got chain: {chain:?}"
    );
}
