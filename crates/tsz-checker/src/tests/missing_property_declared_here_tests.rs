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
//! Three cross-file shapes are deliberately *not* covered and produce no
//! pointer at all (unchanged output, never a wrong anchor): an imported
//! *class*, an imported type alias whose body is a type literal, and a target
//! reached through a re-exporting hub file. All three are blocked upstream of
//! this renderer on raw-`SymbolId` ambiguity between per-file binders, and are
//! written up with reduced repros in #16415.
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

// ---------------------------------------------------------------------------
// Cross-file targets.
//
// The pointer anchors in the file that *declares* the property, which is not
// the file the primary diagnostic lives in. Every row below is pinned against
// `typescript@7.0.2` (`--noEmit --strict --pretty --target es2022 --lib
// es2022`); the reported line/column is quoted on each case.
//
// Both harnesses are exercised deliberately.
// `check_multi_file_with_libs_stamped` is production-faithful: each binder is
// given its file index before binding, so `StableLocation::file_idx` is
// stamped. `check_multi_file` leaves it unassigned, which is the shape that
// makes a stable location resolve by `(pos, end)` against whichever arena the
// caller happens to be holding. The owning symbol's declaring file is the
// authoritative answer in both.
// ---------------------------------------------------------------------------

/// Diagnostics for `entry_file` with each binder stamped with its file index,
/// exactly as the driver does.
fn check_stamped(files: &[(&str, &str)], entry_file: &str) -> Vec<Diagnostic> {
    crate::test_utils::check_multi_file_with_libs_stamped(
        files,
        entry_file,
        crate::context::CheckerOptions::default(),
        &[],
    )
}

/// Diagnostics for `entry_file` with every `StableLocation::file_idx` left
/// unassigned.
fn check_unstamped(files: &[(&str, &str)], entry_file: &str) -> Vec<Diagnostic> {
    crate::test_utils::check_multi_file(
        files,
        entry_file,
        crate::context::CheckerOptions::default(),
    )
}

const CROSS_DEP: &str = "export interface Cross { one: number; two: number; }\n";
const CROSS_ENTRY: &str = "import { Cross } from \"./dep\";\nconst c: Cross = { one: 1 };\n";

/// `dep.ts:1:39 - 'two' is declared here.`, underlining `two`.
///
/// The owner symbol is read out of the binder that declares it. Per-file
/// binders hand out raw `SymbolId`s from `0`, so reading an imported target's
/// id out of the *checking* file's binder names an unrelated local symbol —
/// which is why this produced no pointer at all rather than a wrong one.
#[test]
fn cross_file_declaration_points_at_the_declaring_file() {
    let diagnostic = only(
        &check_stamped(
            &[("dep.ts", CROSS_DEP), ("test.ts", CROSS_ENTRY)],
            "test.ts",
        ),
        TS2741,
    );
    let (file, start, length, message) = declared_here(&diagnostic);
    assert_eq!(message, "'two' is declared here.");
    assert_eq!(file, "dep.ts");
    assert_eq!(span_text(CROSS_DEP, start, length), "two");
}

/// Same row with unstamped stable locations: the declaring file resolved from
/// the owning symbol carries the anchor when the location cannot.
#[test]
fn cross_file_declaration_resolves_without_a_stamped_file_index() {
    let diagnostic = only(
        &check_unstamped(
            &[("dep.ts", CROSS_DEP), ("test.ts", CROSS_ENTRY)],
            "test.ts",
        ),
        TS2741,
    );
    let (file, start, length, message) = declared_here(&diagnostic);
    assert_eq!(message, "'two' is declared here.");
    assert_eq!(file, "dep.ts");
    assert_eq!(span_text(CROSS_DEP, start, length), "two");
}

/// The pointer must still be tagged as a cross-location pointer across files,
/// so `--pretty false` suppresses it exactly as tsc does: the oracle's plain
/// run on this pair prints the `TS2741` line alone.
#[test]
fn cross_file_pointer_is_tagged_as_a_cross_location_pointer() {
    let diagnostic = only(
        &check_stamped(
            &[("dep.ts", CROSS_DEP), ("test.ts", CROSS_ENTRY)],
            "test.ts",
        ),
        TS2741,
    );
    let pointer = diagnostic
        .related_information
        .iter()
        .find(|info| info.code == TS2728)
        .expect("TS2728 pointer");
    assert!(
        pointer.is_location_pointer(),
        "cross-file TS2728 must be a cross-location pointer: {pointer:?}"
    );
}

