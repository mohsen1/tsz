//! Union-source *structural* elaboration (TS2322 + member header + drill).
//!
//! Structural rule: when a *union* value is assigned to a tuple or object
//! target and the first failing member is rejected for a *structural* reason —
//! a tuple element type mismatch or an object property-type mismatch — tsc
//! keeps the top-level `Type 'A | B' is not assignable to type 'T'` (TS2322)
//! and elaborates *which* member fails, leading with the member-type header and
//! then drilling into the structural detail:
//!
//! ```text
//! Type 'B' is not assignable to type 'A'.
//!   Type '[id: number, count: number]' is not assignable to type '[id: string, count: number]'.
//!     Type at position 0 in source is not compatible with type at position 0 in target.
//!       Type 'number' is not assignable to type 'string'.
//! ```
//!
//! ```text
//! Type 'B' is not assignable to type 'A'.
//!   Type '{ id: number; count: number; }' is not assignable to type 'A'.
//!     Types of property 'id' are incompatible.
//!       Type 'number' is not assignable to type 'string'.
//! ```
//!
//! tsz previously surfaced union-source elaboration only for *self-heading*
//! members (leaf relations and the `MissingProperty`/`MissingProperties`
//! summaries); a member that failed structurally fell through to the bare
//! top-level `Type 'A | B' is not assignable to type 'T'` line, hiding which
//! member and which position/property was responsible. Routing tuple-element
//! and property-type mismatches through `UnionSourceMismatch` with an explicit
//! member header reproduces tsc's chain.
//!
//! These tests vary the binder names (alias/property spellings) so a fix keyed
//! to a particular spelling would not satisfy them, and they assert
//! structurally (the chain names the failing member, the position/property, and
//! the leaf relation) rather than depending on exact type-printer rendering.

use tsz_checker::test_utils::check_source_strict;
use tsz_common::diagnostics::Diagnostic;

fn diagnostics(source: &str) -> Vec<Diagnostic> {
    check_source_strict(source)
}

/// Collect the nested elaboration lines (in chain order) of the first TS2322.
fn ts2322_chain(diags: &[Diagnostic]) -> Vec<(u8, u32, String)> {
    diags
        .iter()
        .find(|d| d.code == 2322)
        .map(|d| {
            d.related_information
                .iter()
                .map(|r| (r.depth, r.code, r.message_text.clone()))
                .collect()
        })
        .unwrap_or_default()
}

fn chain_has<P: Fn(&str) -> bool>(chain: &[(u8, u32, String)], depth: u8, predicate: P) -> bool {
    chain
        .iter()
        .any(|(d, _, msg)| *d == depth && predicate(msg))
}

/// Tuple union member failing on an element type mismatch elaborates the member
/// header, the TS2626 positional line, and the leaf relation, each one indent
/// deeper than the last.
#[test]
fn union_member_tuple_element_mismatch_emits_header_and_drill() {
    let diags = diagnostics(
        r#"
type Target = [id: string, count: number];
type Source = [id: number, count: number] | [id: string, count: string];
declare const s: Source;
const t: Target = s;
"#,
    );
    let chain = ts2322_chain(&diags);
    assert!(
        chain_has(&chain, 0, |m| m.contains("is not assignable to type")
            && m.contains("number")),
        "expected a member-type header at depth 0 naming the failing tuple member; \
         got {chain:?}"
    );
    assert!(
        chain_has(&chain, 1, |m| m.contains("position 0")),
        "expected the TS2626 positional line at depth 1; got {chain:?}"
    );
    assert!(
        chain_has(&chain, 2, |m| m.contains("'number'")
            && m.contains("'string'")
            && m.contains("is not assignable")),
        "expected the leaf element relation at depth 2; got {chain:?}"
    );
}

/// Renamed binders: the same structural rule must hold with different alias and
/// label spellings, so a name-hardcoded fix would not satisfy this.
#[test]
fn union_member_tuple_element_mismatch_renamed() {
    let diags = diagnostics(
        r#"
type Row = [key: string, n: number];
type Either = [key: boolean, n: number] | [key: string, n: boolean];
declare const e: Either;
const r: Row = e;
"#,
    );
    let chain = ts2322_chain(&diags);
    assert!(
        chain_has(&chain, 0, |m| m.contains("is not assignable to type")),
        "renamed tuple union should still emit a member header; got {chain:?}"
    );
    assert!(
        chain_has(&chain, 1, |m| m.contains("position 0")),
        "renamed tuple union should still emit the positional line; got {chain:?}"
    );
}

