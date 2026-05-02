//! Regression tests for TS2590 ("Expression produces a union type that is too
//! complex to represent.") on array literals.
//!
//! tsc's `removeSubtypes` only increments its cost counter for
//! `StructuredOrInstantiable` source types — identity-comparable
//! primitives/literals (number/string/boolean literals, `null`, `undefined`,
//! `void`, `never`, enum members, unique symbols) short-circuit on TypeId
//! equality and never drive the cost. The tsz pre-check used to count *all*
//! deduplicated element types, so a long array of distinct number literals
//! (e.g. `[0 as 0, 1 as 1, ...]`) wrongly emitted TS2590 even though tsc
//! widens those elements to `number` without complaint.
//!
//! Source: `compiler/unionSubtypeReductionErrors.ts`. The `let a = [...]`
//! prefix is 1002 distinct number literals; tsc emits TS2590 only on the
//! later `let b = [...]` of 1002 distinct object types.
//!
//! See also `crates/tsz-checker/src/types/computation/array_literal.rs` —
//! the pre-check filters identity-comparable types before the pairwise count.

use crate::test_utils::check_source_diagnostics;

fn diag_codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

/// Build an array literal source `let <name> = [0 as 0, 1 as 1, ...];`
/// with `count` distinct number-literal elements. Each element is
/// identity-comparable, so the TS2590 pre-check must not fire.
fn many_distinct_number_literal_array(name: &str, count: usize) -> String {
    use std::fmt::Write as _;
    let mut s = String::new();
    writeln!(s, "let {name} = [").unwrap();
    for i in 0..count {
        writeln!(s, "    {i} as {i},").unwrap();
    }
    s.push_str("];\n");
    s
}

/// Build an array literal of `count` distinct object-literal elements
/// `{ value: 0 as 0 }, { value: 1 as 1 }, ...`. Object types are NOT
/// identity-comparable, so the pre-check must still fire when count is
/// large enough.
fn many_distinct_object_literal_array(name: &str, count: usize) -> String {
    use std::fmt::Write as _;
    let mut s = String::new();
    writeln!(s, "let {name} = [").unwrap();
    for i in 0..count {
        writeln!(s, "    {{ value: {i} as {i} }},").unwrap();
    }
    s.push_str("];\n");
    s
}

/// 1002 distinct number literals must NOT trigger TS2590 (they are
/// identity-comparable; tsc widens them to `number`).
#[test]
fn ts2590_not_emitted_for_long_number_literal_array() {
    let source = many_distinct_number_literal_array("arr", 1002);
    let codes = diag_codes(&source);
    assert!(
        !codes.contains(&2590),
        "TS2590 must not fire on a long array of number literals. Got: {codes:?}"
    );
}

/// Anti-hardcoding cover: same shape, different identifier name and
/// element count below tsc's widen threshold but above the previous
/// over-eager pairwise threshold.
#[test]
fn ts2590_not_emitted_for_renamed_long_number_literal_array() {
    let source = many_distinct_number_literal_array("manyLiterals", 1500);
    let codes = diag_codes(&source);
    assert!(
        !codes.contains(&2590),
        "Renamed variant: TS2590 must not fire on number-literal arrays. Got: {codes:?}"
    );
}

/// String literals are also identity-comparable; long arrays of distinct
/// string literals must not emit TS2590.
#[test]
fn ts2590_not_emitted_for_long_string_literal_array() {
    use std::fmt::Write as _;
    let mut source = String::from("let strings = [\n");
    for i in 0..1100 {
        writeln!(source, "    \"s{i}\" as \"s{i}\",").unwrap();
    }
    source.push_str("];\n");
    let codes = diag_codes(&source);
    assert!(
        !codes.contains(&2590),
        "TS2590 must not fire on a long array of string literals. Got: {codes:?}"
    );
}
