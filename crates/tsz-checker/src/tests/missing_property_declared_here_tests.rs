//! Regression tests for the `TS2728` `'x' is declared here.` pointer that tsc
//! pairs with the single-missing-property form of `TS2741`.
//!
//! Structural rule (pinned against `typescript@7.0.2`, the conformance pin):
//! when exactly one required property of the target is unmatched, tsc's
//! `reportUnmatchedProperty` associates a related-information entry at the
//! *name node* of that property's own declaration, in the file that declares
//! it. When more than one property is unmatched the diagnostic is TS2739 /
//! TS2740 and tsc attaches no pointer at all.
//!
//! Two shapes are deliberately *not* covered yet and produce no pointer at all
//! (unchanged output, never a wrong anchor): a class-typed target, whose
//! members the owner declaration walk below does not reach, and a target whose
//! declaration lives in another file, whose `StableLocation` does not resolve
//! to the declaring arena from here. Both are written up as the next slice.
//!
//! tsz builds the pointer in the assignability renderer
//! (`error_reporter/missing_property_declared_here.rs`), reached from the one
//! TS2741 construction in `render_missing_property`.

use crate::diagnostics::Diagnostic;
use crate::test_utils::check_source_diagnostics;
use tsz_common::diagnostics::diagnostic_codes;