/// Binder names must not matter across files either: the same shape under
/// different identifiers, imported under a local alias, resolves through the
/// target's own member table.
///
/// `dep.ts:1:41 - 'gamma' is declared here.`
#[test]
fn renamed_cross_file_binders_point_at_the_renamed_declaration() {
    let dep = "export interface Payload { beta: number; gamma: number; }\n";
    let entry = "import { Payload as Local } from \"./dep\";\nconst value: Local = { beta: 1 };\n";
    let diagnostic = only(
        &check_stamped(&[("dep.ts", dep), ("test.ts", entry)], "test.ts"),
        TS2741,
    );
    let (file, start, length, message) = declared_here(&diagnostic);
    assert_eq!(message, "'gamma' is declared here.");
    assert_eq!(file, "dep.ts");
    assert_eq!(span_text(dep, start, length), "gamma");
}

/// A local declaration in the checking file carrying a member with the *same*
/// name must not capture the foreign pointer, and its own diagnostic must
/// still anchor locally. tsc reports both, each in its own file
/// (`dep.ts:1:39` and `test.ts:2:34`) — so this discriminates "resolved the
/// declaring arena" from "found something plausible in the arena I had".
#[test]
fn a_same_named_local_member_does_not_capture_the_cross_file_pointer() {
    let entry = "import { Cross } from \"./dep\";\ninterface Decoy { alpha: number; two: number; }\nconst c: Cross = { one: 1 };\nconst d: Decoy = { alpha: 1 };\n";
    let diagnostics = check_stamped(&[("dep.ts", CROSS_DEP), ("test.ts", entry)], "test.ts");
    let pointers: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == TS2741)
        .map(declared_here)
        .collect();
    assert_eq!(
        pointers.len(),
        2,
        "expected both TS2741 rows: {diagnostics:?}"
    );

    let foreign = pointers
        .iter()
        .find(|(file, ..)| file == "dep.ts")
        .expect("the imported target points into dep.ts");
    assert_eq!(foreign.3, "'two' is declared here.");
    assert_eq!(span_text(CROSS_DEP, foreign.1, foreign.2), "two");

    let local = pointers
        .iter()
        .find(|(file, ..)| file == "test.ts")
        .expect("the local target still points into test.ts");
    assert_eq!(local.3, "'two' is declared here.");
    assert_eq!(span_text(entry, local.1, local.2), "two");
}

/// The negative arm survives the cross-file path: two unmatched properties is
/// `TS2739`, and tsc attaches no pointer to it
/// (`main5.ts(2,7): error TS2739: ... from type 'Cross': one, two`).
#[test]
fn multiple_missing_cross_file_properties_carry_no_pointer() {
    let entry = "import { Cross } from \"./dep\";\nconst c: Cross = {};\n";
    let diagnostics = check_stamped(&[("dep.ts", CROSS_DEP), ("test.ts", entry)], "test.ts");
    let diagnostic = only(&diagnostics, TS2739);
    assert!(
        diagnostic
            .related_information
            .iter()
            .all(|info| info.code != TS2728),
        "TS2739 carries no declared-here pointer: {diagnostic:?}"
    );
    assert!(
        !diagnostics.iter().any(|d| d.code == TS2741),
        "the two-missing form is TS2739, not TS2741: {diagnostics:?}"
    );
}

// ---------------------------------------------------------------------------
// Anonymous (type-literal) targets — #16443.
//
// These targets resolve to no binder symbol, and cannot be made to: the
// interner's `ObjectShape.symbol` participates in `Hash`/`PartialEq`, so giving
// a type literal its own symbol would de-intern every structurally identical
// anonymous object in the program, and hanging the declaration on the interned
// shape instead would share one location across every occurrence of that shape.
// The anchor therefore comes from the annotation *written at the failure site*,
// which is per-occurrence by construction.
//
// Every expectation below is pinned against `typescript@7.0.2` with
// `--noEmit --strict --pretty --target es2022 --lib es2022`.
// ---------------------------------------------------------------------------

