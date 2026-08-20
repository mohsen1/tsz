//! Regression tests for the `TS6500` `The expected type comes from property
//! '{0}' which is declared here on type '{1}'` pointer that tsc attaches to the
//! per-property leaf of an object-literal assignment.
//!
//! Structural rule (pinned against `typescript@7.0.2`, the conformance pin):
//! when an object-literal member's value fails assignment to the target
//! property's declared type, tsc's `elaborateElementwise` anchors the `TS2322`
//! at the member's name and attaches a related-information pointer at the
//! *target* property's own declaration. When the elaboration drilled further —
//! an array element, an arrow body — the inner frame owns the report and tsc's
//! `!issuedElaboration` guard emits no pointer at this level.
//!
//! Two divergences from the sibling `TS2728` pointer, both oracle-pinned and
//! both load-bearing:
//!
//! * `TS6500`'s property operand goes through tsc's `symbolToString`, so a
//!   string-literal member reads `property 'two-part'` where `TS2728` reads
//!   `'"two-part"' is declared here.`.
//! * the message carries no trailing period.
//!
//! Two shapes deliberately produce **no** pointer rather than a guessed one,
//! and are pinned negatively below so they cannot regress into a wrong anchor:
//! an index-signature target (tsc: `TS6501`) and a contextual return type
//! inside a property initializer (tsc: `TS6502`).
//!
//! An owner that is an anonymous object type — a type alias to a type
//! literal, or a nested literal — has no binder symbol to resolve through
//! (#16443: a type literal mints none at all), so the owner-candidate walk
//! alone declines. The annotation-syntax fallback recovers it anyway, from
//! the object-literal path at the failure site — the same per-occurrence
//! provenance argument #16521 already established for the sibling `TS2728`
//! pointer, since an interned anonymous shape can never carry its own source
//! location for the direct route to find.

use crate::diagnostics::Diagnostic;
use crate::test_utils::check_source_diagnostics;
use tsz_common::diagnostics::diagnostic_codes;

const TS2322: u32 = diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE;
const TS6500: u32 =
    diagnostic_codes::THE_EXPECTED_TYPE_COMES_FROM_PROPERTY_WHICH_IS_DECLARED_HERE_ON_TYPE;

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

/// The pointer's `(file, start, length)` plus its message, so each case can
/// assert the anchor lands on the declared member and nothing wider.
fn expected_type_pointer(diagnostic: &Diagnostic) -> (String, u32, u32, String) {
    let pointers: Vec<_> = diagnostic
        .related_information
        .iter()
        .filter(|info| info.code == TS6500)
        .collect();
    assert_eq!(
        pointers.len(),
        1,
        "expected exactly one TS6500 pointer; got {:?}",
        diagnostic
            .related_information
            .iter()
            .map(|info| (info.code, info.message_text.clone()))
            .collect::<Vec<_>>()
    );
    (
        pointers[0].file.clone(),
        pointers[0].start,
        pointers[0].length,
        pointers[0].message_text.clone(),
    )
}

fn has_pointer(diagnostic: &Diagnostic) -> bool {
    diagnostic
        .related_information
        .iter()
        .any(|info| info.code == TS6500)
}

fn span_text(source: &str, start: u32, length: u32) -> &str {
    &source[start as usize..(start + length) as usize]
}

/// The pointer models tsc's `relatedInformation`, not a `messageText` chain
/// link. Oracled on `typescript@7.0.2`: `--pretty false` prints the `TS2322`
/// line alone, `--pretty` prints the pointer with its own location and snippet.
/// Only the tag distinguishes the two — a chain link carries a real file and a
/// real span exactly as this entry does.
#[test]
fn expected_type_pointer_is_tagged_as_a_cross_location_pointer() {
    let source = "interface Outer { inner: string; }\nconst r: Outer = { inner: 1 };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2322);
    let pointer = diagnostic
        .related_information
        .iter()
        .find(|info| info.code == TS6500)
        .expect("TS6500 pointer");
    assert!(
        pointer.is_location_pointer(),
        "TS6500 must be a cross-location pointer: {pointer:?}"
    );
    assert!(
        diagnostic
            .related_information
            .iter()
            .filter(|info| info.code != TS6500)
            .all(|info| !info.is_location_pointer()),
        "elaboration links on the same diagnostic stay chain links: {diagnostic:?}"
    );
}

