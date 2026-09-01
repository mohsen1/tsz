//! Redeclarations whose types render to the same simple name but carry distinct
//! nominal identities (a class private/protected brand or an enum declaration)
//! must still emit TS2403. The instance/enum display drops the enclosing
//! namespace, so `A.Foo`/`B.Foo` and `A.E`/`B.E` both spell `Foo`/`E`; the
//! diagnostic decision keys off the nominal handle, not the rendered text.
//!
//! Regression coverage for the false-negative where the shared-render fallback
//! in `error_subsequent_variable_declaration` silently dropped TS2403 for
//! genuinely distinct declarations that happened to print identically.

use crate::test_utils::check_source_diagnostics;

fn ts2403_count(source: &str) -> usize {
    check_source_diagnostics(source)
        .into_iter()
        .filter(|d| d.code == 2403)
        .count()
}

#[test]
fn namespaced_classes_with_private_members_same_name_emit_ts2403() {
    let source = r#"
namespace A { export class Foo { private n: number = 0; } }
namespace B { export class Foo { private n: number = 0; } }
var x: A.Foo;
var x: B.Foo;
"#;
    assert_eq!(
        ts2403_count(source),
        1,
        "expected TS2403 for A.Foo vs B.Foo"
    );
}

#[test]
fn namespaced_classes_with_protected_members_same_name_emit_ts2403() {
    let source = r#"
namespace A { export class Foo { protected n: number = 0; } }
namespace B { export class Foo { protected n: number = 0; } }
var x: A.Foo;
var x: B.Foo;
"#;
    assert_eq!(
        ts2403_count(source),
        1,
        "expected TS2403 for protected-brand A.Foo vs B.Foo"
    );
}

#[test]
fn namespaced_enums_same_name_emit_ts2403() {
    let source = r#"
namespace A { export enum E { X } }
namespace B { export enum E { X } }
var e: A.E;
var e: B.E;
"#;
    assert_eq!(ts2403_count(source), 1, "expected TS2403 for A.E vs B.E");
}

#[test]
fn same_class_redeclaration_stays_clean() {
    // Both sides resolve to the same class declaration: no nominal distinction,
    // so the shared-render fallback must still suppress.
    let source = r#"
class Foo { private n: number = 0; }
var w: Foo;
var w: Foo;
"#;
    assert_eq!(
        ts2403_count(source),
        0,
        "no TS2403 for identical class type"
    );
}

#[test]
fn namespaced_structural_interfaces_same_name_stay_clean() {
    // Interfaces are structural: identical shapes are the same type in tsc, so
    // distinct namespaces do not make them nominally distinct.
    let source = r#"
namespace A { export interface Bar { n: number; } }
namespace B { export interface Bar { n: number; } }
var z: A.Bar;
var z: B.Bar;
"#;
    assert_eq!(
        ts2403_count(source),
        0,
        "no TS2403 for structurally identical interfaces"
    );
}