const TS2741: u32 = diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE;
const TS2739: u32 = diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE;
const TS2728: u32 = diagnostic_codes::IS_DECLARED_HERE;

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
/// assert the anchor lands on the declared name and nothing wider.
fn declared_here(diagnostic: &Diagnostic) -> (String, u32, u32, String) {
    let pointers: Vec<_> = diagnostic
        .related_information
        .iter()
        .filter(|info| info.code == TS2728)
        .collect();
    assert_eq!(
        pointers.len(),
        1,
        "expected exactly one TS2728 pointer; got {:?}",
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

fn span_text(source: &str, start: u32, length: u32) -> &str {
    &source[start as usize..(start + length) as usize]
}

/// The pointer models tsc's `relatedInformation`, not a `messageText` chain
/// link, so it must carry that tag through to the reporter. Oracled on
/// `typescript@7.0.2`: `tsc --noEmit --strict --pretty false` on this source
/// prints the TS2741 line alone, with no `'y' is declared here.` beneath it,
/// while the `--pretty` run does print it with its own location and snippet.
/// Only the tag distinguishes the two — a chain link carries a real file and a
/// real span exactly as this entry does.
#[test]
fn declared_here_pointer_is_tagged_as_a_cross_location_pointer() {
    let source = "interface Point { x: number; y: number; }\nconst p: Point = { x: 1 };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2741);
    let pointer = diagnostic
        .related_information
        .iter()
        .find(|info| info.code == TS2728)
        .expect("TS2728 pointer");
    assert!(
        pointer.is_location_pointer(),
        "TS2728 must be a cross-location pointer: {pointer:?}"
    );
    assert!(
        diagnostic
            .related_information
            .iter()
            .filter(|info| info.code != TS2728)
            .all(|info| !info.is_location_pointer()),
        "elaboration links on the same diagnostic stay chain links: {diagnostic:?}"
    );
}

#[test]
fn single_missing_interface_property_points_at_its_declaration() {
    let source = "interface Point { x: number; y: number; }\nconst p: Point = { x: 1 };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2741);
    let (file, start, length, message) = declared_here(&diagnostic);
    assert_eq!(message, "'y' is declared here.");
    assert_eq!(span_text(source, start, length), "y");
    assert_eq!(file, "test.ts");
}

/// Binder names must not matter: the same shape under different identifiers
/// resolves through the target's own member table, not a name match.
#[test]
fn renamed_binders_point_at_the_renamed_declaration() {
    let source =
        "interface Coord { alpha: number; omega: number; }\nconst c: Coord = { alpha: 1 };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2741);
    let (_, start, length, message) = declared_here(&diagnostic);
    assert_eq!(message, "'omega' is declared here.");
    assert_eq!(span_text(source, start, length), "omega");
}

/// A quoted property name anchors on the name node as written, quotes included
/// — the anchor is the declaration's name node, not a re-rendered identifier.
#[test]
fn quoted_property_name_anchors_on_the_written_name_node() {
    let source = "interface Q { one: number; \"two-part\": number; }\nconst q: Q = { one: 1 };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2741);
    let (_, start, length, message) = declared_here(&diagnostic);
    // tsc renders a string-literal property name with its quotes in BOTH the
    // primary and the pointer (`'"two-part"' is declared here.`); tsz renders
    // it bare in the primary today, and the pointer reuses that same display so
    // the two can never disagree with each other. The quoted-name display gap
    // is a separate, pre-existing divergence.
    assert_eq!(message, "'two-part' is declared here.");
    assert!(
        diagnostic
            .message_text
            .contains("Property 'two-part' is missing"),
        "pointer must reuse the primary's property display: {}",
        diagnostic.message_text
    );
    assert_eq!(span_text(source, start, length), "\"two-part\"");
}

#[test]
fn missing_method_member_points_at_the_method_name() {
    let source =
        "interface HasRun { keep: number; run(): void; }\nconst h: HasRun = { keep: 1 };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2741);
    let (_, start, length, message) = declared_here(&diagnostic);
    assert_eq!(message, "'run' is declared here.");
    assert_eq!(span_text(source, start, length), "run(): void;");
}

/// Negative half of the rule: two or more unmatched properties is TS2739, and
/// tsc attaches no pointer there.
#[test]
fn multiple_missing_properties_carry_no_pointer() {
    let source = "interface P3 { x: number; y: number; z: number; }\nconst q: P3 = { x: 1 };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2739);
    assert!(
        diagnostic
            .related_information
            .iter()
            .all(|info| info.code != TS2728),
        "TS2739 must not carry a declared-here pointer: {:?}",
        diagnostic
            .related_information
            .iter()
            .map(|info| (info.code, info.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

/// Optional members are not required, so no diagnostic and no pointer.
#[test]
fn optional_missing_property_produces_no_diagnostic() {
    let source = "interface Opt { x: number; y?: number; }\nconst o: Opt = { x: 1 };\n";
    let diags = check_source_diagnostics(source);
    assert!(
        diags.iter().all(|d| d.code != TS2741),
        "optional property must not report TS2741: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

/// An alias in front of the interface does not change the owner the pointer
/// resolves through.
#[test]
fn aliased_target_still_points_at_the_underlying_declaration() {
    let source = "interface Base { keep: number; gone: number; }\ntype Alias = Base;\nconst a: Alias = { keep: 1 };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2741);
    let (_, start, length, message) = declared_here(&diagnostic);
    assert_eq!(message, "'gone' is declared here.");
    assert_eq!(span_text(source, start, length), "gone");
}

/// A generic interface instantiated at a concrete argument points at the
/// declaration in the generic's own body.
#[test]
fn generic_target_points_at_the_declaration_in_the_generic_body() {
    let source =
        "interface Box<T> { held: T; label: string; }\nconst b: Box<number> = { held: 1 };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2741);
    let (_, start, length, message) = declared_here(&diagnostic);
    assert_eq!(message, "'label' is declared here.");
    assert_eq!(span_text(source, start, length), "label");
}

/// A class-typed target points at its own field declaration. The owner walk
/// already resolved the class's own symbol correctly; the gap was
/// `member_name_node` reading a class member's name through `get_signature`
/// (the accessor for an interface/type-literal `PROPERTY_SIGNATURE`), which
/// returns `None` for a class's `PROPERTY_DECLARATION` — its name lives on
/// the distinct `PropertyDeclData` instead.
#[test]
fn single_missing_class_property_points_at_its_declaration() {
    let source = "class C { a: number = 0; b: number = 0; }\nconst c: C = { a: 1 };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2741);
    let (_, start, length, message) = declared_here(&diagnostic);
    assert_eq!(message, "'b' is declared here.");
    assert_eq!(span_text(source, start, length), "b");
}

/// Unlike an interface/type-literal method signature (underlined whole,
/// `run(): void;`), tsc underlines only a *class* method's name — pinned
/// against `typescript@7.0.2`: `class HasRun { keep = 0; run(): void {} }`
/// underlines exactly `run` (3 chars), not the method body.
#[test]
fn missing_class_method_member_points_at_the_method_name_only() {
    let source =
        "class HasRun { keep: number = 0; run(): void {} }\nconst h: HasRun = { keep: 1 };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2741);
    let (_, start, length, message) = declared_here(&diagnostic);
    assert_eq!(message, "'run' is declared here.");
    assert_eq!(span_text(source, start, length), "run");
}