#[test]
fn interface_property_points_at_its_declaration() {
    let source = "interface Outer { inner: string; keep: number; }\nconst r: Outer = { inner: 1, keep: 2 };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2322);
    let (file, start, length, message) = expected_type_pointer(&diagnostic);
    assert_eq!(
        message,
        "The expected type comes from property 'inner' which is declared here on type 'Outer'"
    );
    assert_eq!(span_text(source, start, length), "inner");
    assert_eq!(file, "test.ts");
}

/// Binder names must not matter: the anchor comes from the target's own member
/// table, not from a name match against the reported text.
#[test]
fn renamed_binders_point_at_the_renamed_declaration() {
    let source = "interface Coord { alpha: string; omega: number; }\nconst c: Coord = { alpha: 1, omega: 2 };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2322);
    let (_, start, length, message) = expected_type_pointer(&diagnostic);
    assert_eq!(
        message,
        "The expected type comes from property 'alpha' which is declared here on type 'Coord'"
    );
    assert_eq!(span_text(source, start, length), "alpha");
}

#[test]
fn class_property_points_at_its_declaration() {
    let source = "class Klass { field: string = \"\"; other = 0; }\nconst k: Klass = { field: 3, other: 4 };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2322);
    let (_, start, length, message) = expected_type_pointer(&diagnostic);
    assert_eq!(
        message,
        "The expected type comes from property 'field' which is declared here on type 'Klass'"
    );
    assert_eq!(span_text(source, start, length), "field");
}

/// A call argument reaches the same elaboration as a direct assignment, so the
/// pointer follows the parameter's declared type rather than the call site.
#[test]
fn call_argument_property_points_at_the_parameter_type_declaration() {
    let source =
        "interface ArgT { q: string; }\nfunction take(a: ArgT) { return a; }\ntake({ q: 8 });\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2322);
    let (_, start, length, message) = expected_type_pointer(&diagnostic);
    assert_eq!(
        message,
        "The expected type comes from property 'q' which is declared here on type 'ArgT'"
    );
    assert_eq!(span_text(source, start, length), "q");
}

/// An interface *method signature* is underlined whole, semicolon included —
/// the same anchor rule `TS2728` follows, and the reason both share one walk.
#[test]
fn method_signature_member_is_underlined_whole() {
    let source = "interface M { run(x: number): void; }\nconst m: M = { run: 1 };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2322);
    let (_, start, length, message) = expected_type_pointer(&diagnostic);
    assert_eq!(
        message,
        "The expected type comes from property 'run' which is declared here on type 'M'"
    );
    assert_eq!(span_text(source, start, length), "run(x: number): void;");
}

/// The quoted-name divergence from `TS2728`: the underline keeps the quotes the
/// member was written with, the message does not.
#[test]
fn string_literal_member_name_is_unquoted_in_the_message_and_quoted_in_the_anchor() {
    let source = "interface SL { \"two-part\": string; }\nconst sl: SL = { \"two-part\": 2 };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2322);
    let (_, start, length, message) = expected_type_pointer(&diagnostic);
    assert_eq!(
        message,
        "The expected type comes from property 'two-part' which is declared here on type 'SL'"
    );
    assert_eq!(span_text(source, start, length), "\"two-part\"");
}

#[test]
fn optional_member_points_at_its_name_not_its_question_mark() {
    let source = "interface Opt { maybe?: string; }\nconst o: Opt = { maybe: 3 };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2322);
    let (_, start, length, message) = expected_type_pointer(&diagnostic);
    assert_eq!(
        message,
        "The expected type comes from property 'maybe' which is declared here on type 'Opt'"
    );
    assert_eq!(span_text(source, start, length), "maybe");
}

/// A union-typed member still resolves through the same member table; the
/// owner display is the declaring type, never the member's own type.
#[test]
fn union_typed_member_names_the_declaring_type() {
    let source = "interface Union { u: string | number; }\nconst un: Union = { u: true };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2322);
    let (_, _, _, message) = expected_type_pointer(&diagnostic);
    assert_eq!(
        message,
        "The expected type comes from property 'u' which is declared here on type 'Union'"
    );
}

