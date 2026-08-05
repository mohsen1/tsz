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

/// A member annotated with a type *reference* declines. tsc anchors inside the
/// alias body (`type Fn = () => string` → the `() => string` there), which
/// needs an alias hop this walk deliberately does not take; declining leaves
/// output exactly as it was rather than anchoring at `Fn` or at `cb`. Pinned so
/// the day the hop lands, this row's move is visible.
#[test]
fn type_reference_annotation_declines_rather_than_anchoring_at_the_reference() {
    let source =
        "type Fn = () => string;\ninterface Ref { cb: Fn; }\nconst rf: Ref = { cb: () => 6 };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2322);
    assert!(
        !has_return_pointer(&diagnostic),
        "an alias-annotated member must decline: {diagnostic:?}"
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