/// Object union member failing on a property type mismatch elaborates the
/// member header, the `Types of property 'p' are incompatible.` line, and the
/// leaf relation.
#[test]
fn union_member_object_property_mismatch_emits_header_and_drill() {
    let diags = diagnostics(
        r#"
type Target = { id: string; count: number };
type Source = { id: number; count: number } | { id: string; count: string };
declare const s: Source;
const t: Target = s;
"#,
    );
    let chain = ts2322_chain(&diags);
    assert!(
        chain_has(&chain, 0, |m| m.contains("is not assignable to type")
            && m.contains("id")),
        "expected a member-type header at depth 0 naming the failing object member; \
         got {chain:?}"
    );
    assert!(
        chain_has(&chain, 1, |m| m.contains("property 'id'")
            && m.contains("incompatible")),
        "expected the `Types of property 'id' are incompatible.` line at depth 1; \
         got {chain:?}"
    );
    assert!(
        chain_has(&chain, 2, |m| m.contains("'number'")
            && m.contains("'string'")),
        "expected the leaf property relation at depth 2; got {chain:?}"
    );
}

/// Member selection matches `tsc`'s order-dependent rule: `tsc` (and tsz) pick
/// the **first failing member in source order** and elaborate *that* member's
/// position/leaf. The forward union (`[number, number] | [string, string]`)
/// fails on its first member at position 0 (`number` -> `string`); the reversed
/// union (`[string, string] | [number, number]`) fails on *its* first member at
/// position 1 (`string` -> `number`). Asserting the concrete member type,
/// position, and leaf relation per ordering proves the *right* member is
/// selected (not merely that both chains share the TS2322/TS2626 shape, which
/// would not catch a contradictory member pick).
#[test]
fn union_member_selection_matches_tsc_first_failing_member_per_order() {
    let forward = ts2322_chain(&diagnostics(
        r#"
type Target = [id: string, count: number];
type Source = [id: number, count: number] | [id: string, count: string];
declare const s: Source;
const t: Target = s;
"#,
    ));
    // forward: first member `[id: number, count: number]`, position 0, number -> string
    assert!(
        chain_has(&forward, 0, |m| m.contains("number")) // member header names the number-keyed member
            && chain_has(&forward, 1, |m| m.contains("position 0"))
            && chain_has(&forward, 2, |m| m.contains("'number'") && m.contains("'string'")),
        "forward union should select its first failing member at position 0 \
         (number -> string); got {forward:?}"
    );

    let reversed = ts2322_chain(&diagnostics(
        r#"
type Target = [id: string, count: number];
type Source = [id: string, count: string] | [id: number, count: number];
declare const s: Source;
const t: Target = s;
"#,
    ));
    // reversed: first member `[id: string, count: string]`, position 1, string -> number
    assert!(
        chain_has(&reversed, 1, |m| m.contains("position 1"))
            && chain_has(&reversed, 2, |m| m.contains("'string'")
                && m.contains("'number'")),
        "reversed union should select its first failing member at position 1 \
         (string -> number); got {reversed:?}"
    );
}

/// Determinism: the same union, checked twice, must produce a byte-identical
/// elaboration chain. This is the actual anti-"contradictory" guarantee — the
/// failing member/position/leaf must not alternate across runs due to traversal
/// or cache-insertion order (the `checker-23-20` symptom).
#[test]
fn union_member_elaboration_is_deterministic_across_runs() {
    let source = r#"
type Target = [id: string, count: number];
type Source = [id: number, count: number] | [id: string, count: string];
declare const s: Source;
const t: Target = s;
"#;
    let first = ts2322_chain(&diagnostics(source));
    let second = ts2322_chain(&diagnostics(source));
    assert!(
        !first.is_empty(),
        "expected an elaboration chain; got {first:?}"
    );
    assert_eq!(
        first, second,
        "the same union must elaborate an identical chain across runs \
         (stable, non-contradictory member selection); \
         first={first:?} second={second:?}"
    );
}

/// Self-heading members are unchanged: a missing-property union member keeps the
/// bare `Property 'a' is missing ...` elaboration with no spurious member header.
#[test]
fn union_member_missing_property_unchanged() {
    let diags = diagnostics(
        r#"
interface Target { a: 1 }
interface Other { b: 2 }
declare const u: Target | Other;
const v: Target = u;
"#,
    );
    let chain = ts2322_chain(&diags);
    assert!(
        chain_has(&chain, 0, |m| m.contains("Property 'a' is missing")
            && m.contains("but required in type")),
        "missing-property union member should keep its self-heading elaboration \
         with no extra member header; got {chain:?}"
    );
}