/// The alias body is a type literal, so the alias declaration carries the
/// member list and the pointer anchors on the signature as written.
#[test]
fn alias_to_type_literal_points_into_the_alias_body() {
    let source = "type Lit = { alpha: string; beta: string };\nconst a: Lit = { alpha: \"a\" };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2741);
    let (file, start, length, message) = declared_here(&diagnostic);
    assert_eq!(message, "'beta' is declared here.");
    assert_eq!(span_text(source, start, length), "beta");
    assert_eq!(file, "test.ts");
}

/// Binder names must not matter here either — the same shape under different
/// identifiers anchors on whatever the annotation actually declares.
#[test]
fn alias_to_type_literal_renamed_binders_point_at_the_renamed_declaration() {
    let source = "type Zed = { qux: string; wob: string };\nconst r: Zed = { qux: \"a\" };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2741);
    let (_, start, length, message) = declared_here(&diagnostic);
    assert_eq!(message, "'wob' is declared here.");
    assert_eq!(span_text(source, start, length), "wob");
}

/// An inline annotation has no declaration of its own anywhere — the written
/// type literal *is* the declaration, and it is the one this binding was
/// annotated with rather than any other literal of the same shape.
#[test]
fn inline_type_literal_annotation_points_into_itself() {
    let source = "const b: { one: number; two: number } = { one: 1 };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2741);
    let (_, start, length, message) = declared_here(&diagnostic);
    assert_eq!(message, "'two' is declared here.");
    assert_eq!(span_text(source, start, length), "two");
}

/// Two structurally identical inline annotations intern to one `TypeId`. Each
/// must still point at *its own* text — this is the case that any
/// location-on-the-interned-shape design gets wrong, and it is why the anchor
/// is taken from the annotation node.
#[test]
fn identical_inline_annotations_each_point_at_their_own_text() {
    let source = "const first: { one: number; two: number } = { one: 1 };\n\
                  const second: { one: number; two: number } = { one: 2 };\n";
    let diagnostics = check_source_diagnostics(source);
    let anchors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == TS2741)
        .map(declared_here)
        .collect();
    assert_eq!(anchors.len(), 2, "both rows report: {diagnostics:?}");
    let starts: Vec<u32> = anchors.iter().map(|(_, start, ..)| *start).collect();
    assert_ne!(
        starts[0], starts[1],
        "each annotation anchors on its own `two`, not on a shared interned one: {anchors:?}"
    );
    for (_, start, length, message) in &anchors {
        assert_eq!(message, "'two' is declared here.");
        assert_eq!(span_text(source, *start, *length), "two");
    }
    assert!(
        (starts[0] as usize) < source.find('\n').expect("two lines"),
        "the first row anchors on the first line: {anchors:?}"
    );
}

/// A parenthesized annotation is the same type literal with wrappers; the walk
/// peels them rather than declining.
#[test]
fn parenthesized_type_literal_annotation_points_into_the_literal() {
    let source = "const p: ({ mm: number; nn: number }) = { mm: 1 };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2741);
    let (_, start, length, message) = declared_here(&diagnostic);
    assert_eq!(message, "'nn' is declared here.");
    assert_eq!(span_text(source, start, length), "nn");
}

/// An alias chain declares no member list of its own, so the walk continues
/// through the alias body until it reaches the literal that does.
#[test]
fn alias_chain_to_type_literal_points_at_the_final_body() {
    let source = "type Zed = { qux: string; wob: string };\n\
                  type Indirect = Zed;\n\
                  const r: Indirect = { qux: \"a\" };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2741);
    let (_, start, length, message) = declared_here(&diagnostic);
    assert_eq!(message, "'wob' is declared here.");
    assert_eq!(span_text(source, start, length), "wob");
    assert!(
        (start as usize)
            < source
                .find("type Indirect")
                .expect("the chain's middle link"),
        "the anchor is in `Zed`'s body, not the alias that forwards to it"
    );
}

/// A generic alias applied at the annotation still anchors on the type
/// parameter's own signature as written in the alias body.
#[test]
fn generic_alias_application_points_into_the_uninstantiated_body() {
    let source = "type Gen<TParam> = { gee: TParam; aitch: TParam };\n\
                  const g: Gen<string> = { gee: \"a\" };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2741);
    let (_, start, length, message) = declared_here(&diagnostic);
    assert_eq!(message, "'aitch' is declared here.");
    assert_eq!(span_text(source, start, length), "aitch");
}

