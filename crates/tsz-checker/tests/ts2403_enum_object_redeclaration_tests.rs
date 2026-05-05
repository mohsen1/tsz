//! Regression coverage for #2811: enum object redeclarations must use the
//! TS2403 identity relation, not structural assignability. A plain object
//! literal type like `{ A: Local.A }` must not be considered the same
//! redeclaration type as `typeof Local`, even though the property types
//! are bidirectionally assignable, because `typeof Local` carries
//! additional structural signals (numeric reverse-mapping signature for
//! non-const numeric enums; the `CONST_ENUM` `ObjectFlags` for const
//! enums) that the plain object literal lacks.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

fn count_ts2403(source: &str) -> usize {
    let opts = CheckerOptions {
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    let diags = check_source(source, "test.ts", opts);
    diags.iter().filter(|d| d.code == 2403).count()
}

#[test]
fn typeof_enum_then_object_literal_redeclaration_emits_ts2403() {
    let source = r#"
enum Local { A }

var localObject: typeof Local;
var localObject: { A: Local.A };
"#;
    assert_eq!(
        count_ts2403(source),
        1,
        "Expected one TS2403 for typeof Local vs object literal redeclaration"
    );
}

#[test]
fn object_literal_then_typeof_enum_redeclaration_emits_ts2403() {
    let source = r#"
enum Local { A }

var shapeFirst: { A: Local.A };
var shapeFirst: typeof Local;
"#;
    assert_eq!(
        count_ts2403(source),
        1,
        "Expected one TS2403 when object-literal annotation precedes typeof Enum"
    );
}

#[test]
fn enum_initializer_then_object_literal_redeclaration_emits_ts2403() {
    let source = r#"
enum Local { A }

var fromInitializer = Local;
var fromInitializer: { A: Local.A };
"#;
    assert_eq!(
        count_ts2403(source),
        1,
        "Expected one TS2403 when first redeclaration is initialized to the enum value"
    );
}

#[test]
fn namespaced_enum_redeclaration_emits_ts2403() {
    let source = r#"
namespace Outer {
    export enum Nested { A }
}

var namespacedObject: typeof Outer.Nested;
var namespacedObject: { A: Outer.Nested.A };
"#;
    assert_eq!(
        count_ts2403(source),
        1,
        "Expected one TS2403 for typeof Outer.Nested vs structural object redeclaration"
    );
}

/// Sanity: a same-shape redeclaration with both annotations as `typeof Enum`
/// must STILL be allowed (no TS2403). The fix targets only the asymmetric
/// case where one side is an enum-object type and the other is a plain
/// object literal.
#[test]
fn typeof_enum_then_typeof_enum_redeclaration_no_ts2403() {
    let source = r#"
enum Local { A }

var same: typeof Local;
var same: typeof Local;
"#;
    assert_eq!(
        count_ts2403(source),
        0,
        "Expected no TS2403 when both redeclarations are typeof Enum"
    );
}

/// Sanity: ordinary assignability is unchanged — assigning a plain
/// object literal value to a `typeof Enum` variable must remain allowed.
#[test]
fn object_literal_value_assignable_to_typeof_enum_variable() {
    let source = r#"
enum Local { A }

let value: typeof Local = { A: Local.A };
"#;
    let opts = CheckerOptions {
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    let diags = check_source(source, "test.ts", opts);
    let blocker: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2322 || d.code == 2403)
        .collect();
    assert!(
        blocker.is_empty(),
        "Expected no TS2322/TS2403 for assignment of object literal to typeof Enum: {diags:#?}"
    );
}
