//! Regression tests for cross-file type references whose raw `SymbolId`
//! collides with an unrelated declaration in the imported file.
//!
//! Structural rule (pinned against `typescript@7.0.2`, the conformance pin):
//! a type reference to an imported name denotes the entity that name is
//! *imported as*, whatever ordinal position its declaration occupies in the
//! declaring file. tsz resolves that through `type_reference_symbol_type`,
//! whose declaration metadata may only come from the declaring file's binder
//! when the raw `SymbolId` genuinely names the imported entity there.
//!
//! Per-file binders mint raw `SymbolId`s from zero with no `base_offset`, so
//! `import { Shape } from "./dep"` in one file and an unrelated `Unused` in
//! `./dep` routinely share an id. `get_symbol_from_registered_file_target`
//! answers the right *file* but indexes it with the consuming file's raw id,
//! which lands on whichever declaration sits at that ordinal — right only by
//! coincidence, when the imported entity happens to be the declaring file's
//! first declaration. When the collision landed on an `INTERFACE` or `CLASS`
//! symbol it drove a committing branch of `type_reference_symbol_type` and
//! produced a diagnostic naming the *wrong target type* and a *wrong missing
//! property* (the class's static side, so `prototype`); when it landed on a
//! type alias, variable, function, or enum the fallback path re-resolved the
//! alias and the answer came out right. Every row below therefore varies the
//! declaration that precedes the imported one.
//!
//! The oracle for each row is recorded inline as the tsc output it was pinned
//! against.

use crate::context::CheckerOptions;
use crate::diagnostics::Diagnostic;
use crate::test_utils::check_multi_file_with_libs_stamped;
use tsz_common::diagnostics::diagnostic_codes;

const TS2741: u32 = diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE;

const SHAPE_CLASS: &str = "export class Shape { held: number = 0; away: number = 0; }\n";
const IMPORT_SHAPE_ANNOTATION: &str =
    "import { Shape } from \"./dep\";\nconst s: Shape = { held: 1 };\n";

fn diagnostics(dep: &str, main: &str) -> Vec<Diagnostic> {
    check_multi_file_with_libs_stamped(
        &[("dep.ts", dep), ("main.ts", main)],
        "main.ts",
        CheckerOptions::default(),
        &[],
    )
}

/// The single diagnostic of `code`, as `(message, count of all diagnostics)`.
fn only_message(dep: &str, main: &str, code: u32) -> String {
    let diags = diagnostics(dep, main);
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
    matching[0].message_text.clone()
}

