//! Manual tests for enum nominal assignability rules.
//!
//! Tests that enum members are not assignable to different enum members,
//! even when the values are the same. This validates TypeScript's nominal
//! typing for enums.

use tsz_checker::test_utils::{
    check_source_code_messages as collect_diagnostics, check_source_diagnostics,
};

fn test_enum_assignability(source: &str, expected_errors: usize) {
    let diagnostics = check_source_diagnostics(source);
    let error_count = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        error_count, expected_errors,
        "Expected {expected_errors} TS2322 errors, got {error_count}: {diagnostics:?}",
    );
}

#[test]
fn test_enum_member_to_same_member() {
    // E.A should be assignable to E.A
    let source = r"
enum E { A = 0, B = 1 }
const x: E.A = E.A;  // OK - same member
";
    test_enum_assignability(source, 0);
}

#[test]
fn test_enum_member_to_different_member() {
    // E.A should NOT be assignable to E.B
    let source = r"
enum E { A = 0, B = 1 }
const x: E.B = E.A;  // ERROR: different member
";
    test_enum_assignability(source, 1);
}

#[test]
fn test_enum_member_to_whole_enum() {
    // E.A should be assignable to E (whole enum)
    let source = r"
enum E { A = 0, B = 1 }
const x: E = E.A;  // OK - member to whole enum
";
    test_enum_assignability(source, 0);
}

#[test]
fn test_whole_enum_to_member() {
    // E (whole enum) should NOT be assignable to E.A
    let source = r"
enum E { A = 0, B = 1 }
const x: E.A = E;  // ERROR: whole enum to member
";
    test_enum_assignability(source, 1);
}

#[test]
fn test_different_enums_same_values() {
    // E.A should NOT be assignable to F.A, even if both are 0
    let source = r"
enum E { A = 0 }
enum F { A = 0 }
const x: F.A = E.A;  // ERROR: different enums
";
    test_enum_assignability(source, 1);
}

#[test]
fn test_numeric_enum_to_number() {
    // Numeric enum member should be assignable to number
    let source = r"
enum E { A = 0 }
const x: number = E.A;  // OK - numeric enum to number
";
    test_enum_assignability(source, 0);
}

#[test]
fn test_number_to_numeric_enum_member() {
    // number should NOT be assignable to numeric enum member
    let source = r"
enum E { A = 0 }
const x: E.A = 1;  // ERROR: number to enum member
";
    test_enum_assignability(source, 1);
}

#[test]
fn test_number_type_to_numeric_enum_member() {
    let source = r"
enum E { A = 0 }
declare const n: number;
const x: E.A = n;  // OK
";
    test_enum_assignability(source, 0);
}

#[test]
fn test_matching_number_literal_to_numeric_enum_member() {
    let source = r"
enum E { A = 0 }
const x: E.A = 0;  // OK
";
    test_enum_assignability(source, 0);
}

#[test]
fn test_number_to_numeric_enum_type() {
    // bare `number` type SHOULD be assignable to numeric enum type
    // but arbitrary number literals that aren't member values should error
    let source = r"
enum E { A = 0 }
declare const n: number;
const x: E = n;  // OK: number type to enum type
";
    test_enum_assignability(source, 0);
}

#[test]
fn test_typeof_enum_keyof_indexed_access_assigns_to_enum() {
    let source = r#"
enum Direction {
  Up,
  Down,
  Left,
  Right
}

type EnumValues = typeof Direction[keyof typeof Direction];
declare const ev: EnumValues;
const evCheck: Direction = ev;
"#;
    test_enum_assignability(source, 0);
}

// Note: Tests for rejecting arbitrary number literals (e.g., 999) assigned to
// numeric enums are validated via conformance tests, which have full lib types.
// The unit test checker doesn't load lib types, so enum member unions may not
// resolve correctly for rejection tests.

#[test]
fn test_number_literal_to_numeric_enum_type() {
    // Numeric enum types still reject arbitrary numeric literals.
    let source = r"
enum E { A = 0 }
const x: E = 1;  // ERROR
";
    test_enum_assignability(source, 1);
}

#[test]
fn test_negative_number_literal_to_numeric_enum_type() {
    // Negative numeric literals are not in `E { A = 0, B = 1 }` so the
    // `-1` argument must keep its literal type and be rejected against the
    // enum's structural member union (TS2322). Without contextual literal
    // preservation for enum types, `-1` would widen to `number` and the
    // open-numeric-enum rule would silently accept the assignment.
    let source = r"
enum E { A, B }
const x: E = -1;
";
    test_enum_assignability(source, 1);
}

