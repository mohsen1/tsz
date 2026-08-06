//! Regression tests for the `TS6502` `The expected type comes from the return
//! type of this signature.` pointer.
//!
//! Structural rule, pinned against `typescript@7.0.2` (the conformance pin,
//! run through `scripts/conformance/oracle.sh --strict --pretty --target
//! es2022 --lib es2022`): when an object-literal member's value is an
//! expression-bodied arrow and the elaboration drilled to the arrow's *body*,
//! the `TS2322` reported there carries a pointer at the **signature
//! declaration** the expected return type came from — not at the member's name,
//! which is the sibling `TS6500` anchor.
//!
//! Every anchor below is the byte range tsc underlines, taken from the oracle
//! run rather than from tsz's own output, and asserted through
//! [`span_text`] so a wrong-but-plausible span fails loudly.

use crate::diagnostics::Diagnostic;
use crate::test_utils::check_source_diagnostics;
use tsz_common::diagnostics::diagnostic_codes;

const TS2322: u32 = diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE;
const TS6500: u32 =
    diagnostic_codes::THE_EXPECTED_TYPE_COMES_FROM_PROPERTY_WHICH_IS_DECLARED_HERE_ON_TYPE;
const TS6502: u32 =
    diagnostic_codes::THE_EXPECTED_TYPE_COMES_FROM_THE_RETURN_TYPE_OF_THIS_SIGNATURE;