/// A property declared in another file points into *that* file. The owner
/// symbol is read out of the binder that declares it, so a per-file binder's
/// raw `SymbolId` cannot anchor the pointer in the checking file by accident.
#[test]
fn cross_file_property_points_into_the_declaring_file() {
    const DEP: &str = "export interface Cross { one: string; }\n";
    const ENTRY: &str = "import { Cross } from \"./dep\";\nconst c: Cross = { one: 1 };\n";
    let diagnostics = crate::test_utils::check_multi_file_with_libs_stamped(
        &[("dep.ts", DEP), ("test.ts", ENTRY)],
        "test.ts",
        crate::context::CheckerOptions::default(),
        &[],
    );
    let diagnostic = only(&diagnostics, TS2322);
    let (file, start, length, message) = expected_type_pointer(&diagnostic);
    assert_eq!(
        message,
        "The expected type comes from property 'one' which is declared here on type 'Cross'"
    );
    assert_eq!(span_text(DEP, start, length), "one");
    assert!(
        file.ends_with("dep.ts"),
        "pointer must name the declaring file, got {file}"
    );
}

// --- negative rows: no pointer beats a guessed one -------------------------

/// tsc reports `TS6501` (`The expected type comes from this index signature.`)
/// here, not `TS6500`. Until that sibling is wired, the correct output is no
/// pointer at all rather than one naming a property that was never declared.
#[test]
fn index_signature_target_gets_no_property_pointer() {
    let source = "interface Idx { [k: string]: string; }\nconst ix: Idx = { any: 5 };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2322);
    assert!(
        !has_pointer(&diagnostic),
        "an index-signature target must not claim a declared property: {diagnostic:?}"
    );
}

/// tsc reports `TS6502` (`The expected type comes from the return type of this
/// signature.`) at the arrow's return, anchored at the signature — a different
/// code at a different anchor, so the property pointer must stay off.
#[test]
fn contextual_return_type_mismatch_gets_no_property_pointer() {
    let source = "interface Ret { cb: () => string; }\nconst rt: Ret = { cb: () => 6 };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2322);
    assert!(
        !has_pointer(&diagnostic),
        "a return-type frame must not carry the property pointer: {diagnostic:?}"
    );
}

/// tsc emits no related entry for an array *element* mismatch: the element is
/// the frame it reported at, and the enclosing property's `!issuedElaboration`
/// guard suppresses the outer pointer.
#[test]
fn array_element_mismatch_gets_no_property_pointer() {
    let source = "interface Arr { items: string[]; }\nconst ar: Arr = { items: [9] };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2322);
    assert!(
        !has_pointer(&diagnostic),
        "an array-element frame must not carry the enclosing property's pointer: {diagnostic:?}"
    );
}

/// A nested literal reports at the inner property; tsc's pointer then names the
/// *inner* anonymous type. The owner has no binder symbol at all — a type
/// literal mints none (#16443) — so `member_declaration_anchor_for_owner`
/// still declines, but the annotation-syntax fallback recovers the same
/// per-occurrence path #16521 gave the sibling `TS2728` pointer: walk the
/// object-literal ancestry from the failing value out to `Deep`, then back
/// down through `lvl`'s own written type to `p`.
#[test]
fn nested_type_literal_owner_points_at_the_inner_anonymous_type() {
    let source = "interface Deep { lvl: { p: string }; }\nconst d: Deep = { lvl: { p: 7 } };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2322);
    let (_, start, length, message) = expected_type_pointer(&diagnostic);
    assert_eq!(
        message,
        "The expected type comes from property 'p' which is declared here on type '{ p: string; }'"
    );
    assert_eq!(span_text(source, start, length), "p");
}

/// Same root cause as above with the alias spelled out: `type Alias = { .. }`
/// evaluates to an anonymous object type carrying no symbol, so the direct
/// owner-candidate walk declines. Unlike the nested case, the annotation walk
/// needs no hop at all — `path` is just `["av"]` — so it stops on the
/// `Alias` type-reference node itself, which is also why the owner displays
/// as the alias's own written name rather than its expanded body.
#[test]
fn type_alias_to_type_literal_owner_points_at_its_member() {
    let source = "type Alias = { av: string; bv: number };\nconst al: Alias = { av: 5, bv: 6 };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2322);
    let (_, start, length, message) = expected_type_pointer(&diagnostic);
    assert_eq!(
        message,
        "The expected type comes from property 'av' which is declared here on type 'Alias'"
    );
    assert_eq!(span_text(source, start, length), "av");
}