#[test]
fn test_computed_numeric_enum_comparisons_preserve_member_overlap() {
    // Mirrors TypeScript's `equalityWithEnumTypes`: literal enum types reject
    // impossible numeric comparisons, and computed numeric enums still use
    // evaluated member values for equality-overlap diagnostics.
    let diagnostics = collect_diagnostics(
        r"
enum LiteralEnum {
    a = 1,
    b = 2,
}

enum ComputedEnum {
    a = 1 << 0,
    b = 1 << 1,
}

function f1(v: LiteralEnum) {
    if (v !== 0) { v; }
    if (v !== 1) { v; }
    if (v !== 2) { v; }
    if (v !== 3) { v; }
}

function f2(v: ComputedEnum) {
    if (v !== 0) { v; }
    if (v !== 1) { v; }
    if (v !== 2) { v; }
    if (v !== 3) { v; }
}
",
    );

    let ts2367 = diagnostics.iter().filter(|d| d.0 == 2367).count();
    assert_eq!(
        ts2367, 4,
        "Expected TS2367 for non-member comparisons in both enum forms, got: {diagnostics:?}"
    );
}

#[test]
fn test_computed_numeric_enum_members_do_not_assign_to_other_enum_unions() {
    // Mirrors TypeScript's `enumLiteralAssignableToEnumInsideUnion`: enum
    // members from a different enum can flow to another enum only through the
    // numeric-enum compatibility path that tsc accepts. A computed enum member
    // does not become assignable to an unrelated literal enum union just
    // because the computed value is numeric.
    let diagnostics = collect_diagnostics(
        r"
namespace X {
    export enum Foo {
        A, B
    }
}
namespace Y {
    export enum Foo {
        A, B
    }
}
namespace Z {
    export enum Foo {
        A = 1 << 1,
        B = 1 << 2,
    }
}
namespace Ka {
    export enum Foo {
        A = 1 << 10,
        B = 1 << 11,
    }
}
const e0: X.Foo | boolean = Y.Foo.A;
const e1: X.Foo | boolean = Z.Foo.A;
const e2: X.Foo.A | X.Foo.B | boolean = Z.Foo.A;
const e3: X.Foo.B | boolean = Z.Foo.A;
const e4: X.Foo.A | boolean = Z.Foo.A;
const e5: Ka.Foo | boolean = Z.Foo.A;
",
    );

    let ts2322 = diagnostics.iter().filter(|d| d.0 == 2322).count();
    assert_eq!(
        ts2322, 5,
        "Expected TS2322 for the computed enum member assignments, got: {diagnostics:?}"
    );
}

#[test]
fn test_string_enum_opacity() {
    // String literal should NOT be assignable to string enum
    let source = r#"
enum E { A = "a" }
const x: E = "a";  // ERROR: string literal to string enum
"#;
    test_enum_assignability(source, 1);
}

#[test]
fn test_string_enum_to_string() {
    // String enum SHOULD be assignable to string (TS behavior)
    let source = r#"
enum E { A = "a" }
const x: string = E.A;  // OK: string enum to string
"#;
    test_enum_assignability(source, 0);
}

// ── Enum instance property access tests ──

#[test]
fn test_enum_instance_tostring_no_error() {
    // Calling .toString() on an enum instance should NOT produce TS2339
    let diagnostics = collect_diagnostics(
        r"
enum Foo { X = 100 }
let x: Foo = Foo.X;
let s = x.toString();
",
    );
    let ts2339 = diagnostics.iter().filter(|d| d.0 == 2339).count();
    assert_eq!(
        ts2339, 0,
        "Expected no TS2339 for enum instance .toString(), got: {diagnostics:?}"
    );
}

#[test]
fn test_enum_instance_tofixed_no_error() {
    // Calling .toFixed() on a numeric enum instance should NOT produce TS2339
    let diagnostics = collect_diagnostics(
        r"
enum Foo { X = 100 }
let x: Foo = Foo.X;
let s = x.toFixed();
",
    );
    let ts2339 = diagnostics.iter().filter(|d| d.0 == 2339).count();
    assert_eq!(
        ts2339, 0,
        "Expected no TS2339 for enum instance .toFixed(), got: {diagnostics:?}"
    );
}

#[test]
fn test_enum_instance_valueof_no_error() {
    // Calling .valueOf() on an enum instance should NOT produce TS2339
    let diagnostics = collect_diagnostics(
        r"
enum Foo { X = 100 }
let x: Foo = Foo.X;
let n = x.valueOf();
",
    );
    let ts2339 = diagnostics.iter().filter(|d| d.0 == 2339).count();
    assert_eq!(
        ts2339, 0,
        "Expected no TS2339 for enum instance .valueOf(), got: {diagnostics:?}"
    );
}