/// Array-element union member: a `number[]` member assigned where `string[]` is
/// required self-heads with its own `Type 'number[]' is not assignable to type
/// 'string[]'.` line (which doubles as the member header) and then drills into
/// the element relation one indent deeper — matching tsc. Before the fix the
/// chain stopped at the bare top-level union line.
#[test]
fn union_member_array_element_mismatch_emits_header_and_drill() {
    let diags = diagnostics(
        r#"
declare const s: string[] | number[];
const t: string[] = s;
"#,
    );
    let chain = ts2322_chain(&diags);
    assert!(
        chain_has(&chain, 0, |m| m.contains("'number[]'")
            && m.contains("'string[]'")
            && m.contains("is not assignable")),
        "expected the array member header at depth 0; got {chain:?}"
    );
    assert!(
        chain_has(&chain, 1, |m| m.contains("'number'")
            && m.contains("'string'")
            && m.contains("is not assignable")),
        "expected the element leaf relation at depth 1 (directly beneath the \
         member header, not over-indented); got {chain:?}"
    );
}

/// Array-of-object union member drills member header -> element header ->
/// `Types of property` -> leaf, each exactly one indent deeper. This pins the
/// depth composition (the element drill must sit beneath the member line, not a
/// level too deep).
#[test]
fn union_member_array_of_object_drills_each_level_one_indent() {
    let diags = diagnostics(
        r#"
declare const s: { a: string }[] | { a: number }[];
const t: { a: string }[] = s;
"#,
    );
    let chain = ts2322_chain(&diags);
    assert!(
        chain_has(&chain, 0, |m| m.contains("is not assignable")
            && m.contains("[]")),
        "expected the array member header at depth 0; got {chain:?}"
    );
    assert!(
        chain_has(&chain, 2, |m| m.contains("property 'a'")
            && m.contains("incompatible")),
        "expected `Types of property 'a' are incompatible.` at depth 2; \
         got {chain:?}"
    );
    assert!(
        chain_has(&chain, 3, |m| m.contains("'number'")
            && m.contains("'string'")),
        "expected the leaf property relation at depth 3; got {chain:?}"
    );
}

/// Readonly-tuple union member self-heads with the readonly elaboration (no
/// extra member header), matching tsc.
#[test]
fn union_member_readonly_tuple_self_heads() {
    let diags = diagnostics(
        r#"
declare const s: readonly [number] | [string];
const t: [string] = s;
"#,
    );
    let chain = ts2322_chain(&diags);
    assert!(
        chain_has(&chain, 0, |m| m.contains("'readonly'")
            && m.contains("cannot be assigned to the mutable type")),
        "expected the readonly-to-mutable self-heading line at depth 0; \
         got {chain:?}"
    );
}

/// Renamed binders: the array-element rule must hold regardless of the alias
/// spelling, so a name-keyed fix would not satisfy this.
#[test]
fn union_member_array_element_mismatch_renamed() {
    let diags = diagnostics(
        r#"
type Elems = boolean[] | string[];
declare const xs: Elems;
const ys: boolean[] = xs;
"#,
    );
    let chain = ts2322_chain(&diags);
    assert!(
        chain_has(&chain, 0, |m| m.contains("'string[]'")
            && m.contains("'boolean[]'")),
        "renamed array union should still emit the member header; got {chain:?}"
    );
    assert!(
        chain_has(&chain, 1, |m| m.contains("'string'")
            && m.contains("'boolean'")),
        "renamed array union should still drill the element leaf at depth 1; \
         got {chain:?}"
    );
}

// ---------------------------------------------------------------------------
// Function-signature and index-signature union members.
//
// `tsc` elaborates a failing union member that is a function or an
// index-signatured object exactly like the non-union case, beneath the member
// header. Before this slice, tsz dropped these member shapes entirely (the
// solver's union-source walk only surfaced tuple/property/array members), so a
// function-return, function-parameter, or index-signature member collapsed the
// chain to the bare top-level union line and hid which member — and which
// position/signature — was responsible.
//
// Ground truth (`tsc` 6.0.2):
//
// ```text
// Type 'Source' is not assignable to type 'Target'.
//   Type '(x: string) => string' is not assignable to type 'Target'.
//     Type 'string' is not assignable to type 'number'.
// ```
// ```text
// Type 'Source' is not assignable to type 'Target'.
//   Type '(x: number) => void' is not assignable to type 'Target'.
//     Types of parameters 'x' and 'x' are incompatible.
//       Type 'string' is not assignable to type 'number'.
// ```
// ```text
// Type 'Source' is not assignable to type 'Target'.
//   Type '{ [k: string]: string; }' is not assignable to type 'Target'.
//     'string' index signatures are incompatible.
//       Type 'string' is not assignable to type 'number'.
// ```
// ---------------------------------------------------------------------------

