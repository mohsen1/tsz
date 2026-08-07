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

use crate::CheckerOptions;
use crate::diagnostics::Diagnostic;
use crate::test_utils::{check_source_diagnostics, check_source_with_libs, load_default_lib_files};
use std::sync::{Arc, OnceLock};
use tsz_binder::lib_loader::LibFile;
use tsz_common::diagnostics::diagnostic_codes;

/// The array-like rows below name `Array` / `ReadonlyArray`, so they need the
/// real lib on the side; `check_source_diagnostics` runs lib-free and would
/// report TS2318 `Cannot find global type 'Array'` instead of the TS2741 under
/// test. Loaded once for the whole suite.
fn default_libs() -> &'static [Arc<LibFile>] {
    static DEFAULT_LIBS: OnceLock<Vec<Arc<LibFile>>> = OnceLock::new();
    DEFAULT_LIBS.get_or_init(load_default_lib_files)
}

fn check_source_diagnostics_with_libs(source: &str) -> Vec<Diagnostic> {
    check_source_with_libs(source, "test.ts", CheckerOptions::default(), default_libs())
}

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

/// Each `TS2741`'s declared-here `(anchor start, anchor text)`, in source order
/// of the diagnostics themselves. Shared by the per-element tuple rows, which
/// all assert that every failing element carries its own pointer.
fn ts2741_anchors_in_order(source: &str) -> Vec<(u32, String)> {
    let diagnostics = check_source_diagnostics(source);
    let mut ts2741: Vec<_> = diagnostics.iter().filter(|d| d.code == TS2741).collect();
    ts2741.sort_by_key(|d| d.start);
    ts2741
        .iter()
        .map(|d| {
            let (_, start, length, _) = declared_here(d);
            (start, span_text(source, start, length).to_string())
        })
        .collect()
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

/// #16415 row 2: an imported type alias whose body is a type literal. #16430
/// pinned this as *declining* — correct for the code as it stood, since the
/// target resolves to no symbol and there is therefore no alias edge to follow.
///
/// The annotation route reaches it a different way: the type reference written
/// in the annotation resolves to the alias symbol, which the shared alias walk
/// then follows into its declaring file. tsc points here
/// (`dep.ts(1,36)`: `'beta' is declared here.`, oracled on `typescript@7.0.2`
/// with `--module commonjs`), so the expectation moves from tsz's old decline
/// to tsc's own anchor.
#[test]
fn imported_type_literal_alias_points_into_the_declaring_file() {
    const ALIAS_DEP: &str = "export type Lit = { alpha: string; beta: string };\n";
    let diagnostic = only(
        &check_stamped(
            &[
                ("dep.ts", ALIAS_DEP),
                (
                    "test.ts",
                    "import { Lit } from \"./dep\";\nconst l: Lit = { alpha: \"a\" };\n",
                ),
            ],
            "test.ts",
        ),
        TS2741,
    );
    let (file, start, length, message) = declared_here(&diagnostic);
    assert_eq!(message, "'beta' is declared here.");
    assert_eq!(file, "dep.ts");
    assert_eq!(span_text(ALIAS_DEP, start, length), "beta");
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

/// Row 3 of #16443: a call argument's missing property points at the matched
/// parameter's own anonymous type-literal annotation, one call/new-expression
/// hop past the statement boundary `target_annotation_node` otherwise stops
/// at. The callee must resolve to a single (non-overloaded) same-file
/// `function` declaration and the matched parameter must not be a rest
/// parameter; see `call_argument_annotation_node`'s doc comment for why.
#[test]
fn call_argument_against_anonymous_parameter_points_at_the_parameter_annotation() {
    let source = "declare function f(arg: { u: number; v: number }): void;\nf({ u: 1 });\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2741);
    let (_, start, length, message) = declared_here(&diagnostic);
    assert_eq!(message, "'v' is declared here.");
    assert_eq!(span_text(source, start, length), "v");
}

/// Negative control: an overloaded callee reports `TS2769` ("No overload
/// matches this call"), not a bare `TS2741`, so the single-declaration guard
/// in `call_argument_annotation_node` is never even reached here — this pins
/// that no `TS2728` pointer leaks in from it regardless.
#[test]
fn call_argument_against_overloaded_callee_reports_no_overload_matches() {
    let source = "declare function f(arg: { u: number; v: number }): void;\n\
                  declare function f(arg: string): void;\n\
                  f({ u: 1 });\n";
    let diags = check_source_diagnostics(source);
    assert!(
        diags.iter().any(|d| d.code == 2769),
        "expected TS2769 (no overload matches): {diags:?}"
    );
    assert!(
        diags
            .iter()
            .all(|d| d.related_information.iter().all(|info| info.code != TS2728)),
        "an overloaded callee must not leak a TS2728 pointer: {diags:?}"
    );
}

/// Negative control: a rest parameter is not a sound positional match — the
/// argument index does not name one declared parameter.
#[test]
fn call_argument_against_rest_parameter_declines_rather_than_guessing() {
    let source = "declare function f(...args: { u: number; v: number }[]): void;\nf({ u: 1 });\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2741);
    assert!(
        diagnostic
            .related_information
            .iter()
            .all(|info| info.code != TS2728),
        "a rest parameter is not a positional match: {diagnostic:?}"
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

// ---------------------------------------------------------------------------
// Nested elaboration: a property missing from an *inner* object literal.
//
// tsc's `reportUnmatchedProperty` draws no distinction between an inner and an
// outer literal — every row below carries the pointer in `typescript@7.0.2`.
// The leaf property name alone cannot locate it: `oq` is no member of `Outer`
// in `const r: Outer = { inner: { op: 1 } }`, so the annotation walk first
// follows the path the object-literal syntax at the failure site spells out.
// ---------------------------------------------------------------------------

/// Row 1 of #16443's nested-elaboration table. Oracle (`typescript@7.0.2`,
/// `--noEmit --strict --pretty --target es2022 --lib es2022`):
/// `matrix.ts:1:37 - 'oq' is declared here.`
#[test]
fn nested_object_literal_pointer_anchors_in_the_inner_type_literal() {
    let source = "type Outer = { inner: { op: number; oq: number } };\nconst r: Outer = { inner: { op: 1 } };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2741);
    let (_, start, length, message) = declared_here(&diagnostic);
    assert_eq!(
        span_text(source, start, length),
        "oq",
        "the anchor underlines the inner member's own name node"
    );
    assert!(
        message.contains("'oq'"),
        "pointer names the property: {message}"
    );
}

/// Anti-hardcoding: the walk keys on the nesting structure, not on the
/// identifiers `Outer`/`inner`/`oq`. Renamed binders anchor identically.
#[test]
fn nested_pointer_is_binder_name_independent() {
    let source = "type Envelope = { payload: { alpha: number; beta: number } };\nconst env: Envelope = { payload: { alpha: 1 } };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2741);
    let (_, start, length, _) = declared_here(&diagnostic);
    assert_eq!(span_text(source, start, length), "beta");
}

/// The same nesting written as an `interface` rather than a type alias. The
/// annotation walk reaches an interface's member list through the same
/// declaration route, so the interface and alias forms must not diverge.
/// Oracle: `matrix.ts:3:41 - 'oq' is declared here.`
#[test]
fn nested_pointer_anchors_through_an_interface_annotation() {
    let source = "interface OuterI { inner: { op: number; oq: number } }\nconst ri: OuterI = { inner: { op: 1 } };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2741);
    let (_, start, length, _) = declared_here(&diagnostic);
    assert_eq!(span_text(source, start, length), "oq");
}

/// Two levels of nesting: the anchor follows the whole path, not just the
/// first hop. Oracle: `matrix.ts:6:36 - 'q' is declared here.`
#[test]
fn twice_nested_object_literal_pointer_anchors_at_the_innermost_member() {
    let source = "type Deep = { a: { b: { p: number; q: number } } };\nconst rd: Deep = { a: { b: { p: 1 } } };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2741);
    let (_, start, length, _) = declared_here(&diagnostic);
    assert_eq!(span_text(source, start, length), "q");
    // The outer member `b` shares no name with the leaf, but a path walk that
    // stopped one hop early would still find *a* plausible member; pin the
    // offset so a shallow anchor cannot pass.
    assert_eq!(length, 1, "underlines `q` alone, not a wider member span");
}

/// A nested member written as a *named* alias resolves through the symbol
/// route with no path walk at all — the alias declares the property itself.
/// Oracle: `matrix.ts:3:29 - 'iq' is declared here.` (in `Inner2`'s own body).
#[test]
fn nested_member_named_alias_anchors_in_the_alias_body() {
    let source = "type Inner2 = { ip: number; iq: number };\ntype Outer2 = { inner: Inner2 };\nconst r2: Outer2 = { inner: { ip: 1 } };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2741);
    let (_, start, length, _) = declared_here(&diagnostic);
    assert_eq!(span_text(source, start, length), "iq");
}

/// Negative arm, unchanged by the path walk: more than one unmatched property
/// is TS2739, and tsc attaches no `'x' is declared here.` pointer to it at any
/// depth. Oracle row 5 of the matrix carries only the TS6500 entry.
#[test]
fn nested_multi_property_failure_still_carries_no_pointer() {
    let source = "type Multi = { inner: { m1: number; m2: number; m3: number } };\nconst rm: Multi = { inner: { m1: 1 } };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2739);
    assert!(
        !diagnostic
            .related_information
            .iter()
            .any(|info| info.code == TS2728),
        "TS2739 carries no declared-here pointer: {diagnostic:?}"
    );
}

/// A computed member name in the path is not matchable against the written
/// annotation by name, so the walk abandons the whole path rather than
/// skipping the level and anchoring one member too shallow.
///
/// tsc *does* resolve this row (`matrix.ts:9:36`) because `K` is a `const` with
/// a literal type — declining here is a known, safe gap, not parity. What this
/// test pins is that the decline stays a decline: no pointer is better than a
/// pointer into the wrong member, which is the failure mode the name+kind guard
/// exists to prevent.
///
/// Asserted over every diagnostic rather than a chosen one: this witness also
/// diverges on its *primary* code (tsz reports TS2418 where tsc reports
/// TS2741), which is a separate defect from the pointer and must not decide
/// whether this test passes.
#[test]
fn computed_path_member_declines_rather_than_anchoring_shallow() {
    let source = "const K = \"inner\";\ntype Comp = { inner: { cp: number; cq: number } };\nconst rc: Comp = { [K]: { cp: 1 } };\n";
    let diagnostics = check_source_diagnostics(source);
    assert!(
        !diagnostics
            .iter()
            .flat_map(|d| d.related_information.iter())
            .any(|info| info.code == TS2728),
        "a computed path member produces no anchor at all: {diagnostics:?}"
    );
}

/// A literal nested inside an *array* member: the path reaches the member's
/// written type node, and `annotation_property_anchor` descends through the
/// array type's own element type to anchor at the missing property inside it.
/// `T[]` and `T` describe the same shape at every index, so no path segment is
/// consumed by the array itself. Oracle: `case.ts:1:34 - 'lq' is declared
/// here.`
#[test]
fn array_element_literal_anchors_through_the_element_type() {
    let source = "type Arr = { list: { lp: number; lq: number }[] };\nconst ra: Arr = { list: [{ lp: 1 }] };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2741);
    let (_, start, length, _) = declared_here(&diagnostic);
    assert_eq!(span_text(source, start, length), "lq");
}

/// Renamed binders: the array-element anchor must not depend on the
/// particular identifiers chosen above, only on the structural shape.
/// Oracle: `case.ts:2:43 - 'beta' is declared here.`
#[test]
fn array_element_literal_anchor_is_not_identifier_keyed() {
    let source = "type Roster = { members: { alpha: string; beta: string }[] };\nconst r: Roster = { members: [{ alpha: \"a\" }] };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2741);
    let (_, start, length, _) = declared_here(&diagnostic);
    assert_eq!(span_text(source, start, length), "beta");
}

/// The array's element type can itself be a named alias rather than an inline
/// type literal; the existing alias-chain fallback in
/// `annotation_property_anchor` composes with the new array-type descent with
/// no further change. Oracle: `case.ts:1:27 - 'lq' is declared here.` (inside
/// `Item`'s own body, not the array member's).
#[test]
fn array_element_named_alias_anchors_in_the_alias_body() {
    let source = "type Item = { lp: number; lq: number };\ntype ArrOfAlias = { list: Item[] };\nconst ra: ArrOfAlias = { list: [{ lp: 1 }] };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2741);
    let (_, start, length, _) = declared_here(&diagnostic);
    assert_eq!(span_text(source, start, length), "lq");
}

/// Negative control: more than one unmatched property inside an array element
/// is `TS2739`, which — same as the non-array nested case — carries no
/// `'x' is declared here.` pointer at all.
#[test]
fn array_element_multi_property_failure_still_carries_no_pointer() {
    let source = "type Multi = { list: { m1: number; m2: number; m3: number }[] };\nconst rm: Multi = { list: [{ m1: 1 }] };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2739);
    assert!(
        !diagnostic
            .related_information
            .iter()
            .any(|info| info.code == TS2728),
        "TS2739 carries no declared-here pointer: {diagnostic:?}"
    );
}

/// `Array<T>` holds the written element shape in a type argument exactly as
/// `T[]` holds it in its element type, and tsc anchors there identically.
/// Oracle: `case.ts:1:41 - 'lq' is declared here.`
#[test]
fn generic_array_type_argument_anchors_like_an_array_element() {
    let source = "type ArrG = { list: Array<{ lp: number; lq: number }> };\nconst ra: ArrG = { list: [{ lp: 1 }] };\n";
    let diagnostic = only(&check_source_diagnostics_with_libs(source), TS2741);
    let (_, start, length, _) = declared_here(&diagnostic);
    assert_eq!(span_text(source, start, length), "lq");
}

/// `ReadonlyArray<T>` is the same shape reached through a different global
/// name — the walk must not be keyed to either one.
/// Oracle: `case.ts:1:47 - 'lq' is declared here.`
#[test]
fn readonly_array_type_argument_anchors_like_an_array_element() {
    let source = "type ArrR = { list: ReadonlyArray<{ lp: number; lq: number }> };\nconst ra: ArrR = { list: [{ lp: 1 }] };\n";
    let diagnostic = only(&check_source_diagnostics_with_libs(source), TS2741);
    let (_, start, length, _) = declared_here(&diagnostic);
    assert_eq!(span_text(source, start, length), "lq");
}

/// `readonly T[]` describes the same element shape as `T[]`, so the operator is
/// transparent to the walk. Oracle: `case.ts:1:44 - 'lq' is declared here.`
#[test]
fn readonly_operator_is_transparent_to_the_element_walk() {
    let source = "type ArrRO = { list: readonly { lp: number; lq: number }[] };\nconst ra: ArrRO = { list: [{ lp: 1 }] };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2741);
    let (_, start, length, _) = declared_here(&diagnostic);
    assert_eq!(span_text(source, start, length), "lq");
}

/// The array-like descents must not depend on the identifiers chosen: the same
/// shapes under renamed binders anchor at the renamed property.
#[test]
fn array_like_type_argument_anchor_is_not_identifier_keyed() {
    let source = "type Roster = { members: Array<{ alpha: string; beta: string }> };\nconst r: Roster = { members: [{ alpha: \"a\" }] };\n";
    let diagnostic = only(&check_source_diagnostics_with_libs(source), TS2741);
    let (_, start, length, _) = declared_here(&diagnostic);
    assert_eq!(span_text(source, start, length), "beta");
}

/// `keyof T` denotes T's *keys*, not T, so the type operator is not blanket
/// transparent — only `readonly` is. This pins the scoping of the operator
/// case; tsc reports a plain TS2322 here with no missing-property pointer.
#[test]
fn keyof_operator_is_not_transparent_to_the_element_walk() {
    let source =
        "type K = { obj: keyof { aa: number; ab: number } };\nconst k: K = { obj: \"zz\" };\n";
    let diagnostics = check_source_diagnostics(source);
    assert!(
        diagnostics.iter().all(|d| d.code != TS2741),
        "keyof denotes keys, not the object shape: {diagnostics:?}"
    );
}

/// When two type arguments both declare the property the walk has no basis to
/// choose, and pointing at the wrong occurrence is worse than omitting the
/// pointer. Reaching tsc's answer here needs type-parameter substitution
/// through the alias body (`p.a` -> `A` -> the first written argument), which
/// this walk does not model; see #16552.
#[test]
fn ambiguous_type_arguments_decline_rather_than_guessing() {
    let source = "type Pair<A, B> = { a: A; b: B };\ntype W = { p: Pair<{ lq: number }, { lq: string }> };\nconst w: W = { p: { a: {}, b: { lq: \"x\" } } };\n";
    let diagnostics = check_source_diagnostics(source);
    for diagnostic in diagnostics.iter().filter(|d| d.code == TS2741) {
        assert!(
            diagnostic
                .related_information
                .iter()
                .all(|info| info.code != TS2728),
            "two arguments declare 'lq'; the walk must decline: {diagnostic:?}"
        );
    }
}

// The three tests below were written for #16559, which fixed the same family
// with a lib-provenance gate. They are salvaged here so its coverage survives
// the merge, adapted to this file's helper names; the `Array`-shadow control
// holds for a different reason under this walk (see its own comment).

/// `readonly Array<T>` puts the operator peel and the type-argument descent in
/// one annotation; both must compose.
/// Oracle: `case.ts:1:53 - 'cq' is declared here.`
#[test]
fn readonly_array_type_reference_composes_with_the_type_operator_peel() {
    let source = "type Combo = { items: readonly Array<{ cp: number; cq: number }> };\nconst c2: Combo = { items: [{ cp: 1 }] };\n";
    let diagnostic = only(&check_source_diagnostics_with_libs(source), TS2741);
    let (_, start, length, _) = declared_here(&diagnostic);
    assert_eq!(span_text(source, start, length), "cq");
}

/// A user-defined generic interface merely *named* `Array` must not be given
/// the global's treatment. This walk never asks whether a reference is the lib
/// `Array` — it asks whether the reference's own members or alias body declare
/// the property, and only then looks at the type arguments. Here the shadowing
/// `Array<T>` declares `held`, the failing literal is reached through the
/// `list` path segment, and nothing anchors: the same answer a lib-provenance
/// gate gives, arrived at without consulting the name.
#[test]
fn user_defined_array_named_type_does_not_take_the_element_descent() {
    let source = "interface Array<T> { held: T }\ntype Boxed = { list: Array<{ up: number; uq: number }> };\nconst u: Boxed = { list: { up: 1 } };\n";
    let diagnostics = check_source_diagnostics_with_libs(source);
    assert!(
        !diagnostics
            .iter()
            .flat_map(|d| d.related_information.iter())
            .any(|info| info.code == TS2728),
        "a shadowed `Array` must not anchor through the element descent: {diagnostics:?}"
    );
}

// The gap this file pinned as `tuple_element_literal_still_carries_no_pointer`
// is closed by `a_tuple_element_literal_anchors_in_the_element_type` at the end
// of this file, which asserts the same source's oracle anchor instead of its
// absence. Tracked in #16552.

// ---------------------------------------------------------------------------
// An array literal written for a tuple-typed *member* keeps its tuple form.
//
// Structural rule, oracled against `typescript@7.0.2`
// (`--noEmit --strict --pretty --target es2022 --lib es2022`): the type of an
// array literal written for a tuple-typed target is the tuple it wrote, in a
// nested object-literal member exactly as in a direct annotation. tsz reaches
// that through the elaboration's contextual re-type
// (`error_reporter/call_errors/elaboration_object_properties.rs`), which
// previously kept the *widened* cached array (`{ x: number }[]`) whenever the
// contextual form still failed. Handing the widened array to the relation
// erases the element count, and the failure was then explained as the
// unbounded-source arity gap — `TS2620` "Target requires N element(s) but
// source may have fewer" on a literal that wrote exactly N elements.
//
// These rows pin the two things that follow: the false arity reason is gone,
// and a *genuine* arity mismatch reports tsc's own `TS2618` counts.
// ---------------------------------------------------------------------------

const TS2322: u32 = 2322; // Type X is not assignable to type Y.
const TS2618: u32 = 2618; // Source has N element(s) but target requires M.
const TS2620: u32 = 2620; // Target requires N element(s) but source may have fewer.

fn related_codes(diagnostics: &[Diagnostic]) -> Vec<u32> {
    diagnostics
        .iter()
        .flat_map(|d| d.related_information.iter())
        .map(|info| info.code)
        .collect()
}

fn codes(diagnostics: &[Diagnostic]) -> Vec<u32> {
    diagnostics.iter().map(|d| d.code).collect()
}

/// The headline row. A one-element literal for a one-slot tuple whose element
/// misses a required property must not be explained as an arity gap: the
/// counts agree. tsc reports `TS2741`; the false `TS2620` is what this fixes.
#[test]
fn tuple_member_missing_property_is_not_reported_as_an_arity_gap() {
    let source =
        "type Tc = { p: [{ x: number; y: number }] };\nconst vc: Tc = { p: [{ x: 1 }] };\n";
    let diagnostics = check_source_diagnostics(source);
    assert!(
        !related_codes(&diagnostics).contains(&TS2620),
        "one written element cannot be 'fewer' than one required: {diagnostics:?}"
    );
    assert_eq!(codes(&diagnostics), vec![TS2741], "{diagnostics:?}");
}

/// Renamed binders and a different arity: the recovery keys on the shape
/// (cached form lost tuple-ness, contextual form regained it, target is a
/// tuple), never on the identifiers written above it.
#[test]
fn tuple_member_recovery_is_not_identifier_keyed() {
    let source = "type Roster = { slots: [{ alpha: string; beta: string }, { gamma: string; delta: string }] };\nconst r: Roster = { slots: [{ alpha: \"a\", beta: \"b\" }, { gamma: \"g\" }] };\n";
    let diagnostics = check_source_diagnostics(source);
    assert!(
        !related_codes(&diagnostics).contains(&TS2620),
        "{diagnostics:?}"
    );
    assert_eq!(codes(&diagnostics), vec![TS2741], "{diagnostics:?}");
}

/// A named tuple element (`[first: T]`) is the same tuple to the relation, so
/// it takes the same recovery. Oracle reports `TS2741` here too.
#[test]
fn named_tuple_member_element_takes_the_same_recovery() {
    let source = "type Tup4 = { pair: [first: { na: number; nb: number }] };\nconst v4: Tup4 = { pair: [{ na: 1 }] };\n";
    let diagnostics = check_source_diagnostics(source);
    assert!(
        !related_codes(&diagnostics).contains(&TS2620),
        "{diagnostics:?}"
    );
    assert_eq!(codes(&diagnostics), vec![TS2741], "{diagnostics:?}");
}

/// A rest-tailed tuple target (`[T, ...T[]]`) reached the same false reason
/// family through `TS2623` ("Source provides no match for required element at
/// position 0"), which is equally untrue of a literal that wrote that element.
#[test]
fn rest_tailed_tuple_member_keeps_its_written_leading_element() {
    let source = "type Tup9 = { pair: [{ ca: number; cb: number }, ...{ ca: number; cb: number }[]] };\nconst v9: Tup9 = { pair: [{ ca: 1 }] };\n";
    let diagnostics = check_source_diagnostics(source);
    assert!(
        related_codes(&diagnostics)
            .iter()
            .all(|&code| code != TS2620),
        "{diagnostics:?}"
    );
    assert_eq!(codes(&diagnostics), vec![TS2741], "{diagnostics:?}");
}

/// The negative control that keeps the recovery honest: a literal that really
/// does write too few elements still fails on arity — and now with tsc's own
/// reason and counts. Oracle:
/// `TS2322` … / `Source has 1 element(s) but target requires 2.`
#[test]
fn a_real_tuple_arity_shortfall_reports_tscs_own_counts() {
    let source = "type Tup8 = { pair: [{ ea: number }, { eb: number }] };\nconst v8: Tup8 = { pair: [{ ea: 1 }] };\n";
    let diagnostics = check_source_diagnostics(source);
    assert_eq!(codes(&diagnostics), vec![TS2322], "{diagnostics:?}");
    assert!(
        related_codes(&diagnostics).contains(&TS2618),
        "a genuine shortfall keeps an arity reason, with the source's own \
         count rather than the unbounded-source form: {diagnostics:?}"
    );
    let arity = diagnostics[0]
        .related_information
        .iter()
        .find(|info| info.code == TS2618)
        .expect("TS2618");
    assert_eq!(
        arity.message_text, "Source has 1 element(s) but target requires 2.",
        "{diagnostics:?}"
    );
}

/// A tuple member that is fully satisfied stays clean — the recovery must not
/// invent a failure where the contextual form relates.
#[test]
fn a_satisfied_tuple_member_stays_clean() {
    let source = "type Tup1 = { pair: [{ lp: number; lq: number }] };\nconst ok1: Tup1 = { pair: [{ lp: 1, lq: 2 }] };\n";
    assert!(
        check_source_diagnostics(source).is_empty(),
        "{:?}",
        check_source_diagnostics(source)
    );
}

/// An *array*-typed member is untouched: its cached form is already the right
/// shape, so the recovery declines and the element-wise pointer this file
/// covers keeps working.
#[test]
fn an_array_typed_member_is_untouched_by_the_tuple_recovery() {
    let source = "type Arr7 = { list: { la: number; lb: number }[] };\nconst v7: Arr7 = { list: [{ la: 1 }] };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2741);
    let (_, start, length, _) = declared_here(&diagnostic);
    assert_eq!(span_text(source, start, length), "lb");
}

/// A non-literal member value cannot be re-typed contextually, so an array
/// *variable* assigned to a tuple member keeps the unbounded-source reason —
/// which is correct there, because the array really may have fewer elements.
#[test]
fn an_array_variable_assigned_to_a_tuple_member_keeps_the_arity_reason() {
    let source = "type Tz = { p: [{ x: number; y: number }] };\nconst xs: { x: number; y: number }[] = [];\nconst vz: Tz = { p: xs };\n";
    let diagnostics = check_source_diagnostics(source);
    assert!(
        related_codes(&diagnostics).contains(&TS2620),
        "an unbounded array source genuinely may have fewer: {diagnostics:?}"
    );
}

// ---------------------------------------------------------------------------
// A `readonly` tuple target reaches the same `TS2741` a mutable tuple does.
//
// Structural rule, oracled against `typescript@7.0.2`
// (`--noEmit --strict --pretty --target es2022 --lib es2022`): tsc elaborates
// a `readonly [T]` target's missing-property mismatch exactly like `[T]`'s —
// the `readonly` modifier is not itself the failure. tsz's solver already
// gets this right end-to-end (`explain_tuple_failure` returns the correct
// `MissingProperty` reason for both), so the bug is a checker-side rendering
// gate mis-firing, not a missing solver descent.
//
// Root cause: `render_missing_property`'s "target has an index signature but
// the missing property is not one of its own named properties" bail-out
// (`crates/tsz-checker/src/error_reporter/render_failure_missing_property.rs`)
// decides whether the target is array/tuple-shaped with
// `array_element_type(target).is_some() || tuple_list_id(target).is_some()`.
// `array_element_type` (`type_queries::data::get_array_element_type`) already
// unwraps a `ReadonlyType` wrapper, but `tuple_list_id`
// (`visitors::visitor_extract::tuple_list_id`) does not — it matches only
// `TypeData::Tuple` directly. For a `readonly [T]` target that asymmetry
// makes `target_is_array_or_tuple` false, so the tuple's own implicit numeric
// index signature is treated as a real index signature, the missing property
// (a named property of the *element*, not the tuple) fails the "named
// property of target" check, and the function bails to a bare `TS2322` with
// no elaboration at all — before the tuple-element drill this file's anchor
// walk depends on ever runs. This is exactly why `Array<T>`/`ReadonlyArray<T>`/
// `readonly T[]` were already fixed (#16551/#16556) while the tuple spelling
// alone stayed broken (#16552): every one of those goes through
// `array_element_type`, which already peels `readonly`.
//
// The fix swaps the raw `tuple_list_id(target).is_some()` check for
// `is_tuple_type(target)` (`visitors::visitor_predicates::is_tuple_type`),
// which already peels `ReadonlyType`/`Substitution` — the same query this
// file's own anchor walk and the solver's `explain.rs` already use for the
// identical "is this shape a tuple" question. `tuple_list_id` itself is left
// alone: it has 24 call sites across the solver's core relation dispatch, and
// widening what it matches is a much larger blast radius than this one
// boolean gate needs.
// ---------------------------------------------------------------------------

/// The headline row: a `readonly` tuple member's missing-property failure
/// reaches `TS2741`, matching the mutable-tuple case
/// (`tuple_member_missing_property_is_not_reported_as_an_arity_gap` above).
/// Oracle: `2:23 - TS2741 Property 'rq' is missing in type '{ rp: number; }' …`
#[test]
fn readonly_tuple_member_missing_property_reaches_ts2741() {
    let source = "type R1 = { tup: readonly [{ rp: number; rq: number }] };\nconst r: R1 = { tup: [{ rp: 1 }] };\n";
    let diagnostics = check_source_diagnostics(source);
    assert_eq!(codes(&diagnostics), vec![TS2741], "{diagnostics:?}");
}

/// The same rule for a `readonly` tuple written directly as a binding's own
/// annotation, with no wrapping object-literal member — the bail-out this
/// fixes runs on this shape too, and unlike the member row above needs no
/// contextual-retype recovery at all: the literal is typed against the
/// annotation from the start.
/// Oracle: `2:16 - TS2741 Property 'rq' is missing in type '{ rp: number; }' …`
#[test]
fn readonly_tuple_direct_annotation_missing_property_reaches_ts2741() {
    let source = "type Ro = readonly [{ rp: number; rq: number }];\nconst v: Ro = [{ rp: 1 }];\n";
    let diagnostics = check_source_diagnostics(source);
    assert_eq!(codes(&diagnostics), vec![TS2741], "{diagnostics:?}");
}

/// A multi-element `readonly` tuple: the fix must not just special-case a
/// one-element tuple, and a genuinely *unrelated* missing-index-signature
/// bail-out (a real index-signature-vs-index-signature target) must still
/// fire for the type that is not array/tuple shaped at all.
#[test]
fn readonly_tuple_with_two_elements_missing_property_reaches_ts2741() {
    let source = "type R2 = { tup: readonly [{ ra: number }, { rb: number; rc: number }] };\nconst r: R2 = { tup: [{ ra: 1 }, { rb: 2 }] };\n";
    let diagnostics = check_source_diagnostics(source);
    assert_eq!(codes(&diagnostics), vec![TS2741], "{diagnostics:?}");
}

/// Negative control: a genuine index-signature-to-index-signature target
/// (not array/tuple-shaped at all) must keep the generic `TS2322` this
/// bail-out exists for — the fix narrows `target_is_array_or_tuple`'s
/// readonly blind spot, it does not remove the check.
/// Oracle: a single `TS2322` — `'string' index signatures are incompatible.`
#[test]
fn indexed_object_target_keeps_the_generic_index_signature_mismatch() {
    let source = "interface SrcIdx { [k: string]: boolean }\ninterface Idx { [k: string]: number }\ndeclare const s: SrcIdx;\nconst v: Idx = s;\n";
    let diagnostics = check_source_diagnostics(source);
    assert_eq!(codes(&diagnostics), vec![TS2322], "{diagnostics:?}");
}

/// Renamed binders and a different property name: the fix keys on the
/// target's *shape* (readonly tuple), never on an identifier written above
/// it.
#[test]
fn readonly_tuple_recovery_is_not_identifier_keyed() {
    let source = "type Roster2 = { slots: readonly [{ alpha: string; beta: string }] };\nconst r: Roster2 = { slots: [{ alpha: \"a\" }] };\n";
    let diagnostics = check_source_diagnostics(source);
    assert_eq!(codes(&diagnostics), vec![TS2741], "{diagnostics:?}");
}

// A tuple element type is descended into exactly as an array element type is.
//
// Structural rule, oracled against `typescript@7.0.2`
// (`--noEmit --strict --pretty --target es2022 --lib es2022`): when the
// unmatched property is missing from an object literal written as a *tuple
// element*, tsc anchors `'x' is declared here.` inside that element's own
// written type, exactly as it does for an array element. tsz owns this in the
// annotation walk (`annotation_property_anchor`), which previously handled
// `ARRAY_TYPE` but stopped at `TUPLE_TYPE`.
//
// The walk carries no element position — `contextual_property_path` skips
// `ARRAY_LITERAL_EXPRESSION` without pushing a segment — so a tuple is
// descended by the same uniqueness discipline `unique_type_argument_anchor`
// already applies to type arguments: anchor when exactly one element type
// declares the property, decline when two do. Declining loses a pointer;
// guessing points at the wrong declaration.
// ---------------------------------------------------------------------------

/// The row #16552 pinned as a known gap while the primary was still a
/// whole-tuple `TS2322`. #16586 fixed the primary to `TS2741`, which made the
/// anchor walk reachable; this closes the pointer half.
///
/// Oracle: `t1.ts:1:36 - 'tq' is declared here.`
#[test]
fn a_tuple_element_literal_anchors_in_the_element_type() {
    let source = "type Nest3 = { tup: [{ tp: number; tq: number }] };\nconst c: Nest3 = { tup: [{ tp: 1 }] };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2741);
    let (_, start, length, _) = declared_here(&diagnostic);
    assert_eq!(span_text(source, start, length), "tq");
}

/// A `readonly [T]` element used to stop here too — the primary was still a
/// whole-tuple `TS2322`, so there was no `TS2741` for this file's own anchor
/// walk to hang a pointer on. That primary is now fixed (checker-side
/// rendering gate, not this walk — see `readonly_tuple_member_missing_property_reaches_ts2741`
/// above), so the walk's `readonly` `TYPE_OPERATOR` peel composes for free and
/// this row now anchors too.
///
/// Oracle: `t2.ts:2:23 - TS2741 Property 'rq' is missing …` with
/// `t2.ts:1:42 - 'rq' is declared here.`
#[test]
fn a_readonly_tuple_element_anchors_in_the_element_type() {
    let source = "type R1 = { tup: readonly [{ rp: number; rq: number }] };\nconst r: R1 = { tup: [{ rp: 1 }] };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2741);
    let (_, start, length, _) = declared_here(&diagnostic);
    assert_eq!(span_text(source, start, length), "rq");
}

/// A tuple written directly as the binding's annotation, with no enclosing
/// object literal: `contextual_property_path` is empty and the descent starts
/// at the annotation itself.
///
/// Oracle: `t3.ts:1:26 - 'dq' is declared here.`
#[test]
fn a_top_level_tuple_annotation_anchors_without_a_property_path() {
    let source = "type D1 = [{ dp: number; dq: number }];\nconst d: D1 = [{ dp: 1 }];\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2741);
    let (_, start, length, _) = declared_here(&diagnostic);
    assert_eq!(span_text(source, start, length), "dq");
}

/// A heterogeneous tuple: only the second element declares `bq`, so the
/// uniqueness rule resolves it even though the walk carries no position.
///
/// Oracle: `t4.ts:1:49 - 'bq' is declared here.`
#[test]
fn a_multi_element_tuple_anchors_in_the_one_element_declaring_the_property() {
    let source = "type M1 = { tup: [{ ap: number }, { bp: number; bq: number }] };\nconst m: M1 = { tup: [{ ap: 1 }, { bp: 2 }] };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2741);
    let (_, start, length, _) = declared_here(&diagnostic);
    assert_eq!(span_text(source, start, length), "bq");
}

/// A named tuple member (`[first: T]`) is the same element type wearing a
/// label; the label is not a property name and contributes no path segment.
///
/// Oracle: `t6.ts:1:40 - 'nq' is declared here.`
#[test]
fn a_named_tuple_member_anchors_in_its_element_type() {
    let source = "type N1 = { tup: [first: { np: number; nq: number }] };\nconst n: N1 = { tup: [{ np: 1 }] };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2741);
    let (_, start, length, _) = declared_here(&diagnostic);
    assert_eq!(span_text(source, start, length), "nq");
}

/// A tuple element written as a *reference* to a named alias hands off to the
/// existing reference arm, which resolves the alias body — the tuple level
/// only has to get the walk there.
///
/// Oracle: `t7.ts:1:26 - 'eq' is declared here.`
#[test]
fn a_tuple_element_type_reference_resolves_through_its_alias() {
    let source = "type El1 = { ep: number; eq: number };\ntype P1 = { tup: [El1] };\nconst p: P1 = { tup: [{ ep: 1 }] };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2741);
    let (_, start, length, _) = declared_here(&diagnostic);
    assert_eq!(span_text(source, start, length), "eq");
}

// The array-*of*-tuples half of the gap — an array literal in a tuple-typed
// element slot stopping at the inner whole-tuple `TS2322` — is closed at the
// elaboration owner (#16631) rather than in this walk; its rows live in the
// block at the end of this file (`an_array_of_tuples_reaches_ts2741_at_the_inner_element`
// and siblings). Both descents this file needs (`ARRAY_TYPE` then `TUPLE_TYPE`)
// were already in place; the primary was the blocker.

/// Two elements declare the *same* property name. Each failing element is a
/// distinct literal, so each is reported as its own `TS2741`, and the pointer
/// is resolved by the failing element's *position* — the first `qq` anchors in
/// the first element's type, the second in the second's. tsc does exactly this
/// (`t5.ts:1:33` and `t5.ts:1:61`); uniqueness alone could not choose between
/// the two identical declarations, so the element index threaded from the
/// failing array-literal element is what disambiguates.
#[test]
fn two_tuple_elements_declaring_the_same_property_anchor_by_position() {
    let source = "type A1 = { tup: [{ xp: number; qq: number }, { yp: number; qq: number }] };\nconst a: A1 = { tup: [{ xp: 1 }, { yp: 2 }] };\n";
    let anchors = ts2741_anchors_in_order(source);
    assert_eq!(
        anchors.len(),
        2,
        "each failing tuple element is its own TS2741"
    );
    // Both name `qq`, but each points at a *different* `qq` declaration — the
    // one inside its own element's type, in source order.
    assert_eq!(anchors[0].1, "qq");
    assert_eq!(anchors[1].1, "qq");
    assert_eq!(
        anchors[0].0,
        source.find("qq").unwrap() as u32,
        "first element points at the first qq"
    );
    assert_eq!(
        anchors[1].0,
        source.rfind("qq").unwrap() as u32,
        "second element points at the second qq"
    );
}

/// The false-negative twin: two elements each miss a *different* required
/// property. tsz used to report only the first (`ax`) and silently drop the
/// second; tsc reports both. Each is now its own `TS2741` anchored at its own
/// element.
#[test]
fn two_tuple_elements_missing_different_properties_both_report() {
    let source = "type B1 = { tup: [{ xp: number; ax: number }, { yp: number; by: number }] };\nconst b: B1 = { tup: [{ xp: 1 }, { yp: 2 }] };\n";
    let names: Vec<String> = ts2741_anchors_in_order(source)
        .into_iter()
        .map(|(_, name)| name)
        .collect();
    assert_eq!(
        names,
        vec!["ax", "by"],
        "each element anchors its own missing prop"
    );
}

/// Renamed binders: the per-element anchoring keys on structure and position,
/// never on the property spelling, so renaming every binder leaves the two
/// same-named-property pointers landing on their own elements.
#[test]
fn two_tuple_elements_same_property_position_is_binder_name_independent() {
    let source = "type Payload = { slots: [{ alpha: number; shared: number }, { beta: number; shared: number }] };\nconst p: Payload = { slots: [{ alpha: 1 }, { beta: 2 }] };\n";
    let anchors = ts2741_anchors_in_order(source);
    assert_eq!(anchors.len(), 2);
    assert_eq!(anchors[0].0, source.find("shared").unwrap() as u32);
    assert_eq!(anchors[1].0, source.rfind("shared").unwrap() as u32);
}

/// A `readonly` two-element tuple: the `readonly` `TYPE_OPERATOR` peel is
/// transparent and carries the element index through, so both same-named
/// pointers still land by position.
#[test]
fn readonly_two_tuple_elements_same_property_anchor_by_position() {
    let source = "type R2 = { tup: readonly [{ xp: number; qq: number }, { yp: number; qq: number }] };\nconst r: R2 = { tup: [{ xp: 1 }, { yp: 2 }] };\n";
    let anchors = ts2741_anchors_in_order(source);
    assert_eq!(anchors.len(), 2);
    assert_eq!(anchors[0].0, source.find("qq").unwrap() as u32);
    assert_eq!(anchors[1].0, source.rfind("qq").unwrap() as u32);
}

/// `keyof` is not transparent the way `readonly` is, and the tuple arm must
/// not make it so: `keyof [T]` denotes the tuple's *keys*, not its elements.
#[test]
fn keyof_over_a_tuple_stays_opaque_to_the_element_descent() {
    let source = "type G1 = { tup: keyof [{ gp: number; gq: number }] };\nconst g: G1 = { tup: { gp: 1 } as never };\n";
    let diagnostics = check_source_diagnostics(source);
    assert!(
        !diagnostics
            .iter()
            .flat_map(|d| d.related_information.iter())
            .any(|info| info.code == TS2728),
        "`keyof` must not descend into the tuple element: {diagnostics:?}"
    );
}

// An array literal written for a tuple-typed *element slot* recovers its
// tuple form, exactly as one written for a tuple-typed *property* does.
//
// Structural rule, oracled against `typescript@7.0.2`
// (`--noEmit --strict --pretty --target es2022 --lib es2022`): when an array
// literal is written for a slot whose declared type is a tuple, tsc
// (`elaborateElementwise`) compares the *written* element list against the
// tuple, so a missing property in one of its object-literal elements is
// reported as `TS2741` with the `'x' is declared here.` pointer into the
// element's own written type.
//
// tsz cached that inner literal as the widened array (`{ kp: number }[]`) —
// its own check ran before the slot's contextual type was available — and
// handed the widened form to the relation. An unbounded source against a
// closed tuple takes the arity branch, so every one of these rows reported
// `TS2322` plus the sub-message `Target requires 1 element(s) but source may
// have fewer.`, which is false on its face for a literal that wrote exactly
// one element, and it hid the real missing-property failure and its pointer.
//
// The owner is the array-element half of the elaborator
// (`error_reporter/call_errors/elaboration_object_properties.rs`,
// `try_elaborate_array_literal_elements`), which now applies the same
// `contextual_tuple_recovers_elementwise_failure` predicate the object-literal
// property half already applied for this shape. The recovery is gated on the
// cached form having lost tuple-ness, the contextual form having regained it,
// and the target slot being a tuple, so a genuinely unbounded source (an array
// *variable*, not a literal) keeps today's arity reason — see
// `an_unbounded_array_variable_in_a_tuple_slot_keeps_the_arity_reason`.
//
// tsz anchors the primary at the inner array literal where tsc anchors it at
// the failing object literal one node further in; that difference is shared
// with the already-accepted single-level tuple rows above (where tsz anchors at
// the property name) and is not what these rows pin. The `TS2728` offsets below
// are the oracle's exact ones.
// ---------------------------------------------------------------------------

/// `[ T ][]` — an array of tuples, the row #16552 left open at the elaboration
/// owner after #16599 closed the anchor walk's own tuple descent.
///
/// Oracle: `k1.ts:2:24 - TS2741 Property 'kq' is missing …` with
/// `k1.ts:1:33 - 'kq' is declared here.`
#[test]
fn an_array_of_tuples_reaches_ts2741_at_the_inner_element() {
    let source = "type K1 = { tup: [{ kp: number; kq: number }][] };\nconst k: K1 = { tup: [[{ kp: 1 }]] };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2741);
    let (_, start, length, _) = declared_here(&diagnostic);
    assert_eq!(span_text(source, start, length), "kq");
}

/// The `readonly` spelling of the same array: the recovery keys on the target
/// slot being a tuple, which `is_tuple_type` already sees through a `readonly`
/// wrapper, so this row composes without its own arm.
///
/// Oracle: `k2.ts:1:42 - 'kq' is declared here.`
#[test]
fn a_readonly_array_of_tuples_reaches_ts2741_at_the_inner_element() {
    let source = "type K2 = { tup: readonly [{ kp: number; kq: number }][] };\nconst k: K2 = { tup: [[{ kp: 1 }]] };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2741);
    let (_, start, length, _) = declared_here(&diagnostic);
    assert_eq!(span_text(source, start, length), "kq");
}

/// The same array of tuples written directly as the binding's own annotation,
/// with no enclosing object literal — the recovery is an element-slot rule, not
/// a property-value one, so it must not depend on there being a member above.
///
/// Oracle: `k3.ts:1:26 - 'kq' is declared here.`
#[test]
fn a_directly_annotated_array_of_tuples_reaches_ts2741() {
    let source = "type K3 = [{ kp: number; kq: number }][];\nconst k: K3 = [[{ kp: 1 }]];\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2741);
    let (_, start, length, _) = declared_here(&diagnostic);
    assert_eq!(span_text(source, start, length), "kq");
}

/// A tuple *of* tuples: here the enclosing literal is the one whose cached form
/// is a widened array, so the source-tuple slot the elaborator recovers is
/// itself widened. This is the row that needs the recovery applied to the
/// recovered slot as well as to the context-free element type.
///
/// Oracle: `k5.ts:1:34 - 'kq' is declared here.`
#[test]
fn a_tuple_of_tuples_reaches_ts2741_at_the_inner_element() {
    let source = "type K5 = { tup: [[{ kp: number; kq: number }]] };\nconst k: K5 = { tup: [[{ kp: 1 }]] };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2741);
    let (_, start, length, _) = declared_here(&diagnostic);
    assert_eq!(span_text(source, start, length), "kq");
}

/// Two levels of array around the tuple: the recovery runs per element slot, so
/// it composes at every depth rather than only at the first.
///
/// Oracle: `m6.ts:2:25 - TS2741 …` (the pointer lands on `kq` in the element).
#[test]
fn an_array_of_arrays_of_tuples_reaches_ts2741_at_the_inner_element() {
    let source = "type M6 = { tup: [{ kp: number; kq: number }][][] };\nconst m: M6 = { tup: [[[{ kp: 1 }]]] };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2741);
    let (_, start, length, _) = declared_here(&diagnostic);
    assert_eq!(span_text(source, start, length), "kq");
}

/// The `Array< [ T ] >` spelling, which reaches the same slot through the
/// generic reference rather than the `T[]` shorthand. Needs the real lib.
///
/// Oracle: `m1.ts:2:24 - TS2741 Property 'kq' is missing …`
#[test]
fn a_generic_array_of_tuples_reaches_ts2741_at_the_inner_element() {
    let source = "type M1 = { tup: Array<[{ kp: number; kq: number }]> };\nconst m: M1 = { tup: [[{ kp: 1 }]] };\n";
    let diagnostic = only(&check_source_diagnostics_with_libs(source), TS2741);
    let (_, start, length, _) = declared_here(&diagnostic);
    assert_eq!(span_text(source, start, length), "kq");
}

/// Renamed binders and different property names: the recovery keys on the
/// target slot's *shape*, never on an identifier written above it.
///
/// Oracle: `m5.ts:2:29 - TS2741 Property 'beta' is missing …`
#[test]
fn the_array_of_tuples_recovery_is_not_identifier_keyed() {
    let source = "type Roster = { rows: [{ alpha: string; beta: string }][] };\nconst r: Roster = { rows: [[{ alpha: \"a\" }]] };\n";
    let diagnostic = only(&check_source_diagnostics(source), TS2741);
    let (_, start, length, _) = declared_here(&diagnostic);
    assert_eq!(span_text(source, start, length), "beta");
}

/// Negative control, and the reason the recovery is gated rather than blanket:
/// an array *variable* handed to a tuple-typed slot really may have fewer
/// elements, so tsc keeps the arity reason there. The contextual retype of an
/// identifier is still the array, so the predicate declines and this row is
/// unchanged.
///
/// Oracle: `m3.ts(3,23): TS2322 Type '{ kp: number; }[]' is not assignable to
/// type '[{ kp: number; }]'.` / `Target requires 1 element(s) but source may
/// have fewer.`
#[test]
fn an_unbounded_array_variable_in_a_tuple_slot_keeps_the_arity_reason() {
    let source = "type M3 = { tup: [{ kp: number }][] };\ndeclare const loose: { kp: number }[];\nconst m: M3 = { tup: [loose] };\n";
    let diagnostics = check_source_diagnostics(source);
    assert!(
        diagnostics.iter().all(|d| d.code != TS2741),
        "an unbounded array source must keep the arity reason: {diagnostics:?}"
    );
}

/// Negative control: a *genuine* element shortfall inside the written literal
/// stays an arity failure. Recovering the tuple form does not invent elements,
/// so a one-element literal against a two-element tuple keeps tsc's
/// `Source has 1 element(s) but target requires 2.`
///
/// Oracle: `m2.ts(2,23): TS2322 …` / `Source has 1 element(s) but target
/// requires 2.`
#[test]
fn a_real_element_shortfall_inside_a_tuple_slot_stays_an_arity_failure() {
    let source = "type M2 = { tup: [{ ka: number }, { kb: number }][] };\nconst m: M2 = { tup: [[{ ka: 1 }]] };\n";
    let diagnostics = check_source_diagnostics(source);
    assert!(
        diagnostics.iter().all(|d| d.code != TS2741),
        "a real shortfall must not be recast as a missing property: {diagnostics:?}"
    );
}

/// Negative control: a property whose *type* is wrong (not missing) inside the
/// nested tuple element keeps the property-level `TS2322` and grows no
/// `TS2728` pointer — `TS2728` pairs only with the missing-property form.
///
/// Oracle: `m4.ts(2,26): TS2322 Type 'string' is not assignable to type
/// 'number'.`
#[test]
fn a_wrong_property_type_inside_a_tuple_slot_stays_ts2322() {
    let source =
        "type M4 = { tup: [{ kp: number }][] };\nconst m: M4 = { tup: [[{ kp: \"s\" }]] };\n";
    let diagnostics = check_source_diagnostics(source);
    assert!(
        diagnostics.iter().all(|d| d.code != TS2741),
        "a wrong property type is not a missing property: {diagnostics:?}"
    );
    assert!(
        !diagnostics
            .iter()
            .flat_map(|d| d.related_information.iter())
            .any(|info| info.code == TS2728),
        "no declared-here pointer belongs on a value mismatch: {diagnostics:?}"
    );
}