/// An indexed access resolves its written key against the object type's
/// members and continues into that member's own type node.
#[test]
fn indexed_access_annotation_points_into_the_member_type_literal() {
    let source = "interface Nest { inner: { p: number; q: number }; }\n\
                  const c: Nest[\"inner\"] = { p: 1 };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2741);
    let (_, start, length, message) = declared_here(&diagnostic);
    assert_eq!(message, "'q' is declared here.");
    assert_eq!(span_text(source, start, length), "q");
}

/// A `return` is checked against the enclosing function's declared return
/// type, so an anonymous return annotation anchors there.
#[test]
fn anonymous_return_type_annotation_points_into_itself() {
    let source = "function ret(): { rp: number; rq: number } { return { rp: 1 }; }\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2741);
    let (_, start, length, message) = declared_here(&diagnostic);
    assert_eq!(message, "'rq' is declared here.");
    assert_eq!(span_text(source, start, length), "rq");
}

/// An assignment to an already-annotated binding anchors on that binding's
/// annotation, not on the assignment.
#[test]
fn assignment_to_annotated_binding_points_into_the_declaration_annotation() {
    let source = "let target: { ap: number; aq: number };\ntarget = { ap: 1 };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2741);
    let (_, start, length, message) = declared_here(&diagnostic);
    assert_eq!(message, "'aq' is declared here.");
    assert_eq!(span_text(source, start, length), "aq");
}

/// Negative arm: two unmatched properties is `TS2739`, which tsc leaves with
/// no pointer — the anonymous route must not add one.
#[test]
fn anonymous_target_missing_two_properties_carries_no_pointer() {
    let source =
        "type Multi = { m1: string; m2: string; m3: string };\nconst m: Multi = { m1: \"a\" };\n";
    let diagnostics = check_source_diagnostics(source);
    let diagnostic = only(&diagnostics, TS2739);
    assert!(
        diagnostic
            .related_information
            .iter()
            .all(|info| info.code != TS2728),
        "TS2739 carries no declared-here pointer: {diagnostic:?}"
    );
}

/// Negative arm: an argument's parameter annotation is not reachable from the
/// call site's annotation walk, which stops at the statement boundary. tsc
/// *does* point here (`fam.ts:1:38`), so this pins today's decline as a known
/// remaining slice of #16443 rather than a wrong anchor — the assertion is
/// that no pointer appears, never that a wrong one does.
#[test]
fn call_argument_against_anonymous_parameter_declines_rather_than_guessing() {
    let source = "declare function f(arg: { u: number; v: number }): void;\nf({ u: 1 });\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2741);
    assert!(
        diagnostic
            .related_information
            .iter()
            .all(|info| info.code != TS2728),
        "no pointer is produced for this shape today, and never a wrong one: {diagnostic:?}"
    );
}

/// The annotation route is a *fallback*: a target that does resolve to a
/// binder symbol must still anchor through the symbol route, so adding the
/// fallback cannot shadow or relocate an existing pointer.
#[test]
fn named_target_still_anchors_through_the_symbol_route() {
    let source = "type Src = { one: number };\n\
                  interface Want { one: number; two: number }\n\
                  declare const s: Src;\n\
                  const w: Want = s;\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2741);
    let (_, start, length, message) = declared_here(&diagnostic);
    assert_eq!(message, "'two' is declared here.");
    assert_eq!(span_text(source, start, length), "two");
    assert!(
        (start as usize) > source.find("interface Want").expect("the interface"),
        "the anchor is `Want`'s own member, not anything in `Src`"
    );
}

/// A cross-file imported alias whose body is a type literal — one of the three
/// shapes the module header above recorded as producing no pointer. The
/// annotation route resolves the alias through its *declaring* file's arena, so
/// this now anchors in `dep.ts`, oracle-exact.
#[test]
fn imported_alias_to_type_literal_points_into_the_declaring_file() {
    let dep = "export type Remote = { rone: string; rtwo: string };\n";
    let entry = "import { Remote } from \"./dep\";\nconst m: Remote = { rone: \"a\" };\n";
    let diagnostics = check_stamped(&[("dep.ts", dep), ("test.ts", entry)], "test.ts");
    let diagnostic = only(&diagnostics, TS2741);
    let (file, start, length, message) = declared_here(&diagnostic);
    assert_eq!(message, "'rtwo' is declared here.");
    assert_eq!(file, "dep.ts");
    assert_eq!(span_text(dep, start, length), "rtwo");
}
