use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_multi_file_with_libs, load_lib_files};
use tsz_common::common::{ModuleKind, ScriptTarget};

const GENERATOR_MODULE: &str = r#"
export function* generatorExport(): Generator<number> {
    yield 1;
    yield 2;
}
"#;

fn compile_entry_file(files: &[(&str, &str)], entry_file: &str) -> Vec<(u32, String)> {
    let libs = load_lib_files(&[
        "es5.d.ts",
        "es2015.symbol.d.ts",
        "es2015.iterable.d.ts",
        "es2015.generator.d.ts",
    ]);
    assert_eq!(libs.len(), 4, "expected all Generator dependencies to load");

    check_multi_file_with_libs(
        files,
        entry_file,
        CheckerOptions {
            module: ModuleKind::CommonJS,
            target: ScriptTarget::ES2015,
            strict: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
        &libs,
    )
    .into_iter()
    .map(|diag| (diag.code, diag.message_text))
    .collect()
}

#[test]
fn imported_generator_return_type_is_iterable_in_for_of() {
    let consumer = r#"
import { generatorExport } from "./generator";

for (const n of generatorExport()) {
    const _n: number = n;
}
"#;

    let diagnostics = compile_entry_file(
        &[
            ("generator.ts", GENERATOR_MODULE),
            ("consumer.ts", consumer),
        ],
        "consumer.ts",
    );

    assert!(
        diagnostics.is_empty(),
        "imported Generator<number> should be iterable in for-of, got: {diagnostics:#?}"
    );
}

#[test]
fn imported_generator_for_of_preserves_number_yield_type() {
    let consumer = r#"
import { generatorExport } from "./generator";

for (const n of generatorExport()) {
    const _s: string = n;
}
"#;

    let diagnostics = compile_entry_file(
        &[
            ("generator.ts", GENERATOR_MODULE),
            ("consumer.ts", consumer),
        ],
        "consumer.ts",
    );

    assert_eq!(
        diagnostics,
        vec![(
            2322,
            "Type 'number' is not assignable to type 'string'.".to_string()
        )],
        "the imported Generator element must remain number"
    );
}

#[test]
fn imported_generator_for_of_preserves_explicit_any_yield_type() {
    let generator = r#"
export function* generatorExport(): Generator<any> {
    yield 1;
}
"#;
    let consumer = r#"
import { generatorExport } from "./generator";

for (const value of generatorExport()) {
    const _s: string = value;
}
"#;

    let diagnostics = compile_entry_file(
        &[("generator.ts", generator), ("consumer.ts", consumer)],
        "consumer.ts",
    );

    assert!(
        diagnostics.is_empty(),
        "an explicit Generator<any> yield must remain any, got: {diagnostics:#?}"
    );
}

#[test]
fn imported_generator_for_of_preserves_unknown_yield_type() {
    let generator = r#"
export function* generatorExport(): Generator<unknown> {
    yield 1;
}
"#;
    let consumer = r#"
import { generatorExport } from "./generator";

for (const value of generatorExport()) {
    const _s: string = value;
}
"#;

    let diagnostics = compile_entry_file(
        &[("generator.ts", generator), ("consumer.ts", consumer)],
        "consumer.ts",
    );

    assert_eq!(
        diagnostics,
        vec![(
            2322,
            "Type 'unknown' is not assignable to type 'string'.".to_string()
        )],
        "the imported Generator element must remain unknown"
    );
}
