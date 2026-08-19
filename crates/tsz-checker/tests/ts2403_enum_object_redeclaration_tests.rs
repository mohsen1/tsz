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

#[test]
fn nested_namespaced_enum_redeclaration_no_false_ts2403() {
    let source = r#"
namespace A {
    export namespace B {
        export enum E { X }
    }
}

var v: typeof A.B.E;
var v = A.B.E;

var wrong: typeof A.B.E;
var wrong = { X: "not a number" };
"#;
    assert_eq!(
        count_ts2403(source),
        1,
        "Expected only the object-literal control redeclaration to emit TS2403"
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

/// The reported witness (#17707): `typeof E` carries the numeric
/// reverse-mapping index signature, so a structural object that *also*
/// declares `[n: number]: string` matches on the index-signature axis. The
/// remaining divergence is the property types — `number` vs the nominal enum
/// member types — which tsc's identity relation rejects even though `number`
/// is assignable to a numeric enum member. Without the nominal-enum identity
/// rule this dropped to zero diagnostics.
#[test]
fn typeof_enum_vs_number_props_with_matching_index_emits_ts2403() {
    let source = r#"
enum E1 { A, B, C }

var e = E1;
var e: {
    readonly A: number;
    readonly B: number;
    readonly C: number;
    readonly [n: number]: string;
};
var e: typeof E1;
"#;
    assert_eq!(
        count_ts2403(source),
        1,
        "Expected one TS2403: the number-typed structural shape is not identical to typeof E1"
    );
}

/// Binder-name independence: the rule keys off enum nominality, not the
/// identifier text, so a renamed enum produces the identical diagnostic count.
#[test]
fn renamed_enum_vs_number_props_with_matching_index_emits_ts2403() {
    let source = r#"
enum Palette { Red, Green, Blue }

var swatch = Palette;
var swatch: {
    readonly Red: number;
    readonly Green: number;
    readonly Blue: number;
    readonly [n: number]: string;
};
"#;
    assert_eq!(
        count_ts2403(source),
        1,
        "Expected one TS2403 regardless of the enum/variable identifiers"
    );
}

/// Positive control for the nominal rule: when the structural side types its
/// properties as the matching enum *members* (`Palette.Red`, ...) AND carries
/// the reverse-mapping index signature, it IS identical to `typeof Palette`, so
/// no TS2403. Guards against the nominal-enum rule over-rejecting.
#[test]
fn typeof_enum_vs_matching_member_props_with_index_no_ts2403() {
    let source = r#"
enum Palette { Red, Green, Blue }

var swatch = Palette;
var swatch: {
    readonly Red: Palette.Red;
    readonly Green: Palette.Green;
    readonly Blue: Palette.Blue;
    readonly [n: number]: string;
};
"#;
    assert_eq!(
        count_ts2403(source),
        0,
        "Expected no TS2403 when the structural properties are the matching enum member types"
    );
}

/// String enums have no numeric reverse-mapping signature, but the nominal
/// rule still applies: `string`-typed properties are not identical to the
/// string-enum member types.
#[test]
fn typeof_string_enum_vs_string_props_emits_ts2403() {
    let source = r#"
enum Dir { Up = "up", Down = "down" }

var d = Dir;
var d: {
    readonly Up: string;
    readonly Down: string;
};
"#;
    assert_eq!(
        count_ts2403(source),
        1,
        "Expected one TS2403: string-typed properties are not identical to string-enum members"
    );
}

/// Guard against the nominal-enum identity rule leaking out of the
/// redeclaration path into contextual/generic inference. Here `var v1: number`
/// supplies a contextual return type to the later generic call, whose best
/// common type across the `number` (from `r`) and `E1` (from `b`) candidates is
/// `number` — exactly as tsc infers. If `identity_relation` were active while
/// inference resolved those candidates, `number` and `E1` would stop being
/// mergeable and inference would settle on `E1`, producing a spurious TS2403.
/// The flag is scoped to the redeclaration identity check, so v1 stays `number`
/// and no TS2403 is reported (matches tsc on
/// `typeArgumentInferenceWithObjectLiteral.ts`).
#[test]
fn enum_candidate_generic_inference_under_number_context_no_ts2403() {
    let source = r#"
enum E1 { X }
declare function f1<T, U>(a: { w: (x: T) => U; r: () => T; }, b: T): U;
var v1: number;
var v1 = f1({ w: x => x, r: () => 0 }, E1.X);
"#;
    assert_eq!(
        count_ts2403(source),
        0,
        "Expected no TS2403: the generic call's best common type is number, matching the annotation"
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