/// Binder names must not matter here either: the annotation-syntax fallback
/// matches members by the path recovered from the object-literal source, not
/// by any name coincidence with the primary diagnostic's own display text.
#[test]
fn renamed_binders_point_at_the_inner_anonymous_type() {
    let source = "interface Zeta { qux: { xylo: string; yak: number }; }\nconst z: Zeta = { qux: { xylo: 9 } };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2322);
    let (_, start, length, message) = expected_type_pointer(&diagnostic);
    assert_eq!(
        message,
        "The expected type comes from property 'xylo' which is declared here on type '{ xylo: string; yak: number; }'"
    );
    assert_eq!(span_text(source, start, length), "xylo");
}

/// Negative control for both rows above: a nested literal that type-checks
/// cleanly must stay clean — the fallback only ever activates once a `TS2322`
/// has already been reported for this member, never as a side effect of
/// walking the annotation.
#[test]
fn nested_type_literal_matching_value_stays_clean() {
    let source =
        "interface Deep { lvl: { p: string }; }\nconst d: Deep = { lvl: { p: \"ok\" } };\n";
    assert!(
        check_source_diagnostics(source).is_empty(),
        "a matching nested value must not report anything"
    );
}

// ---------------------------------------------------------------------------
// Nested object literals: the pointer rides a *missing-property* leaf
// ---------------------------------------------------------------------------
//
// When the nested literal is missing a required member rather than mismatching
// one, the leaf tsc reports is `TS2741` / `TS2739` / `TS2740`, not `TS2322`.
// tsc attaches the same `TS6500` pointer to it, naming the *enclosing* property
// and the type that declares that property. Oracled on `typescript@7.0.2` with
// `--noEmit --strict --pretty --target es2022 --lib es2022`.
//
// The multi-property rows are what force this to be its own attach: tsc emits
// `TS6500` there with **no** `TS2728` at all, so the pointer cannot be folded
// into the sibling declared-here site (#16443 item 1, Serpentine).

const TS2728: u32 = diagnostic_codes::IS_DECLARED_HERE;
const TS2739: u32 = diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE;
const TS2740: u32 = diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE;
const TS2741: u32 = diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE;

fn has_declared_here(diagnostic: &Diagnostic) -> bool {
    diagnostic
        .related_information
        .iter()
        .any(|info| info.code == TS2728)
}

/// `nested.ts:1:16` — `The expected type comes from property 'inner' which is
/// declared here on type 'Outer'`, alongside the `TS2728` at `1:37`.
#[test]
fn nested_literal_missing_property_points_at_the_enclosing_property() {
    let source = "type Outer = { inner: { op: number; oq: number } };\nconst r: Outer = { inner: { op: 1 } };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2741);
    let (file, start, length, message) = expected_type_pointer(&diagnostic);
    assert_eq!(
        message,
        "The expected type comes from property 'inner' which is declared here on type 'Outer'"
    );
    assert_eq!(span_text(source, start, length), "inner");
    assert_eq!(file, "test.ts");
    // tsc emits both pointers on this row; the new attach must not displace the
    // sibling declared-here that #16521 landed.
    assert!(
        has_declared_here(&diagnostic),
        "TS2728 must survive alongside TS6500: {diagnostic:?}"
    );
}

/// The owner reached through an `interface` rather than an alias renders its
/// written name, exactly as tsc does (`on type 'IOuter'`).
#[test]
fn nested_literal_under_an_interface_names_the_interface() {
    let source = "interface IOuter { inner: { op: number; oq: number } }\nconst r: IOuter = { inner: { op: 1 } };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2741);
    let (_, start, length, message) = expected_type_pointer(&diagnostic);
    assert_eq!(
        message,
        "The expected type comes from property 'inner' which is declared here on type 'IOuter'"
    );
    assert_eq!(span_text(source, start, length), "inner");
}

/// Twice nested. The pointer names the **immediately enclosing** property and
/// the anonymous type that declares it — `b` on `{ b: ...; }`, never the outer
/// alias `Deep`. Oracled: `nested.ts:5:20`.
#[test]
fn twice_nested_literal_names_the_immediately_enclosing_owner() {
    let source = "type Deep = { a: { b: { p: number; q: number } } };\nconst r: Deep = { a: { b: { p: 1 } } };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2741);
    let (_, start, length, message) = expected_type_pointer(&diagnostic);
    assert_eq!(
        message,
        "The expected type comes from property 'b' which is declared here on type '{ b: { p: number; q: number; }; }'"
    );
    assert_eq!(span_text(source, start, length), "b");
}

