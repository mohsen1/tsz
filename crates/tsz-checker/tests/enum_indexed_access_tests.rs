//! Tests for indexed access types on enum objects (`(typeof Enum)[K]`).
//!
//! Structural rule: For any enum `E`, `(typeof E)[K]` where `K` is a member-name
//! key should resolve to the member's type, which is assignable to its base
//! primitive (`number` for numeric enums, `string` for string enums).
//!
//! Covers: numeric enums, string enums, mapped types over enums, varied
//! key variable names (K, P, X), aliased vs. inline forms, and negative cases.

use tsz_checker::test_utils::check_source_codes;

fn expect_no_error(source: &str) {
    let errors = check_source_codes(source)
        .into_iter()
        .filter(|&c| c == 2322)
        .count();
    assert_eq!(errors, 0, "Expected no TS2322 errors:\n{source}");
}

fn expect_error(source: &str) {
    let errors = check_source_codes(source)
        .into_iter()
        .filter(|&c| c == 2322)
        .count();
    assert!(errors > 0, "Expected at least one TS2322 error:\n{source}");
}

// ---------------------------------------------------------------------------
// Numeric enum — direct indexed access
// ---------------------------------------------------------------------------

#[test]
fn test_numeric_enum_indexed_access_to_number() {
    let source = r#"
enum Direction { Up = 1, Down, Left, Right }
declare const obj: typeof Direction;
const v: number = obj["Up"];
"#;
    expect_no_error(source);
}

#[test]
fn test_numeric_enum_property_access_to_number() {
    let source = r#"
enum Direction { Up = 1, Down, Left, Right }
declare const obj: typeof Direction;
const v: number = obj.Up;
"#;
    expect_no_error(source);
}

#[test]
fn test_numeric_enum_indexed_access_all_members() {
    let source = r#"
enum Color { Red = 10, Green = 20, Blue = 30 }
declare const obj: typeof Color;
const r: number = obj["Red"];
const g: number = obj["Green"];
const b: number = obj["Blue"];
"#;
    expect_no_error(source);
}

// ---------------------------------------------------------------------------
// Numeric enum — mapped type
// ---------------------------------------------------------------------------

#[test]
fn test_mapped_type_over_numeric_enum_assignable_to_number() {
    // Original repro from issue #6824.
    let source = r#"
enum Direction { Up = 1, Down, Left, Right }
type EnumObject<E> = { [K in keyof E]: E[K] };
type DirObj = EnumObject<typeof Direction>;
declare const dirObj: DirObj;
const up: number = dirObj.Up;
"#;
    expect_no_error(source);
}

#[test]
fn test_mapped_type_key_variable_names() {
    // Renaming the mapped-type iteration variable must not change behaviour.
    for var in ["K", "P", "X"] {
        let source = format!(
            "enum Size {{ Small = 1, Medium = 2, Large = 3 }}\n\
             type M<E> = {{ [{var} in keyof E]: E[{var}] }};\n\
             declare const m: M<typeof Size>;\n\
             const v: number = m.Small;\n"
        );
        expect_no_error(&source);
    }
}

// ---------------------------------------------------------------------------
// String enum — direct indexed access
// ---------------------------------------------------------------------------

#[test]
fn test_string_enum_indexed_access_to_string() {
    let source = r#"
enum Status { Active = "active", Inactive = "inactive" }
declare const obj: typeof Status;
const v: string = obj["Active"];
"#;
    expect_no_error(source);
}

#[test]
fn test_string_enum_property_access_to_string() {
    let source = r#"
enum Status { Active = "active", Inactive = "inactive" }
declare const obj: typeof Status;
const v: string = obj.Active;
"#;
    expect_no_error(source);
}

#[test]
fn test_mapped_type_over_string_enum_assignable_to_string() {
    let source = r#"
enum Direction { North = "north", South = "south", East = "east", West = "west" }
type Mapped<E> = { [K in keyof E]: E[K] };
declare const mapped: Mapped<typeof Direction>;
const v: string = mapped.North;
"#;
    expect_no_error(source);
}

// ---------------------------------------------------------------------------
// Enum with auto-incremented values
// ---------------------------------------------------------------------------

#[test]
fn test_auto_incremented_numeric_enum() {
    let source = r#"
enum Steps { First, Second, Third }
declare const obj: typeof Steps;
const a: number = obj.First;
const b: number = obj.Second;
const c: number = obj.Third;
"#;
    expect_no_error(source);
}

// ---------------------------------------------------------------------------
// Inline (non-aliased) mapped type
// ---------------------------------------------------------------------------

#[test]
fn test_inline_mapped_type_over_enum() {
    let source = r#"
enum Flags { A = 1, B = 2, C = 4 }
declare const mapped: { [K in keyof typeof Flags]: (typeof Flags)[K] };
const v: number = mapped.A;
"#;
    expect_no_error(source);
}

// ---------------------------------------------------------------------------
// Negative cases — type errors must still be reported
// ---------------------------------------------------------------------------

#[test]
fn test_numeric_enum_member_not_assignable_to_string() {
    let source = r#"
enum E { A = 1 }
declare const obj: typeof E;
const v: string = obj.A;
"#;
    expect_error(source);
}

#[test]
fn test_string_enum_member_not_assignable_to_number() {
    let source = r#"
enum E { A = "a" }
declare const obj: typeof E;
const v: number = obj.A;
"#;
    expect_error(source);
}
