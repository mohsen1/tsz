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

/// The property-drill leaf of a *generic-base* indexed access (both operands
/// still carry a free type parameter) must keep the deferred `T[K]` identity
/// at its own pair, not the constraint-evaluated concrete type: tsc never
/// shows `Type 'number' is not assignable to type 'string'.` here — it keeps
/// `Type 'TBox[KKey]' is not assignable to type 'string | undefined'.` (see
/// #17718 witness 1; the deeper constraint-walk elaboration beneath this line
/// is a separate residual, not asserted here).
#[test]
fn deferred_generic_index_access_member_source_keeps_pair_identity() {
    let msg = message_with_chain(
        "function dig<TBox extends { a: number }, KKey extends keyof TBox>(x: { m: TBox[KKey] }) {\n  const y: { m: string | undefined } = x;\n}\n",
        2322,
    );
    assert!(
        msg.contains("Type 'TBox[KKey]' is not assignable to type 'string | undefined'."),
        "deferred generic-base indexed-access member source must keep its own identity, got: {msg}"
    );
    assert!(
        !msg.contains("'number'") && !msg.contains("'string'.\n"),
        "must not leak the constraint-evaluated concrete pair, got: {msg}"
    );
}

/// Same rule, renamed binders (anti-hardcoding: the behavior is structural,
/// not name-driven) and a TS2345 argument position instead of TS2322.
#[test]
fn deferred_generic_index_access_member_source_keeps_pair_identity_renamed_ts2345() {
    let msg = message_with_chain(
        "declare function gulp(v: { n: string | undefined }): void;\nfunction pipe<TRow extends { z: number }, KCol extends keyof TRow>(x: { n: TRow[KCol] }) {\n  gulp(x);\n}\n",
        2345,
    );
    assert!(
        msg.contains("Type 'TRow[KCol]' is not assignable to type 'string | undefined'."),
        "renamed binders / TS2345 must keep the deferred pair identity too, got: {msg}"
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