/// The row that makes this its own attach site: `TS2739` carries the pointer
/// and tsc emits **no** `TS2728` for a multi-property miss.
#[test]
fn nested_multi_property_miss_carries_the_pointer_without_a_declared_here() {
    let source = "type Multi = { inner: { m1: number; m2: number; m3: number } };\nconst r: Multi = { inner: { m1: 1 } };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2739);
    let (_, start, length, message) = expected_type_pointer(&diagnostic);
    assert_eq!(
        message,
        "The expected type comes from property 'inner' which is declared here on type 'Multi'"
    );
    assert_eq!(span_text(source, start, length), "inner");
    assert!(
        !has_declared_here(&diagnostic),
        "tsc emits no TS2728 on a multi-property miss: {diagnostic:?}"
    );
}

/// The `and N more` form is a distinct code and needs its own arm.
#[test]
fn nested_and_more_property_miss_carries_the_pointer() {
    let source = "type Huge = { inner: { a: number; b: number; c: number; d: number; e: number; f: number; g: number; h: number; i: number; j: number } };\nconst r: Huge = { inner: { a: 1 } };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2740);
    let (_, start, length, message) = expected_type_pointer(&diagnostic);
    assert_eq!(
        message,
        "The expected type comes from property 'inner' which is declared here on type 'Huge'"
    );
    assert_eq!(span_text(source, start, length), "inner");
}

/// An **optional** enclosing member still carries the pointer — the nullish
/// half of the member type is not what gates this, unlike the arrow-body drill
/// gate in the sibling `TS6502` family (#16550).
#[test]
fn optional_enclosing_member_still_carries_the_pointer() {
    let source =
        "type Opt = { inner?: { p: number; q: number } };\nconst r: Opt = { inner: { p: 1 } };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2741);
    let (_, start, length, message) = expected_type_pointer(&diagnostic);
    assert_eq!(
        message,
        "The expected type comes from property 'inner' which is declared here on type 'Opt'"
    );
    assert_eq!(span_text(source, start, length), "inner");
}

/// Binder names must not matter on the missing-property leaf either.
#[test]
fn renamed_binders_on_a_nested_missing_property() {
    let source = "type Zeta = { qux: { xylo: string; yak: number } };\nconst z: Zeta = { qux: { xylo: \"s\" } };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2741);
    let (_, start, length, message) = expected_type_pointer(&diagnostic);
    assert_eq!(
        message,
        "The expected type comes from property 'qux' which is declared here on type 'Zeta'"
    );
    assert_eq!(span_text(source, start, length), "qux");
}

/// NEGATIVE CONTROL — depth 0. The literal *is* the assignment's right-hand
/// side, so there is no enclosing property for the pointer to name and tsc
/// emits `TS2728` alone. This is the row that would break if the new attach
/// were reached from the top-level missing-property report rather than from
/// the per-property elaboration leaf.
#[test]
fn top_level_missing_property_takes_no_expected_type_pointer() {
    let source = "type Top = { alpha: string; beta: string };\nconst r: Top = { alpha: \"a\" };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2741);
    assert!(
        !has_pointer(&diagnostic),
        "a depth-0 miss has no enclosing property: {diagnostic:?}"
    );
    assert!(
        has_declared_here(&diagnostic),
        "TS2728 still fires at depth 0: {diagnostic:?}"
    );
}

/// NEGATIVE CONTROL — an **array element**. tsc anchors the `TS2728` inside the
/// element type but emits no `TS6500`: the frame immediately enclosing the
/// failing literal is an array element, which has no property name to report.
/// Oracled: `n2.ts:5:34` for the `TS2728`, no pointer line.
#[test]
fn array_element_literal_takes_no_expected_type_pointer() {
    let source = "type Arr = { list: { lp: number; lq: number }[] };\nconst r: Arr = { list: [{ lp: 1 }] };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2741);
    assert!(
        !has_pointer(&diagnostic),
        "an array element has no enclosing property: {diagnostic:?}"
    );
}

/// NEGATIVE CONTROL — a nested literal that type-checks cleanly must stay
/// clean, so none of the arms above can fire as a side effect of the walk.
#[test]
fn nested_missing_property_negative_control_stays_clean() {
    let source = "type Fine = { inner: { p: number; q: number } };\nconst f: Fine = { inner: { p: 1, q: 2 } };\n";
    assert!(
        check_source_diagnostics(source).is_empty(),
        "a complete nested value must not report anything"
    );
}