fn assert_clean(dep: &str, main: &str) {
    let diags = diagnostics(dep, main);
    assert!(
        diags.is_empty(),
        "expected no diagnostics; got {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

/// The witness. `Shape` is the declaring file's *second* declaration, so its
/// raw `SymbolId` in `dep.ts` is not the one `main.ts` minted for the import
/// alias. tsc:
///
/// ```text
/// main.ts(2,7): error TS2741: Property 'away' is missing in type
/// '{ held: number; }' but required in type 'Shape'.
/// ```
#[test]
fn imported_class_after_an_interface_resolves_to_the_class_instance() {
    let dep = format!("export interface Unused {{ keep: number; }}\n{SHAPE_CLASS}");
    assert_eq!(
        only_message(&dep, IMPORT_SHAPE_ANNOTATION, TS2741),
        "Property 'away' is missing in type '{ held: number; }' but required in type 'Shape'."
    );
}

/// The coincidence case that already worked: the imported class is the
/// declaring file's first declaration, so the colliding raw id happened to
/// name it. Same tsc output as the row above — it must stay unchanged.
#[test]
fn imported_class_as_the_first_declaration_is_unchanged() {
    assert_eq!(
        only_message(SHAPE_CLASS, IMPORT_SHAPE_ANNOTATION, TS2741),
        "Property 'away' is missing in type '{ held: number; }' but required in type 'Shape'."
    );
}

/// A preceding *class* is the other shape that drove a committing branch. tsc
/// reports the same TS2741 as every other row.
#[test]
fn imported_class_after_another_class_resolves_to_the_imported_class() {
    let dep = format!("export class Unused {{ keep: number = 0; }}\n{SHAPE_CLASS}");
    assert_eq!(
        only_message(&dep, IMPORT_SHAPE_ANNOTATION, TS2741),
        "Property 'away' is missing in type '{ held: number; }' but required in type 'Shape'."
    );
}

/// Two preceding declarations, so the collision lands on neither the imported
/// class nor the declaration immediately before it — the defect was an ordinal
/// mismatch, not an off-by-one.
#[test]
fn imported_class_after_two_interfaces_resolves_to_the_imported_class() {
    let dep = format!(
        "export interface A {{ a: number; }}\nexport interface B {{ b: number; }}\n{SHAPE_CLASS}"
    );
    assert_eq!(
        only_message(&dep, IMPORT_SHAPE_ANNOTATION, TS2741),
        "Property 'away' is missing in type '{ held: number; }' but required in type 'Shape'."
    );
}

/// Binder names must not matter: the same shape under different identifiers
/// resolves through the imported name, not a name the fix could have keyed on.
/// tsc:
///
/// ```text
/// main.ts(2,7): error TS2741: Property 'omega' is missing in type
/// '{ alpha: number; }' but required in type 'Widget'.
/// ```
#[test]
fn renamed_binders_resolve_the_imported_class_the_same_way() {
    let dep = "export interface Zed { keep: number; }\nexport class Widget { alpha: number = 0; omega: number = 0; }\n";
    let main = "import { Widget } from \"./dep\";\nconst w: Widget = { alpha: 1 };\n";
    assert_eq!(
        only_message(dep, main, TS2741),
        "Property 'omega' is missing in type '{ alpha: number; }' but required in type 'Widget'."
    );
}

/// A renamed import: the alias's local name is `S`, its module-side name is
/// `Shape`, and tsc reports the *module-side* name as the target type.
///
/// ```text
/// main.ts(2,7): error TS2741: Property 'away' is missing in type
/// '{ held: number; }' but required in type 'Shape'.
/// ```
#[test]
fn renamed_import_resolves_through_the_module_side_name() {
    let dep = format!("export interface Unused {{ keep: number; }}\n{SHAPE_CLASS}");
    let main = "import { Shape as S } from \"./dep\";\nconst s: S = { held: 1 };\n";
    assert_eq!(
        only_message(&dep, main, TS2741),
        "Property 'away' is missing in type '{ held: number; }' but required in type 'Shape'."
    );
}

/// A generic imported class after an interface. tsc:
///
/// ```text
/// main.ts(2,7): error TS2741: Property 'away' is missing in type
/// '{ held: number; }' but required in type 'Box[number]'.
/// ```
/// (rendered here with square brackets only in this comment).
#[test]
fn imported_generic_class_after_an_interface_keeps_its_type_arguments() {
    let dep = "export interface Unused { keep: number; }\nexport class Box<T> { held: T; away: T; constructor(v: T) { this.held = v; this.away = v; } }\n";
    let main = "import { Box } from \"./dep\";\nconst b: Box<number> = { held: 1 };\n";
    assert_eq!(
        only_message(dep, main, TS2741),
        "Property 'away' is missing in type '{ held: number; }' but required in type 'Box<number>'."
    );
}

/// A namespace import reaches the same declaration through a qualified name.
/// tsc reports the same TS2741, so the two spellings must agree.
#[test]
fn namespace_import_member_resolves_to_the_same_class() {
    let dep = format!("export interface Unused {{ keep: number; }}\n{SHAPE_CLASS}");
    let main = "import * as dep from \"./dep\";\nconst s: dep.Shape = { held: 1 };\n";
    assert_eq!(
        only_message(&dep, main, TS2741),
        "Property 'away' is missing in type '{ held: number; }' but required in type 'Shape'."
    );
}

/// An imported *interface* after another interface — the sibling of the class
/// witness, and the shape that already worked. It must keep working.
#[test]
fn imported_interface_after_an_interface_is_unchanged() {
    let dep = "export interface Unused { keep: number; }\nexport interface Shape { held: number; away: number; }\n";
    assert_eq!(
        only_message(dep, IMPORT_SHAPE_ANNOTATION, TS2741),
        "Property 'away' is missing in type '{ held: number; }' but required in type 'Shape'."
    );
}

/// Negative case: a complete object literal satisfies the imported class's
/// *instance* side. The defect compared its static side, where every such
/// literal is missing `prototype`, so a clean row is the load-bearing half of
/// the fix. tsc reports nothing here.
#[test]
fn a_complete_literal_satisfies_the_imported_class_instance_side() {
    let dep = format!("export interface Unused {{ keep: number; }}\n{SHAPE_CLASS}");
    assert_clean(
        &dep,
        "import { Shape } from \"./dep\";\nconst s: Shape = { held: 1, away: 2 };\n",
    );
}

/// Property access through an imported-class annotation. tsc reports nothing;
/// the members must come from the instance side.
#[test]
fn member_access_through_an_imported_class_annotation_is_clean() {
    let dep = format!("export interface Unused {{ keep: number; }}\n{SHAPE_CLASS}");
    assert_clean(
        &dep,
        "import { Shape } from \"./dep\";\nexport function f(s: Shape): number { return s.held; }\n",
    );
}

/// Heritage through an imported class after an interface: the subclass's
/// instance must still be assignable to the base annotation. tsc reports
/// nothing.
#[test]
fn heritage_through_an_imported_class_after_an_interface_is_clean() {
    let dep = format!("export interface Unused {{ keep: number; }}\n{SHAPE_CLASS}");
    assert_clean(
        &dep,
        "import { Shape } from \"./dep\";\nexport class Sub extends Shape { extra: number = 0; }\nconst x: Shape = new Sub();\n",
    );
}
