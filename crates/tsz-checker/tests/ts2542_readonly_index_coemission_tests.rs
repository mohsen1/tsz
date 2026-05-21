//! Tests for TS2542 (readonly index signature) co-emitting with TS2322.
//!
//! Structural rule: when a readonly-wrapped indexable type is the target of an
//! element-access assignment, TS2542 still fires for the readonly write, but the
//! element type is also checked so TS2322 co-emits for mismatched values.

use tsz_checker::test_utils::check_source_codes;

fn codes(source: &str) -> Vec<u32> {
    check_source_codes(source)
        .into_iter()
        .filter(|&code| code != 2318)
        .collect()
}

fn has_both(source: &str, a: u32, b: u32) -> bool {
    let codes = codes(source);
    codes.contains(&a) && codes.contains(&b)
}

#[test]
fn readonly_number_array_index_write_wrong_type_emits_2322_and_2542() {
    let source = r#"
declare const arr: readonly number[];
arr[0] = "hello";
"#;
    assert!(
        has_both(source, 2322, 2542),
        "readonly number[] write with wrong type must emit TS2322 and TS2542"
    );
}

#[test]
fn readonly_string_array_index_write_wrong_type_emits_2322_and_2542() {
    let source = r#"
declare const arr: readonly string[];
arr[0] = 42;
"#;
    assert!(
        has_both(source, 2322, 2542),
        "readonly string[] write with wrong type must emit TS2322 and TS2542"
    );
}

#[test]
fn readonly_array_index_write_same_type_emits_2542_not_2322() {
    let source = r#"
declare const arr: readonly number[];
arr[0] = 42;
"#;
    let codes = codes(source);
    assert!(
        codes.contains(&2542),
        "readonly array write must emit TS2542"
    );
    assert!(
        !codes.contains(&2322),
        "matching readonly array write must not emit TS2322"
    );
}

#[test]
fn mutable_array_index_write_wrong_type_emits_2322_not_2542() {
    let source = r#"
let arr: number[] = [1, 2, 3];
arr[0] = "hello";
"#;
    let codes = codes(source);
    assert!(
        codes.contains(&2322),
        "mutable array wrong-type write must emit TS2322"
    );
    assert!(
        !codes.contains(&2542),
        "mutable array write must not emit TS2542"
    );
}

#[test]
fn readonly_array_generic_index_write_wrong_type_emits_2322_and_2542() {
    let source = r#"
declare const arr: ReadonlyArray<number>;
arr[0] = "hello";
"#;
    assert!(
        has_both(source, 2322, 2542),
        "ReadonlyArray<number> wrong-type write must emit TS2322 and TS2542"
    );
}

#[test]
fn readonly_named_property_write_wrong_type_emits_2540_not_2322() {
    let source = r#"
interface R { readonly b: number; }
declare const obj: R;
obj.b = "hello";
"#;
    let codes = codes(source);
    assert!(
        codes.contains(&2540),
        "readonly named property write must emit TS2540"
    );
    assert!(
        !codes.contains(&2322),
        "readonly named property write must not co-emit TS2322"
    );
}

#[test]
fn readonly_tuple_fixed_element_write_wrong_type_emits_2540_not_2322() {
    let source = r#"
declare const t: readonly [number, string];
t[0] = "hello";
"#;
    let codes = codes(source);
    assert!(
        codes.contains(&2540),
        "readonly tuple fixed element write must emit TS2540"
    );
    assert!(
        !codes.contains(&2322),
        "readonly tuple fixed element write must not co-emit TS2322"
    );
}

#[test]
fn readonly_array_via_type_alias_emits_2322_and_2542() {
    let source = r#"
type RONumbers = readonly number[];
declare const arr: RONumbers;
arr[0] = "hello";
"#;
    assert!(
        has_both(source, 2322, 2542),
        "aliased readonly array wrong-type write must emit TS2322 and TS2542"
    );
}
