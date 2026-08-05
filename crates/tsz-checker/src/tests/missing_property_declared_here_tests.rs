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
// Re-export hubs (#16415 row 3).
//
// A target reached through `export { X } from "./dep"` resolves to the *hub's*
// export specifier, which owns no member list, so the walk declined and no
// pointer was produced. tsc's `resolveAlias` follows the alias to the original
// declaration and anchors there, never on the re-export clause.
//
// Every row is pinned against `typescript@7.0.2` (`--noEmit --strict --pretty
// --target es2022 --lib es2022`) and the oracle's reported location is quoted
// on each case. In all four the pointer lands in `dep.ts` and `hub.ts` is
// never mentioned.
// ---------------------------------------------------------------------------

const HUB_DEP: &str = "export interface Cross { one: number; two: number; }\nexport class Shape { held: number = 0; away: number = 0; }\n";

/// `dep.ts:1:39 - 'two' is declared here.`, underlining `two`.
#[test]
fn reexport_hub_points_at_the_original_declaration() {
    let diagnostic = only(
        &check_stamped(
            &[
                ("dep.ts", HUB_DEP),
                ("hub.ts", "export { Cross } from \"./dep\";\n"),
                (
                    "test.ts",
                    "import { Cross } from \"./hub\";\nconst c: Cross = { one: 1 };\n",
                ),
            ],
            "test.ts",
        ),
        TS2741,
    );
    let (file, start, length, message) = declared_here(&diagnostic);
    assert_eq!(message, "'two' is declared here.");
    assert_eq!(file, "dep.ts");
    assert_eq!(span_text(HUB_DEP, start, length), "two");
}

/// A renamed re-export names the *original* export in the target module, and
/// the pointer still lands on that original name:
/// `dep.ts:1:39 - 'two' is declared here.` for `export { Cross as Renamed }`.
#[test]
fn renamed_reexport_hub_points_at_the_original_declaration() {
    let diagnostic = only(
        &check_stamped(
            &[
                ("dep.ts", HUB_DEP),
                ("hub.ts", "export { Cross as Renamed } from \"./dep\";\n"),
                (
                    "test.ts",
                    "import { Renamed } from \"./hub\";\nconst r: Renamed = { one: 1 };\n",
                ),
            ],
            "test.ts",
        ),
        TS2741,
    );
    let (file, start, length, message) = declared_here(&diagnostic);
    assert_eq!(message, "'two' is declared here.");
    assert_eq!(file, "dep.ts");
    assert_eq!(span_text(HUB_DEP, start, length), "two");
}

/// A wildcard hub carries no specifier for the name at all, so the edge is the
/// re-export index rather than a written clause. Oracle:
/// `dep.ts:2:40 - 'away' is declared here.`, underlining `away`. A *class*
/// target through the hub also pins that the member-list walk reaches class
/// members, not only interface ones.
#[test]
fn wildcard_reexport_hub_points_at_the_original_declaration() {
    let diagnostic = only(
        &check_stamped(
            &[
                ("dep.ts", HUB_DEP),
                ("hub.ts", "export * from \"./dep\";\n"),
                (
                    "test.ts",
                    "import { Shape } from \"./hub\";\nconst s: Shape = { held: 1 };\n",
                ),
            ],
            "test.ts",
        ),
        TS2741,
    );
    let (file, start, length, message) = declared_here(&diagnostic);
    assert_eq!(message, "'away' is declared here.");
    assert_eq!(file, "dep.ts");
    assert_eq!(span_text(HUB_DEP, start, length), "away");
}

/// Two hubs in a row: `test.ts -> hub2.ts -> hub.ts -> dep.ts`. Oracle points
/// at `dep.ts:1:39` — neither hub is ever mentioned.
#[test]
fn two_hop_reexport_chain_points_at_the_original_declaration() {
    let diagnostic = only(
        &check_stamped(
            &[
                ("dep.ts", HUB_DEP),
                ("hub.ts", "export { Cross } from \"./dep\";\n"),
                ("hub2.ts", "export { Cross } from \"./hub\";\n"),
                (
                    "test.ts",
                    "import { Cross } from \"./hub2\";\nconst c: Cross = { one: 1 };\n",
                ),
            ],
            "test.ts",
        ),
        TS2741,
    );
    let (file, start, length, message) = declared_here(&diagnostic);
    assert_eq!(message, "'two' is declared here.");
    assert_eq!(file, "dep.ts");
    assert_eq!(span_text(HUB_DEP, start, length), "two");
}