/// Function-return union member: the member header (the function type) sits one
/// indent beneath the union line and the return relation drills directly
/// beneath it — `tsc` emits no intermediate `Return type …` frame for a direct
/// function-to-function return mismatch.
#[test]
fn union_member_function_return_mismatch_emits_header_and_drill() {
    let diags = diagnostics(
        r#"
type Target = (x: string) => number;
type Source = ((x: string) => string) | ((x: string) => boolean);
declare const s: Source;
const t: Target = s;
"#,
    );
    let chain = ts2322_chain(&diags);
    assert!(
        chain_has(&chain, 0, |m| m.contains("=> string")
            && m.contains("is not assignable to type")),
        "expected the function member header (the `=> string` member) at depth 0; \
         got {chain:?}"
    );
    assert!(
        chain_has(&chain, 1, |m| m.contains("'string'")
            && m.contains("'number'")
            && m.contains("is not assignable")),
        "expected the return relation leaf at depth 1, directly beneath the member \
         header (no `Return type …` frame); got {chain:?}"
    );
    assert!(
        !chain.iter().any(|(_, _, m)| m.starts_with("Return type ")),
        "tsc relates the return types directly; no `Return type …` frame should \
         appear; got {chain:?}"
    );
}

/// Member selection for function-return unions follows `tsc`'s first-failing
/// member rule: reversing the union order changes which member (and which
/// return type) heads the drill, but the chain stays internally consistent.
#[test]
fn union_member_function_return_selection_matches_tsc_order() {
    let reversed = ts2322_chain(&diagnostics(
        r#"
type Target = (x: string) => number;
type Source = ((x: string) => boolean) | ((x: string) => string);
declare const s: Source;
const t: Target = s;
"#,
    ));
    // reversed: first failing member is the `=> boolean` function.
    assert!(
        chain_has(&reversed, 0, |m| m.contains("=> boolean")),
        "reversed function union should head with the `=> boolean` member; \
         got {reversed:?}"
    );
    assert!(
        chain_has(&reversed, 1, |m| m.contains("'boolean'")
            && m.contains("'number'")),
        "reversed function union should drill `boolean` -> `number` at depth 1; \
         got {reversed:?}"
    );
}

/// Function-parameter union member self-heads with the signature relation (its
/// own first line doubles as the member header), then drills the
/// contravariant `Types of parameters 'a' and 'b' are incompatible.` frame and
/// the parameter leaf — each one indent deeper.
#[test]
fn union_member_function_parameter_mismatch_emits_header_and_drill() {
    let diags = diagnostics(
        r#"
type Target = (x: string) => void;
type Source = ((x: number) => void) | ((x: boolean) => void);
declare const s: Source;
const t: Target = s;
"#,
    );
    let chain = ts2322_chain(&diags);
    assert!(
        chain_has(&chain, 0, |m| m.contains("(x: number) => void")
            && m.contains("is not assignable to type")),
        "expected the function member header at depth 0; got {chain:?}"
    );
    assert!(
        chain_has(&chain, 1, |m| m.contains("parameters 'x' and 'x'")
            && m.contains("incompatible")),
        "expected the `Types of parameters 'x' and 'x' are incompatible.` frame at \
         depth 1; got {chain:?}"
    );
    assert!(
        chain_has(&chain, 2, |m| m.contains("'string'")
            && m.contains("'number'")),
        "expected the contravariant parameter leaf at depth 2; got {chain:?}"
    );
}

/// Renamed binders: the function-parameter rule must hold regardless of alias
/// or parameter-name spelling, so a name-keyed fix would not satisfy this.
#[test]
fn union_member_function_parameter_mismatch_renamed() {
    let diags = diagnostics(
        r#"
type Handler = (value: string) => void;
type Either = ((value: number) => void) | ((value: boolean) => void);
declare const e: Either;
const h: Handler = e;
"#,
    );
    let chain = ts2322_chain(&diags);
    assert!(
        chain_has(&chain, 0, |m| m.contains("=> void")
            && m.contains("is not assignable")),
        "renamed function union should still emit the member header; got {chain:?}"
    );
    assert!(
        chain_has(&chain, 1, |m| m.contains("parameters 'value' and 'value'")
            && m.contains("incompatible")),
        "renamed function union should still emit the parameters frame; got {chain:?}"
    );
}

