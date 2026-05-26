//! Regression tests for generator-yield contextual typing over stable lib identity.

use crate::context::CheckerOptions;
use crate::test_utils::{check_source_with_libs, load_default_lib_files};

fn strict_codes_with_libs(source: &str) -> Vec<u32> {
    let libs = load_default_lib_files();
    check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
        &libs,
    )
    .into_iter()
    .map(|diagnostic| diagnostic.code)
    .collect()
}

#[test]
fn local_generator_alias_does_not_contextually_type_yield_operand() {
    let codes = strict_codes_with_libs(
        r#"
export {};

type Generator<Y, R, N> = { fake: Y };

function* gen(): Generator<(x: string) => void, void, unknown> {
    yield x => x.toUpperCase();
}
"#,
    );

    assert!(
        codes.contains(&7006),
        "module-local Generator alias must not provide the yield contextual type; got: {codes:?}"
    );
}

#[test]
fn local_iterator_alias_does_not_contextually_type_yield_operand() {
    let codes = strict_codes_with_libs(
        r#"
export {};

type Iterator<Y, R, N> = { fake: Y };

function* gen(): Iterator<(x: string) => void, void, unknown> {
    yield x => x.toUpperCase();
}
"#,
    );

    assert!(
        codes.contains(&7006),
        "module-local Iterator alias must not provide the yield contextual type; got: {codes:?}"
    );
}

#[test]
fn lib_generator_identity_still_contextually_types_yield_operand() {
    let codes = strict_codes_with_libs(
        r#"
function* gen(): Generator<(x: string) => void, void, unknown> {
    yield x => x.toUpperCase();
}
"#,
    );

    assert!(
        !codes.contains(&7006),
        "lib Generator identity should contextually type the yield operand; got: {codes:?}"
    );
}
