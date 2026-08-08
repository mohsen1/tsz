//! TS2403 must fire whenever the two redeclaration types are genuinely
//! distinct, even when they render to the same simple display name.
//!
//! The diagnostic reporter previously self-suppressed on
//! `prev_type_str == current_type_str`, silently dropping TS2403 for
//! same-named-but-nominally-distinct classes (`A.Foo` vs `B.Foo`, distinct
//! only by their private brands). The identity decision belongs to
//! `are_var_decl_types_compatible`; the reporter only renders. Binder names
//! are varied deliberately so no test depends on a specific identifier.
//!
//! Oracle: `compiler/propertyIdentityWithPrivacyMismatch.ts` (tsc 7.0.2 reports
//! TS2403 for both the same-name-across-modules and the renamed-class pairs).

use crate::test_utils::check_source_diagnostics;

/// TS2403 diagnostics for `source` as `(start, message)` pairs.
fn ts2403(source: &str) -> Vec<(u32, String)> {
    check_source_diagnostics(source)
        .iter()
        .filter(|d| d.code == 2403)
        .map(|d| (d.start, d.message_text.clone()))
        .collect()
}

#[test]
fn namespace_same_class_name_distinct_private_brand_emits_ts2403() {
    // Two classes with the same simple name in different namespaces are distinct
    // nominal types (distinct private brands). Both render as `Widget`, but tsc
    // still reports TS2403.
    let source = r#"
namespace Left { export class Widget { private n: number = 0; } }
namespace Right { export class Widget { private n: number = 0; } }
var w: Left.Widget;
var w: Right.Widget;
"#;
    let diags = ts2403(source);
    assert_eq!(diags.len(), 1, "Expected 1 TS2403: {diags:?}");
    assert!(
        diags[0].1.contains("'Widget'"),
        "Message should name the shared display name: {diags:?}"
    );
}

#[test]
fn namespace_same_class_name_distinct_protected_brand_emits_ts2403() {
    // Protected members carry the same nominal brand as private for identity.
    let source = r#"
namespace Alpha { export class Node { protected tag: string = ""; } }
namespace Beta { export class Node { protected tag: string = ""; } }
var t: Alpha.Node;
var t: Beta.Node;
"#;
    let diags = ts2403(source);
    assert_eq!(diags.len(), 1, "Expected 1 TS2403: {diags:?}");
}

#[test]
fn ambient_module_same_class_name_distinct_private_brand_emits_ts2403() {
    // The original witness shape: same class name across two ambient modules.
    let source = r#"
declare module "one" { export class Cell { private v: number; } }
declare module "two" { export class Cell { private v: number; } }
import a = require("one");
import b = require("two");
var c: a.Cell;
var c: b.Cell;
"#;
    let diags = ts2403(source);
    assert_eq!(diags.len(), 1, "Expected 1 TS2403: {diags:?}");
}

#[test]
fn renamed_class_distinct_private_brand_emits_ts2403() {
    // Guards the sibling private-brand fix: differently-named classes still
    // report. This path never hit the same-name suppression, but the two must
    // stay coupled so a future regression in either is caught here.
    let source = r#"
class Panel { private n: number = 0; }
class Board { private n: number = 0; }
var p: Panel;
var p: Board;
"#;
    let diags = ts2403(source);
    assert_eq!(diags.len(), 1, "Expected 1 TS2403: {diags:?}");
}

#[test]
fn namespace_same_class_same_brand_no_ts2403() {
    // Redeclaring against the SAME class is identical — must stay clean.
    let source = r#"
namespace Only { export class Gizmo { private n: number = 0; } }
var g: Only.Gizmo;
var g: Only.Gizmo;
"#;
    let diags = ts2403(source);
    assert_eq!(
        diags.len(),
        0,
        "No TS2403 for identical redeclaration: {diags:?}"
    );
}

#[test]
fn structurally_identical_interface_no_ts2403() {
    // Interfaces have no private brand; an interface and a structurally identical
    // object type are redeclaration-identical and must stay clean.
    let source = r#"
interface Shape { x: number; }
var s: Shape;
var s: { x: number };
"#;
    let diags = ts2403(source);
    assert_eq!(
        diags.len(),
        0,
        "No TS2403 for structural identity: {diags:?}"
    );
}