/// The same shape with every binder renamed: the hop is driven by the module
/// graph and the exported name, so nothing about it may depend on the
/// particular identifiers a user chose.
#[test]
fn renamed_hub_binders_point_at_the_renamed_original_declaration() {
    let dep = "export interface Widget { alpha: number; omega: number; }\n";
    let diagnostic = only(
        &check_stamped(
            &[
                ("catalog.ts", dep),
                ("barrel.ts", "export { Widget } from \"./catalog\";\n"),
                (
                    "test.ts",
                    "import { Widget } from \"./barrel\";\nconst w: Widget = { alpha: 1 };\n",
                ),
            ],
            "test.ts",
        ),
        TS2741,
    );
    let (file, start, length, message) = declared_here(&diagnostic);
    assert_eq!(message, "'omega' is declared here.");
    assert_eq!(file, "catalog.ts");
    assert_eq!(span_text(dep, start, length), "omega");
}

/// The negative arm holds through a hub: two unmatched properties is `TS2739`
/// and tsc attaches no pointer to it.
#[test]
fn multiple_missing_properties_through_a_hub_carry_no_pointer() {
    let diagnostics = check_stamped(
        &[
            ("dep.ts", HUB_DEP),
            ("hub.ts", "export { Cross } from \"./dep\";\n"),
            (
                "test.ts",
                "import { Cross } from \"./hub\";\nconst c: Cross = {};\n",
            ),
        ],
        "test.ts",
    );
    let diagnostic = only(&diagnostics, TS2739);
    assert!(
        diagnostic
            .related_information
            .iter()
            .all(|info| info.code != TS2728),
        "TS2739 carries no declared-here pointer: {diagnostic:?}"
    );
}

/// The pointer through a hub is still a cross-location pointer, so
/// `--pretty false` suppresses it exactly as tsc does.
#[test]
fn reexport_hub_pointer_is_tagged_as_a_cross_location_pointer() {
    let diagnostic = only(
        &check_stamped(
            &[
                ("dep.ts", HUB_DEP),
                ("hub.ts", "export { Cross } from \"./dep\";\n"),
                (
                    "test.ts",
                    "import { Cross } from \"./hub\";\nconst c: Cross = { one: 1 };\n",
                ),
            ],
            "test.ts",
        ),
        TS2741,
    );
    let pointer = diagnostic
        .related_information
        .iter()
        .find(|info| info.code == TS2728)
        .expect("a TS2728 pointer");
    assert!(
        pointer.is_location_pointer(),
        "the hub pointer is a cross-location pointer, not a message-chain link: {pointer:?}"
    );
}

/// A hub does not make the *primary* diagnostic move: the reported target is
/// still the name the entry file wrote, and the message is untouched.
#[test]
fn reexport_hub_leaves_the_primary_diagnostic_unchanged() {
    let diagnostic = only(
        &check_stamped(
            &[
                ("dep.ts", HUB_DEP),
                ("hub.ts", "export { Cross } from \"./dep\";\n"),
                (
                    "test.ts",
                    "import { Cross } from \"./hub\";\nconst c: Cross = { one: 1 };\n",
                ),
            ],
            "test.ts",
        ),
        TS2741,
    );
    assert_eq!(
        diagnostic.message_text,
        "Property 'two' is missing in type '{ one: number; }' but required in type 'Cross'."
    );
}

/// #16415 row 2 is out of scope and must stay that way: an imported type alias
/// whose body is a type literal resolves to no symbol at all, so there is no
/// alias edge to follow and the pointer is still declined rather than guessed.
#[test]
fn imported_type_literal_alias_still_declines_the_pointer() {
    let diagnostic = only(
        &check_stamped(
            &[
                (
                    "dep.ts",
                    "export type Lit = { alpha: string; beta: string };\n",
                ),
                (
                    "test.ts",
                    "import { Lit } from \"./dep\";\nconst l: Lit = { alpha: \"a\" };\n",
                ),
            ],
            "test.ts",
        ),
        TS2741,
    );
    assert!(
        diagnostic
            .related_information
            .iter()
            .all(|info| info.code != TS2728),
        "row 2 is unchanged by the alias hop: {diagnostic:?}"
    );
}