fn only(diags: &[Diagnostic], code: u32) -> Diagnostic {
    let matching: Vec<_> = diags.iter().filter(|d| d.code == code).collect();
    assert_eq!(
        matching.len(),
        1,
        "expected exactly one TS{code}; got {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
    matching[0].clone()
}

/// The pointer's `(start, length)` plus its message, so each case can assert
/// the anchor lands on the signature as written and nothing wider.
fn return_pointer(diagnostic: &Diagnostic) -> (u32, u32, String) {
    let pointers: Vec<_> = diagnostic
        .related_information
        .iter()
        .filter(|info| info.code == TS6502)
        .collect();
    assert_eq!(
        pointers.len(),
        1,
        "expected exactly one TS6502 pointer; got {:?}",
        diagnostic
            .related_information
            .iter()
            .map(|info| (info.code, info.message_text.clone()))
            .collect::<Vec<_>>()
    );
    (
        pointers[0].start,
        pointers[0].length,
        pointers[0].message_text.clone(),
    )
}

fn has_return_pointer(diagnostic: &Diagnostic) -> bool {
    diagnostic
        .related_information
        .iter()
        .any(|info| info.code == TS6502)
}

fn span_text(source: &str, start: u32, length: u32) -> &str {
    &source[start as usize..(start + length) as usize]
}

/// tsc, `a.ts`:
///
/// ```text
/// a.ts:2:29 - error TS2322: Type 'number' is not assignable to type 'string'.
///   a.ts:1:21 - The expected type comes from the return type of this signature.
///     1 interface Ret { cb: () => string; }
///                           ~~~~~~~~~~~~
/// ```
///
/// The anchor is the property's *annotation*, not its name: a function-type
/// annotation is the signature declaration tsc points at.
#[test]
fn property_signature_annotated_with_a_function_type_points_at_the_annotation() {
    let source = "interface Ret { cb: () => string; }\nconst rt: Ret = { cb: () => 6 };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2322);
    let (start, length, message) = return_pointer(&diagnostic);
    assert_eq!(span_text(source, start, length), "() => string");
    assert_eq!(
        message,
        "The expected type comes from the return type of this signature."
    );
}

/// A method signature has no separate annotation node, so tsc underlines the
/// whole member — trailing `;` included, exactly as the sibling `TS2728`
/// pointer already does for a method signature. Oracled:
///
/// ```text
/// d.ts:1:16 - The expected type comes from the return type of this signature.
///     1 interface R3 { m(): string; }
///                      ~~~~~~~~~~~~
/// ```
#[test]
fn method_signature_points_at_the_whole_member() {
    let source = "interface R3 { m(): string; }\nconst r3: R3 = { m: () => 8 };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2322);
    let (start, length) = {
        let (start, length, _) = return_pointer(&diagnostic);
        (start, length)
    };
    assert_eq!(span_text(source, start, length), "m(): string;");
}

/// The anchor is the member's own declaration and not anything keyed on how it
/// is spelled: renaming every binder must move the span and change nothing
/// else. Pinned because a pointer that reads a name is exactly the
/// hardcoding the repo's anti-hardcoding gate forbids.
#[test]
fn renamed_binders_anchor_at_their_own_signature() {
    let source = "interface Zeta { qux: () => string; }\nconst zz: Zeta = { qux: () => 6 };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2322);
    let (start, length, _) = return_pointer(&diagnostic);
    assert_eq!(span_text(source, start, length), "() => string");
}

/// Two structurally identical signatures are one interned `FunctionShape`, so
/// a pointer carried on the *type* would anchor both at whichever was interned
/// first. Each must point at its own text. This is the row that proves the
/// anchor comes from syntax, and the failure mode that sank tsz#16454.
#[test]
fn identical_signatures_each_point_at_their_own_text() {
    let source = concat!(
        "interface One { a: () => string; }\n",
        "interface Two { b: () => string; }\n",
        "const o: One = { a: () => 1 };\n",
        "const t: Two = { b: () => 2 };\n",
    );
    let diagnostics = check_source_diagnostics(source);
    let mismatches: Vec<_> = diagnostics.iter().filter(|d| d.code == TS2322).collect();
    assert_eq!(
        mismatches.len(),
        2,
        "expected two TS2322; got {mismatches:?}"
    );
    let anchors: Vec<u32> = mismatches
        .iter()
        .map(|d| return_pointer(d).0)
        .collect::<Vec<_>>();
    assert_ne!(
        anchors[0], anchors[1],
        "identical signatures must not share one anchor: {anchors:?}"
    );
    for &start in &anchors {
        assert_eq!(span_text(source, start, 12), "() => string");
    }
    assert!(
        anchors[0] < source.find("interface Two").expect("second interface") as u32,
        "the first pointer must anchor in the first interface: {anchors:?}"
    );
}

/// A cross-file owner points into the file that declares it, not into the file
/// the primary diagnostic lives in.
#[test]
fn cross_file_owner_points_at_its_own_declaration() {
    use crate::test_utils::check_multi_file;

    let dep = "export interface Dep { cb: () => string; }\n";
    let main = "import { Dep } from \"./dep\";\nconst d: Dep = { cb: () => 5 };\n";
    let diagnostics = check_multi_file(
        &[("dep.ts", dep), ("main.ts", main)],
        "main.ts",
        Default::default(),
    );
    let diagnostic = only(&diagnostics, TS2322);
    let pointer = diagnostic
        .related_information
        .iter()
        .find(|info| info.code == TS6502)
        .expect("TS6502 pointer");
    assert!(
        pointer.file.ends_with("dep.ts"),
        "pointer must anchor in the declaring file: {pointer:?}"
    );
    assert_eq!(
        span_text(dep, pointer.start, pointer.length),
        "() => string"
    );
}

/// A block-bodied function expression is not drilled at all — tsc's
/// `elaborateArrowFunction` only handles expression bodies — so the report
/// stays at the member and carries the `TS6500` property pointer instead.
/// Oracled: `b.ts:1:18 - The expected type comes from property 'cb' ...`.
#[test]
fn block_bodied_function_expression_keeps_the_property_pointer() {
    let source = "interface Ret2 { cb: () => string; }\nconst rt2: Ret2 = { cb: function () { return 6; } };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2322);
    assert!(
        !has_return_pointer(&diagnostic),
        "an undrilled member must not carry the return pointer: {diagnostic:?}"
    );
    assert!(
        diagnostic
            .related_information
            .iter()
            .any(|info| info.code == TS6500),
        "the member frame keeps its TS6500 pointer: {diagnostic:?}"
    );
}

/// A member typed through a *type alias* (`cb: Fn` for `type Fn = () => string`)
/// drills to the arrow body exactly as the inline form does, and the `TS6502`
/// anchors inside the alias's own body — the frame tsz#16542 pinned as wrong.
///
/// tsc, oracle `d.ts` (`typescript@7.0.2 --strict --pretty`):
///
/// ```text
/// d.ts:3:29 - error TS2322: Type 'number' is not assignable to type 'string'.
///   d.ts:1:11 - The expected type comes from the return type of this signature.
///     1 type Fn = () => string;
///                 ~~~~~~~~~~~~
/// ```
///
/// The message names `string` (the alias's resolved return type), the primary
/// lands on the offending `6`, and the pointer underlines `() => string` inside
/// `Fn`, not `Fn` and not `cb`. The sibling `TS6500` — the witness of the old
/// undrilled member frame — must be gone.
#[test]
fn a_type_reference_annotated_member_drills_into_the_alias_body() {
    let source =
        "type Fn = () => string;\ninterface Ref { cb: Fn; }\nconst rf: Ref = { cb: () => 6 };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2322);
    assert_eq!(
        diagnostic.message_text, "Type 'number' is not assignable to type 'string'.",
        "the drilled body names the alias's resolved return type: {diagnostic:?}"
    );
    assert_eq!(
        span_text(source, diagnostic.start, diagnostic.length),
        "6",
        "the primary anchors at the offending body expression: {diagnostic:?}"
    );
    let (start, length, _) = return_pointer(&diagnostic);
    assert_eq!(span_text(source, start, length), "() => string");
    assert_eq!(start, 10, "the anchor is inside Fn's body, not at Fn or cb");
    assert!(
        diagnostic
            .related_information
            .iter()
            .all(|info| info.code != TS6500),
        "the drilled frame drops the undrilled-branch TS6500 pointer: {diagnostic:?}"
    );
}

/// The generic-application form of the row above (tsz#16548): `cb: G<string>`
/// for `type G<T> = () => T`. The trap that split it out is that the two halves
/// disagree on instantiation — the message must name the *instantiated* return
/// type `string`, while the pointer anchors in the alias body *uninstantiated*,
/// underlining `() => T` (not `() => string`).
///
/// tsc, oracle `r6.ts`:
///
/// ```text
/// r6.ts:3:28 - error TS2322: Type 'number' is not assignable to type 'string'.
///   r6.ts:1:13 - The expected type comes from the return type of this signature.
///     1 type G<T> = () => T;
///                   ~~~~~~~
/// ```
#[test]
fn a_generic_alias_application_drills_with_instantiated_return_and_uninstantiated_anchor() {
    let source =
        "type G<T> = () => T;\ninterface R6 { cb: G<string>; }\nconst v6: R6 = { cb: () => 6 };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2322);
    assert_eq!(
        diagnostic.message_text, "Type 'number' is not assignable to type 'string'.",
        "the message names the instantiated return type, not the bare parameter T: {diagnostic:?}"
    );
    assert_eq!(span_text(source, diagnostic.start, diagnostic.length), "6");
    let (start, length, _) = return_pointer(&diagnostic);
    assert_eq!(
        span_text(source, start, length),
        "() => T",
        "the pointer anchors the alias body uninstantiated"
    );
    assert_eq!(start, 12);
    assert!(
        diagnostic
            .related_information
            .iter()
            .all(|info| info.code != TS6500),
    );
}

/// Renamed binders: the alias-drill rule is structural, not keyed on any user
/// name. Every binder is renamed and the anchor still lands inside the alias
/// body, only the span moving. Oracled by construction from the row above.
#[test]
fn a_renamed_alias_reference_keeps_the_drill_rule() {
    let source = "type Zed = () => string;\ninterface Zref { qux: Zed; }\nconst zz: Zref = { qux: () => 6 };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2322);
    assert_eq!(
        diagnostic.message_text,
        "Type 'number' is not assignable to type 'string'."
    );
    let (start, length, _) = return_pointer(&diagnostic);
    assert_eq!(span_text(source, start, length), "() => string");
    assert_eq!(start, 11);
}

/// An alias *chain* (`type B = A` for `type A = () => string`): the drill
/// resolves `B` to `A`'s function signature, and the anchor follows both hops to
/// the function type declared in `A`'s body.
#[test]
fn an_alias_chain_reference_is_followed_to_the_declaring_alias_body() {
    let source = "type A = () => string;\ntype B = A;\ninterface Rc { cb: B; }\nconst rc: Rc = { cb: () => 6 };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2322);
    assert_eq!(
        diagnostic.message_text,
        "Type 'number' is not assignable to type 'string'."
    );
    let (start, length, _) = return_pointer(&diagnostic);
    assert_eq!(span_text(source, start, length), "() => string");
    assert_eq!(start, 9, "the anchor is in A's body, the chain's terminus");
}

/// A two-parameter generic application (`G2<number, string>` for
/// `type G2<A, B> = () => B`): instantiation must select the *second* argument
/// for the return type, so the message names `string` while the uninstantiated
/// anchor underlines `() => B`.
#[test]
fn a_multi_parameter_generic_application_instantiates_the_right_argument() {
    let source = "type G2<A, B> = () => B;\ninterface R2 { cb: G2<number, string>; }\nconst v2: R2 = { cb: () => 6 };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2322);
    assert_eq!(
        diagnostic.message_text, "Type 'number' is not assignable to type 'string'.",
        "the return type is instantiated to the second argument: {diagnostic:?}"
    );
    let (start, length, _) = return_pointer(&diagnostic);
    assert_eq!(span_text(source, start, length), "() => B");
    assert_eq!(start, 16);
}

/// An alias chain in the *owner* position is followed — a different shape from
/// the row above, where the alias sits on the member. `Ind`'s own declaration
/// carries no member list, so the walk must continue through its body to reach
/// the literal that declares `cb`. Oracled: `k.ts:1:18`, inside `Zed`.
#[test]
fn an_alias_chain_in_the_owner_position_is_followed_to_the_declaring_literal() {
    let source =
        "type Zed = { cb: () => string };\ntype Ind = Zed;\nconst q: Ind = { cb: () => 6 };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2322);
    let (start, length, _) = return_pointer(&diagnostic);
    assert_eq!(span_text(source, start, length), "() => string");
    assert_eq!(start, 17, "the anchor is Zed's member, not Ind's body");
}

/// A type alias whose body is a type literal owns no interface declaration for
/// the symbol route to walk, so this is the anonymous-owner shape reached
/// through the annotation instead. Oracled: `a.ts:1:18`.
#[test]
fn a_type_alias_to_a_type_literal_anchors_at_its_member_signature() {
    let source = "type Lit = { cb: () => string };\nconst rt: Lit = { cb: () => 6 };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2322);
    let (start, length, _) = return_pointer(&diagnostic);
    assert_eq!(span_text(source, start, length), "() => string");
    assert_eq!(start, 17);
}

/// The method-signature shape of the row above: no separate annotation node, so
/// the whole member is underlined, trailing `;` included. Oracled: `e.ts:1:15`,
/// 12 columns.
#[test]
fn a_method_signature_inside_a_type_alias_is_underlined_whole() {
    let source = "type Lit2 = { m(): string; };\nconst rt2: Lit2 = { m: () => 6 };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2322);
    let (start, length, _) = return_pointer(&diagnostic);
    assert_eq!(span_text(source, start, length), "m(): string;");
}

/// Renamed binders: the same annotation shape under different user names keeps
/// the same anchor rule and only moves the span, so no identifier text can be
/// driving the decision. Oracled: `f.ts:1:20`.
#[test]
fn renamed_binders_keep_the_anchor_rule_and_only_move_the_span() {
    let source = "type Zeta = { qux: () => string };\nconst zz: Zeta = { qux: () => 6 };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2322);
    let (start, length, _) = return_pointer(&diagnostic);
    assert_eq!(span_text(source, start, length), "() => string");
    assert_eq!(start, 19);
}

/// A *nested* member's owner is an inner type literal, which mints no binder
/// symbol at all (tsz#16443) — the shape every owner candidate declines on.
/// Oracled: `b.ts:1:32`, inside the inner literal.
#[test]
fn a_nested_type_literal_owner_anchors_through_the_written_path() {
    let source = "interface Outer { inner: { cb: () => string }; }\nconst o: Outer = { inner: { cb: () => 6 } };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2322);
    let (start, length, _) = return_pointer(&diagnostic);
    assert_eq!(span_text(source, start, length), "() => string");
    assert_eq!(start, 31);
}

/// Two hops rather than one, so the walk is following the written path and not
/// merely unwrapping a single level. Oracled: `g.ts:1:41`.
#[test]
fn a_twice_nested_owner_walks_every_hop_of_the_written_path() {
    let source = "interface Outer2 { inner: { deep: { cb: () => string } }; }\nconst o2: Outer2 = { inner: { deep: { cb: () => 6 } } };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2322);
    let (start, length, _) = return_pointer(&diagnostic);
    assert_eq!(span_text(source, start, length), "() => string");
    assert_eq!(start, 40);
}

/// An inline annotation names no type at all, so the annotation node *is* the
/// owner and the path needs no hop. Oracled: `c.ts:1:17`.
#[test]
fn an_inline_type_literal_annotation_anchors_at_its_own_member() {
    let source = "const rt: { cb: () => string } = { cb: () => 6 };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2322);
    let (start, length, _) = return_pointer(&diagnostic);
    assert_eq!(span_text(source, start, length), "() => string");
    assert_eq!(start, 16);
}

/// Parentheses are peeled: tsc underlines the function type itself, not the
/// parenthesized wrapper. Oracled: `j.ts:1:19`, 12 columns — the `(` at column
/// 18 is outside the underline.
#[test]
fn a_parenthesized_function_type_annotation_anchors_inside_the_parentheses() {
    let source = "type Par = { cb: (() => string) };\nconst pp: Par = { cb: () => 6 };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2322);
    let (start, length, _) = return_pointer(&diagnostic);
    assert_eq!(span_text(source, start, length), "() => string");
}

/// The negative control for the paren peel: a member whose annotation merely
/// *contains* a signature must never borrow one. `callable_type_node_anchor`
/// peels the parentheses, finds a union, and declines because a union declares
/// no signature of its own.
///
/// tsz drills this body where tsc does not — tsc reports the whole function
/// type at the member with a `TS6500` pointer, tsz reports at the `6` — a
/// pre-existing primary divergence this diff neither causes nor moves. Asserted
/// only as "no `TS6502`", so the row pins the walk's own decision and stays
/// honest about the frame disagreement above it.
#[test]
fn a_union_annotation_takes_no_return_pointer() {
    let source =
        "type Uni = { cb: (() => string) | undefined };\nconst uu: Uni = { cb: () => 6 };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2322);
    assert!(
        !has_return_pointer(&diagnostic),
        "a union annotation declares no signature to point at: {diagnostic:?}"
    );
}

/// A call argument's expected type comes from a *parameter*, not from an
/// owner's member list, so this walk has no owner to resolve and declines.
/// tsc does emit `TS6502` here (anchored in the alias body); pinned negatively
/// so the argument route is a visible gap rather than a wrong anchor.
#[test]
fn call_argument_return_mismatch_declines() {
    let source = "type Fn = () => string;\ndeclare function take(f: Fn): void;\ntake(() => 7);\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2322);
    assert!(
        !has_return_pointer(&diagnostic),
        "a call-argument frame has no member owner to anchor on: {diagnostic:?}"
    );
}

/// The negative arm of the whole family: a member whose value matches its
/// declared return type produces no diagnostic, so there is nothing to point
/// at. Guards against an attach that fires on an unrelated buffered `TS2322`.
#[test]
fn a_matching_return_type_produces_no_diagnostic_and_no_pointer() {
    let source = "interface Ok { cb: () => string; }\nconst ok: Ok = { cb: () => \"s\" };\n";
    let diagnostics = check_source_diagnostics(source);
    assert!(
        diagnostics.iter().all(|d| d.code != TS2322),
        "a matching return type must not report: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().all(|d| !has_return_pointer(d)),
        "no pointer without a report: {diagnostics:?}"
    );
}
