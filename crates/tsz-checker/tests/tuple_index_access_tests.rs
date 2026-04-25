//! Tests for tuple index access diagnostics:
//! - TS2493: Tuple out-of-bounds on single tuple types
//! - TS2339: Property does not exist on union-of-tuple types

use tsz_checker::test_utils::check_source_diagnostics;

#[test]
fn test_type_level_tuple_out_of_bounds_ts2493() {
    let diagnostics = check_source_diagnostics(
        r"
type T1 = [string, number];
type T12 = T1[2];
",
    );
    assert!(
        diagnostics.iter().any(|d| d.code == 2493),
        "Expected TS2493 for out-of-bounds tuple index access, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn test_type_level_union_tuple_out_of_bounds_ts2339() {
    let diagnostics = check_source_diagnostics(
        r"
type T2 = [boolean] | [string, number];
type T22 = T2[2];
",
    );
    assert!(
        diagnostics.iter().any(|d| d.code == 2339),
        "Expected TS2339 for out-of-bounds union tuple index access, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn test_runtime_union_tuple_out_of_bounds_ts2339() {
    let diagnostics = check_source_diagnostics(
        r"
type T2 = [boolean] | [string, number];
declare let t2: T2;
let t22 = t2[2];
",
    );
    assert!(
        diagnostics.iter().any(|d| d.code == 2339),
        "Expected TS2339 for runtime union tuple out-of-bounds access, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn test_destructuring_union_tuple_out_of_bounds_ts2339() {
    let diagnostics = check_source_diagnostics(
        r"
type T2 = [boolean] | [string, number];
declare let t2: T2;
let [d0, d1, d2] = t2;
",
    );
    assert!(
        diagnostics.iter().any(|d| d.code == 2339),
        "Expected TS2339 for destructuring union tuple out-of-bounds, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn test_union_tuple_valid_index_no_error() {
    let diagnostics = check_source_diagnostics(
        r"
type T2 = [boolean] | [string, number];
type T21 = T2[1];
",
    );
    assert!(
        diagnostics.iter().all(|d| d.code != 2339 && d.code != 2493),
        "Expected no TS2339/TS2493 for valid union tuple index, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// Regression: errorForUsingPropertyOfTypeAsType03.ts
/// `type C1 = Color` is a type alias for an enum.  Accessing a non-existent
/// property on `C1` (e.g. `C1["Red"]`) should report the error against the
/// underlying enum's nominal name (`'Color'`), not the alias (`'C1'`).
/// tsc treats type aliases for enums transparently in TS2339 messages.
#[test]
fn test_ts2339_type_alias_for_enum_displays_underlying_enum_name() {
    let diagnostics = check_source_diagnostics(
        r"
namespace Test1 {
    enum Color { Red, Green, Blue }
    type C1 = Color;
    let c3: C1['Red']['toString'];
}
",
    );
    let ts2339 = diagnostics
        .iter()
        .find(|d| d.code == 2339)
        .expect("expected TS2339 for non-existent property on alias-of-enum");
    assert!(
        ts2339.message_text.contains("on type 'Color'"),
        "TS2339 should display underlying enum name `Color`, got: {}",
        ts2339.message_text
    );
    assert!(
        !ts2339.message_text.contains("on type 'C1'"),
        "TS2339 must not display alias name `C1`, got: {}",
        ts2339.message_text
    );
}
