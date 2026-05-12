//! Tests for TS2737: `BigInt` literals require --target ES2020 or higher.
//!
//! When a `BigInt` literal (`1n`) appears in value position and the compilation
//! target is below ES2020, TypeScript emits TS2737. This matches tsc behavior.

use tsz_checker::context::{CheckerOptions, ScriptTarget};
use tsz_checker::test_utils::check_source;

fn codes_with_target(source: &str, target: ScriptTarget) -> Vec<u32> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            target,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|d| d.code)
    .collect()
}

#[test]
fn bigint_literal_below_es2020_emits_ts2737() {
    let codes = codes_with_target("const x = 1n;", ScriptTarget::ES2015);
    assert!(
        codes.contains(&2737),
        "ES2015 target: expected TS2737 for BigInt literal, got: {codes:?}"
    );
}

#[test]
fn bigint_literal_es2019_target_emits_ts2737() {
    let codes = codes_with_target("const x = 1n;", ScriptTarget::ES2019);
    assert!(
        codes.contains(&2737),
        "ES2019 target: expected TS2737 for BigInt literal, got: {codes:?}"
    );
}

#[test]
fn bigint_literal_es2020_target_no_ts2737() {
    let codes = codes_with_target("const x = 1n;", ScriptTarget::ES2020);
    assert!(
        !codes.contains(&2737),
        "ES2020 target: should not emit TS2737 for BigInt literal, got: {codes:?}"
    );
}

#[test]
fn bigint_literal_esnext_target_no_ts2737() {
    let codes = codes_with_target("const x = 1n;", ScriptTarget::ESNext);
    assert!(
        !codes.contains(&2737),
        "ESNext target: should not emit TS2737 for BigInt literal, got: {codes:?}"
    );
}

#[test]
fn bigint_literal_in_ambient_declaration_no_ts2737() {
    // Ambient declarations (declare const) don't emit TS2737 even with low target.
    let codes = codes_with_target("declare const x: 1n;", ScriptTarget::ES2015);
    assert!(
        !codes.contains(&2737),
        "Ambient declaration: should not emit TS2737, got: {codes:?}"
    );
}

#[test]
fn bigint_type_literal_no_ts2737() {
    // Type-position BigInt literals (`type T = 1n`) don't emit TS2737.
    // Only value-position BigInt literals emit TS2737.
    let codes = codes_with_target("type BigOne = 1n;", ScriptTarget::ES2015);
    assert!(
        !codes.contains(&2737),
        "Type position: should not emit TS2737 for BigInt literal type, got: {codes:?}"
    );
}

#[test]
fn bigint_value_after_type_alias_emits_ts2737() {
    // The value-position `1n` should emit TS2737 regardless of the type alias.
    let codes = codes_with_target(
        "type BigOne = 1n;\nconst big: BigOne = 1n;",
        ScriptTarget::ES2015,
    );
    assert!(
        codes.contains(&2737),
        "Value position: expected TS2737 for BigInt literal, got: {codes:?}"
    );
}
