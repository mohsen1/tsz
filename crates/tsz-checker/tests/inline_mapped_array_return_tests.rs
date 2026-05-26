//! Regression tests for inline homomorphic mapped return types over arrays.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs_code_messages, load_default_lib_files};

fn relevant_strict_default_lib_diagnostics(source: &str) -> Vec<(u32, String)> {
    let lib_files = load_default_lib_files();
    check_source_with_libs_code_messages(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            no_implicit_any: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
        &lib_files,
    )
    .into_iter()
    .filter(|(code, _)| *code != 2318)
    .collect()
}

#[test]
fn inline_homomorphic_mapped_return_preserves_tuple_shape() {
    let diagnostics = relevant_strict_default_lib_diagnostics(
        r#"
declare function mapAll<Items extends readonly any[]>(items: Items): { [Pos in keyof Items]: boolean };
declare const input: [number, string];
const mapped = mapAll(input);
const bad: number = mapped;
"#,
    );

    let ts2322 = diagnostics
        .iter()
        .find(|(code, _)| *code == 2322)
        .unwrap_or_else(|| panic!("expected TS2322, got {diagnostics:#?}"));

    assert!(
        ts2322.1.contains("[boolean, boolean]"),
        "inline mapped tuple return should display as a tuple, got {diagnostics:#?}"
    );
    assert!(
        !ts2322.1.contains("concat") && !ts2322.1.contains("filter"),
        "inline mapped tuple return must not enumerate array prototype keys, got {diagnostics:#?}"
    );
}

#[test]
fn inline_homomorphic_mapped_return_preserves_array_shape() {
    let diagnostics = relevant_strict_default_lib_diagnostics(
        r#"
declare function mapAll<Items extends readonly any[]>(items: Items): { [Pos in keyof Items]: string };
declare const input: number[];
const mapped = mapAll(input);
const bad: number = mapped;
"#,
    );

    let ts2322 = diagnostics
        .iter()
        .find(|(code, _)| *code == 2322)
        .unwrap_or_else(|| panic!("expected TS2322, got {diagnostics:#?}"));

    assert!(
        ts2322.1.contains("string[]"),
        "inline mapped array return should display as an array, got {diagnostics:#?}"
    );
    assert!(
        !ts2322.1.contains("concat") && !ts2322.1.contains("filter"),
        "inline mapped array return must not enumerate array prototype keys, got {diagnostics:#?}"
    );
}
