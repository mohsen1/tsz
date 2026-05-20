//! Tests for enum member visibility across merged declarations

use tsz_checker::test_utils::check_source_codes as get_error_codes;

fn assert_no_enum_binding_errors(source: &str) {
    let codes = get_error_codes(source);
    for code in [2322, 2339, 2567] {
        assert!(
            !codes.contains(&code),
            "Expected no TS{code} for bitwise enum binding case, got: {codes:?}\n{source}"
        );
    }
}

#[test]
fn test_merged_enum_member_visibility() {
    // Members from the first enum declaration should be visible in the second
    let codes = get_error_codes(
        r#"
enum E { a, b = a }
enum E { c = a }
"#,
    );
    assert!(
        !codes.contains(&2304),
        "Should not emit TS2304 for 'a' in merged enum, got: {codes:?}"
    );
}

#[test]
fn bitwise_shift_enum_member_binds_as_value_member() {
    assert_no_enum_binding_errors(
        r#"
enum Simple {
  A = 1 << 0
}

const member: Simple = Simple.A;
const numeric: number = Simple.A;
"#,
    );
}

#[test]
fn bitwise_flag_enum_members_preserve_property_access() {
    assert_no_enum_binding_errors(
        r#"
enum Permissions {
  None = 0,
  Read = 1 << 0,
  Write = 1 << 1,
  Execute = 1 << 2,
  All = Read | Write | Execute
}

const perms: Permissions = Permissions.Read | Permissions.Write;
"#,
    );
}

#[test]
fn bitwise_enum_initializers_allow_varied_operators() {
    assert_no_enum_binding_errors(
        r#"
enum Mask {
  A = 1,
  B = 2,
  Both = A | B,
  OnlyA = Both & A,
  WithoutB = Both ^ B,
  High = 1 << 4
}

const value: Mask = Mask.OnlyA | Mask.WithoutB | Mask.High;
"#,
    );
}

#[test]
fn test_merged_enum_export() {
    // Exported enum merging: members from prior declarations visible
    let codes = get_error_codes(
        r#"
export enum Animals { Cat = 1 }
export enum Animals { Dog = 2 }
export enum Animals { CatDog = Cat | Dog }
"#,
    );
    assert!(
        !codes.contains(&2304),
        "Should not emit TS2304 for Cat/Dog in merged exported enum, got: {codes:?}"
    );
}

#[test]
fn test_enum_iife_initializer() {
    // IIFE in enum initializer should have its scope properly bound
    let codes = get_error_codes(
        r#"
enum E {
    A = (() => {
        function localFunction() { }
        var x: { (): void; };
        x = localFunction;
        return 0;
    })()
}
"#,
    );
    assert!(
        !codes.contains(&2304),
        "Should not emit TS2304 for locals inside IIFE in enum initializer, got: {codes:?}"
    );
}

/// When an enum is used in value/expression position and it is merged with a
/// namespace, element access through a union key derived from `keyof typeof E`
/// must return the member or export type, not `any`.
///
/// Rule: the value-position object type for a merged enum+namespace must include
/// both enum member properties AND namespace-exported values.
#[test]
fn test_enum_namespace_element_access_not_any() {
    let codes = get_error_codes(
        r#"
enum Color { Red, Green, Blue }
namespace Color {
  export function parse(s: string): Color { return Color.Red; }
}
const x: Color = Color["parse" as keyof typeof Color];
export {};
"#,
    );
    assert!(
        codes.contains(&2322),
        "Should emit TS2322 when assigning namespace function via element access to enum type, got: {codes:?}"
    );
}

/// Same rule with different names (Direction / fromString) to prove the fix
/// is not keyed on the specific identifier spelling.
#[test]
fn test_enum_namespace_element_access_different_names() {
    let codes = get_error_codes(
        r#"
enum Direction { North, South, East, West }
namespace Direction {
  export function fromString(s: string): Direction { return Direction.North; }
}
const d: Direction = Direction["fromString" as keyof typeof Direction];
export {};
"#,
    );
    assert!(
        codes.contains(&2322),
        "Should emit TS2322 for Direction+fromString element access, got: {codes:?}"
    );
}

/// Property access on a namespace-exported function from a merged enum+namespace
/// must succeed (no false TS2339).
#[test]
fn test_enum_namespace_property_access_ok() {
    let codes = get_error_codes(
        r#"
enum Color { Red, Green, Blue }
namespace Color {
  export function parse(s: string): Color { return Color.Red; }
}
const f = Color.parse;
export {};
"#,
    );
    assert!(
        !codes.contains(&2339),
        "Should not emit TS2339 for Color.parse on merged enum+namespace, got: {codes:?}"
    );
}

/// `keyof typeof E` for a merged enum+namespace must include the namespace export keys.
#[test]
fn test_enum_namespace_keyof_includes_export_keys() {
    let codes = get_error_codes(
        r#"
enum Status { Active, Inactive }
namespace Status {
  export function label(s: Status): string { return ""; }
}
type K = keyof typeof Status;
const ok: K = "label";
export {};
"#,
    );
    assert!(
        !codes.contains(&2322),
        "Should not emit TS2322: 'label' must be in keyof typeof Status, got: {codes:?}"
    );
}