#[test]
fn test_enum_namespace_nonexistent_property_error() {
    // Accessing a non-existent property on the enum namespace should produce TS2339
    let diagnostics = collect_diagnostics(
        r"
enum Foo { X = 100 }
let bad = Foo.nonExistent;
",
    );
    let ts2339 = diagnostics.iter().filter(|d| d.0 == 2339).count();
    assert_eq!(
        ts2339, 1,
        "Expected 1 TS2339 for Foo.nonExistent, got: {diagnostics:?}"
    );
}

#[test]
fn test_same_name_numeric_enums_are_subset_compatible() {
    let source = r"
namespace First { export enum E { a, b, c } }
namespace Second { export enum E { a, b } }
let x: First.E;
let y: Second.E;
x = y;
";
    test_enum_assignability(source, 0);
}

#[test]
fn test_same_name_ambient_numeric_enums_are_compatible() {
    let source = r"
namespace First { export enum E { a, b, c } }
declare namespace Second { export enum E { a, b, c } }
let x: First.E;
let y: Second.E;
x = y;
y = x;
";
    test_enum_assignability(source, 0);
}

#[test]
fn test_const_enums_do_not_gain_same_name_compatibility() {
    let source = r"
namespace First { export enum E { a, b, c } }
namespace Second { export const enum E { a, b, c } }
let x: First.E;
let y: Second.E;
x = y;
";
    test_enum_assignability(source, 1);
}

// Regression: numeric enum literal initializers must keep enum identity so
// cross-enum assignments still emit TS2322. Issue #3659.
#[test]
fn test_let_with_numeric_literal_initializer_preserves_cross_enum_check() {
    let source = r"
enum A { X, Y }
enum B { X, Y }
let a: A = 1;
let b: B = a;
";
    test_enum_assignability(source, 1);
}

#[test]
fn test_const_with_numeric_literal_initializer_preserves_cross_enum_check() {
    let source = r"
enum A { X, Y }
enum B { X, Y }
const a: A = 1;
let b: B = a;
";
    test_enum_assignability(source, 1);
}

#[test]
fn test_let_enum_literal_initializer_still_assignable_to_number() {
    // Sanity: keeping enum identity must not break the numeric-enum -> number
    // structural assignability rule. `let n: number = a` stays valid.
    let source = r"
enum A { X, Y }
let a: A = 1;
let n: number = a;
";
    test_enum_assignability(source, 0);
}

#[test]
fn test_let_with_member_initializer_preserves_member_identity() {
    // `let a: A = A.Y` should still narrow `a` to `A.Y` so a same-enum member
    // assignment to a different member fails (TS2322).
    let source = r"
enum A { X, Y }
let a: A = A.Y;
let other: A.X = a;
";
    test_enum_assignability(source, 1);
}

#[test]
fn test_numeric_enum_reverse_lookup_no_error() {
    let diagnostics = collect_diagnostics(
        r"
enum Direction { Up = 0, Down = 1 }
const up = Direction[0];
const down = Direction[1];
",
    );
    assert!(
        diagnostics.is_empty(),
        "Expected no errors for numeric enum reverse lookup, got: {diagnostics:?}"
    );
}

#[test]
fn test_mixed_enum_numeric_reverse_lookup_no_error() {
    let diagnostics = collect_diagnostics(
        r"
enum Mixed { A = 0, B = 'B', C = 1, D = 'D' }
const v0 = Mixed[0];
const v1 = Mixed[1];
",
    );
    assert!(
        diagnostics.is_empty(),
        "Expected no errors for mixed enum numeric reverse lookup, got: {diagnostics:?}"
    );
}

#[test]
fn test_mixed_enum_string_only_access_no_error() {
    let diagnostics = collect_diagnostics(
        r"
enum Mixed { A = 0, B = 'B', C = 1, D = 'D' }
const a = Mixed['A'];
const b = Mixed['B'];
",
    );
    assert!(
        diagnostics.is_empty(),
        "Expected no errors for string-key access on mixed enum, got: {diagnostics:?}"
    );
}

#[test]
fn test_numeric_enum_reverse_lookup_with_different_iteration_var() {
    // The fix must not be tied to a specific iteration variable name — it's structural.
    // Test auto-incremented members too.
    let diagnostics = collect_diagnostics(
        r"
enum E { X, Y, Z }
const x = E[0];
const y = E[1];
const z = E[2];
",
    );
    assert!(
        diagnostics.is_empty(),
        "Expected no errors for auto-increment enum reverse lookup, got: {diagnostics:?}"
    );
}
