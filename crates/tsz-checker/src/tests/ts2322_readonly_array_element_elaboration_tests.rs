//! Regression tests for readonly array / readonly tuple element elaboration.
//!
//! Structural rule: a `readonly` array or tuple is walked **element-wise**
//! exactly like its mutable counterpart when `tsc` elaborates an assignment
//! failure. The `readonly` modifier is not itself the failure — the element (or
//! tuple position) relation is — so `readonly number[]` vs `readonly string[]`
//! must still nest `Type 'number' is not assignable to type 'string'.`, and
//! `readonly [number, ..]` vs `readonly [string, ..]` must still nest
//! `Type at position 0 in source is not compatible with type at position 0 in
//! target.`
//!
//! Before the fix the solver's failure-reason walk
//! (`SubtypeChecker::explain_failure`) only peeled mutable `Array`/`Tuple`
//! shapes, so a `ReadonlyType` wrapper dropped the nested line entirely. The
//! fix peels a single `readonly` wrapper in the array/tuple explain paths, so it
//! applies to every readonly array/tuple mismatch (TS2322 assignment and TS2345
//! argument), not the reported spelling.

use crate::test_utils::check_source_diagnostics;

/// Collect a diagnostic's full elaboration text (main message plus all
/// related-information lines, joined by newlines) for a single error of `code`.
fn elaboration(source: &str, code: u32) -> String {
    let diags = check_source_diagnostics(source);
    let matching: Vec<_> = diags.iter().filter(|d| d.code == code).collect();
    assert_eq!(
        matching.len(),
        1,
        "Expected exactly one TS{code}. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
    let mut lines = vec![matching[0].message_text.clone()];
    lines.extend(
        matching[0]
            .related_information
            .iter()
            .map(|info| info.message_text.clone()),
    );
    lines.join("\n")
}

/// A readonly array of objects recurses into the element's structural property
/// mismatch — proving the element walk is full subtype elaboration, not a leaf
/// primitive special-case.
#[test]
fn readonly_array_of_objects_recurses_into_property() {
    let text = elaboration(
        r#"
interface A { id: number }
interface B { id: string }
declare const r: readonly A[];
const r2: readonly B[] = r;
"#,
        2322,
    );
    assert!(
        text.contains("Type 'A' is not assignable to type 'B'."),
        "readonly array element must elaborate the object relation. Got: {text:?}"
    );
    assert!(
        text.contains("Types of property 'id' are incompatible."),
        "element object mismatch must reach the offending property. Got: {text:?}"
    );
    assert!(
        text.contains("Type 'number' is not assignable to type 'string'."),
        "property mismatch must reach the leaf relation. Got: {text:?}"
    );
}

/// `readonly T[]` shorthand: same element elaboration as the generic spelling,
/// proving the rule is shape-driven and not spelling-driven.
#[test]
fn readonly_array_shorthand_elaborates_element() {
    let text = elaboration(
        r#"
declare const r: readonly number[];
const r2: readonly string[] = r;
"#,
        2322,
    );
    assert!(
        text.contains("Type 'number' is not assignable to type 'string'."),
        "readonly array shorthand element mismatch must nest the element. Got: {text:?}"
    );
}

/// TS2345 (argument) path shares the same failure-reason walk, so a readonly
/// array argument mismatch must elaborate the element too.
#[test]
fn readonly_array_argument_elaborates_element() {
    let text = elaboration(
        r#"
declare function take(a: readonly string[]): void;
declare const a: readonly number[];
take(a);
"#,
        2345,
    );
    assert!(
        text.contains("Type 'number' is not assignable to type 'string'."),
        "readonly array argument mismatch must nest the element. Got: {text:?}"
    );
}

/// Mixed mutable-source / readonly-target still elaborates the element (the
/// covariant element relation, not the readonly modifier, is the failure).
#[test]
fn mutable_source_readonly_target_elaborates_element() {
    let text = elaboration(
        r#"
declare const a: number[];
const r2: readonly string[] = a;
"#,
        2322,
    );
    assert!(
        text.contains("Type 'number' is not assignable to type 'string'."),
        "mutable->readonly array mismatch must nest the element. Got: {text:?}"
    );
}

/// Nested readonly arrays peel one `readonly` level at a time, and the inner
/// readonly-array element renders parenthesized (`(readonly string[])[]`),
/// matching tsc — never `readonly string[][]`.
#[test]
fn nested_readonly_arrays_chain_and_parenthesize() {
    let text = elaboration(
        r#"
declare const r: readonly (readonly number[])[];
const r2: readonly (readonly string[])[] = r;
"#,
        2322,
    );
    assert!(
        text.contains("Type 'readonly number[]' is not assignable to type 'readonly string[]'."),
        "outer readonly-array element must elaborate the inner readonly array. Got: {text:?}"
    );
    assert!(
        text.contains("Type 'number' is not assignable to type 'string'."),
        "inner leaf element must elaborate. Got: {text:?}"
    );
    assert!(
        text.contains("(readonly string[])[]"),
        "nested readonly-array element must be parenthesized in display. Got: {text:?}"
    );
    assert!(
        !text.contains("readonly string[][]"),
        "nested readonly array must not drop its element parens. Got: {text:?}"
    );
}

/// Readonly tuples elaborate the offending position, exactly like mutable
/// tuples (`Type at position N in source ...`).
#[test]
fn readonly_tuple_elaborates_position() {
    let text = elaboration(
        r#"
declare const t: readonly [number, number];
const t2: readonly [string, string] = t;
"#,
        2322,
    );
    assert!(
        text.contains(
            "Type at position 0 in source is not compatible with type at position 0 in target."
        ),
        "readonly tuple mismatch must nest the position line. Got: {text:?}"
    );
    assert!(
        text.contains("Type 'number' is not assignable to type 'string'."),
        "readonly tuple position leaf must elaborate. Got: {text:?}"
    );
}

/// Readonly array nested inside a property keeps the property wrapper and now
/// also reaches the element leaf beneath it.
#[test]
fn readonly_array_property_elaborates_element_leaf() {
    let text = elaboration(
        r#"
interface Holder { items: readonly number[] }
declare const h: { items: readonly string[] };
const h2: Holder = h;
"#,
        2322,
    );
    assert!(
        text.contains("Types of property 'items' are incompatible."),
        "property wrapper must remain. Got: {text:?}"
    );
    assert!(
        text.contains("Type 'string' is not assignable to type 'number'."),
        "readonly array property must reach the element leaf. Got: {text:?}"
    );
}

/// Negative / fallback cover: a readonly array whose elements *do* relate must
/// stay assignable (no over-firing of the element elaboration as a rejection).
#[test]
fn readonly_array_compatible_elements_stays_assignable() {
    let diags = check_source_diagnostics(
        r#"
declare const r: readonly number[];
const r2: readonly number[] = r;
"#,
    );
    assert!(
        diags.iter().all(|d| d.code != 2322),
        "compatible readonly arrays must not produce TS2322. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}
