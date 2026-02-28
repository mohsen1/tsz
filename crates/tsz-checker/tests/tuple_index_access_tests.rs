//! Tests for tuple index access diagnostics:
//! - TS2493: Tuple out-of-bounds on single tuple types
//! - TS2339: Property does not exist on union-of-tuple types

use crate::test_utils::check_source_diagnostics;

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