/// Index-signature union member: the member header sits beneath the union line,
/// then `'string' index signatures are incompatible.` and the value-type leaf
/// drill one indent deeper each.
#[test]
fn union_member_index_signature_mismatch_emits_header_and_drill() {
    let diags = diagnostics(
        r#"
type Target = { [k: string]: number };
type Source = { [k: string]: string } | { [k: string]: boolean };
declare const s: Source;
const t: Target = s;
"#,
    );
    let chain = ts2322_chain(&diags);
    assert!(
        chain_has(&chain, 0, |m| m.contains("[k: string]")
            && m.contains("is not assignable to type")),
        "expected the index-signature member header at depth 0; got {chain:?}"
    );
    assert!(
        chain_has(&chain, 1, |m| m.contains("'string' index signatures")
            && m.contains("incompatible")),
        "expected the `'string' index signatures are incompatible.` line at depth 1; \
         got {chain:?}"
    );
    assert!(
        chain_has(&chain, 2, |m| m.contains("'string'")
            && m.contains("'number'")),
        "expected the value-type leaf at depth 2; got {chain:?}"
    );
}

/// Number-keyed index signatures elaborate the same way, with the `'number'`
/// index-kind reflected in the incompatibility line.
#[test]
fn union_member_number_index_signature_mismatch() {
    let diags = diagnostics(
        r#"
type Target = { [k: number]: number };
type Source = { [k: number]: string } | { [k: number]: boolean };
declare const s: Source;
const t: Target = s;
"#,
    );
    let chain = ts2322_chain(&diags);
    assert!(
        chain_has(&chain, 1, |m| m.contains("'number' index signatures")
            && m.contains("incompatible")),
        "expected the `'number' index signatures are incompatible.` line at depth 1; \
         got {chain:?}"
    );
}

/// Determinism for the new member shapes: the same function/index union checked
/// twice must produce a byte-identical chain (the anti-"contradictory"
/// guarantee — the failing member/position/leaf must not alternate across runs).
#[test]
fn union_member_function_and_index_elaboration_is_deterministic() {
    for source in [
        r#"
type Target = (x: string) => number;
type Source = ((x: string) => string) | ((x: string) => boolean);
declare const s: Source;
const t: Target = s;
"#,
        r#"
type Target = { [k: string]: number };
type Source = { [k: string]: string } | { [k: string]: boolean };
declare const s: Source;
const t: Target = s;
"#,
    ] {
        let first = ts2322_chain(&diagnostics(source));
        let second = ts2322_chain(&diagnostics(source));
        assert!(
            !first.is_empty(),
            "expected an elaboration chain; got {first:?}"
        );
        assert_eq!(
            first, second,
            "the same union must elaborate an identical chain across runs; \
             first={first:?} second={second:?}"
        );
    }
}

/// Subtype-reducible written union: when a union annotation contains a member
/// that is a structural subtype of a sibling (`[string, string]` is assignable
/// to `string[]`), tsc keeps *both* members under `UnionReduction.Literal` and
/// elaborates the first failing member when the whole union is rejected. tsz
/// previously subtype-reduced the *resolved* source to the single widest member
/// (`string[]`) while still displaying the full union, which silently dropped
/// the union-member elaboration line entirely.
///
/// Structural rule: union-source member elaboration must walk the members the
/// printer displays (the literal-reduced annotation union), not a
/// subtype-reduced collapse of it.
#[test]
fn union_member_subtype_reducible_keeps_member_elaboration() {
    let diags = diagnostics(
        r#"
type Source = string[] | [string, string];
declare const s: Source;
const n: 1 = s;
"#,
    );
    let chain = ts2322_chain(&diags);
    assert!(
        chain_has(&chain, 0, |m| m.contains("string[]")
            && m.contains("is not assignable to type")
            && m.contains("'1'")),
        "subtype-reducible union must still drill into the failing member; got {chain:?}"
    );
}

/// Renamed binders + a flipped declaration order for the same subtype-reducible
/// union family: the member elaboration must not depend on alias spelling or on
/// which member is written first (the widest member `string[]` is the failing
/// member either way).
#[test]
fn union_member_subtype_reducible_renamed_and_reordered() {
    for source in [
        r#"
type Listish = string[] | [string, string];
declare const v: Listish;
const m: 0 = v;
"#,
        r#"
type Collection = [string, string] | string[];
declare const c: Collection;
const k: 0 = c;
"#,
    ] {
        let chain = ts2322_chain(&diagnostics(source));
        assert!(
            chain_has(&chain, 0, |m| m.contains("string[]")
                && m.contains("is not assignable to type")),
            "renamed/reordered subtype-reducible union must still drill the member; got {chain:?}"
        );
    }
}
