//! Tests for TS1323: Dynamic import module flag validation.
//!
//! TSC emits TS1323 when `import()` is used but the `--module` flag is set to
//! a value that doesn't support dynamic imports (e.g., `es2015` or `none`).
//! Supported modules: es2020, es2022, esnext, commonjs, amd, system, umd,
//! node16, nodenext, preserve.

use tsz_binder::BinderState;
use tsz_checker::CheckerState;
use tsz_checker::context::CheckerOptions;
use tsz_common::common::ModuleKind;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn get_diagnostics_with_module_and_file(
    source: &str,
    file_name: &str,
    module: ModuleKind,
) -> Vec<(u32, String)> {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        CheckerOptions {
            module,
            ..Default::default()
        },
    );

    checker.check_source_file(root);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

fn get_diagnostics_with_module(source: &str, module: ModuleKind) -> Vec<(u32, String)> {
    get_diagnostics_with_module_and_file(source, "test.ts", module)
}

fn has_ts1323(source: &str, module: ModuleKind) -> bool {
    get_diagnostics_with_module(source, module)
        .iter()
        .any(|d| d.0 == 1323)
}

const DYNAMIC_IMPORT: &str = r#"import("./foo");"#;

#[test]
fn dynamic_import_emits_ts1323_for_es2015() {
    assert!(has_ts1323(DYNAMIC_IMPORT, ModuleKind::ES2015));
}

#[test]
fn dynamic_import_emits_ts1323_for_none() {
    assert!(has_ts1323(DYNAMIC_IMPORT, ModuleKind::None));
}

#[test]
fn dynamic_import_no_ts1323_for_es2020() {
    assert!(!has_ts1323(DYNAMIC_IMPORT, ModuleKind::ES2020));
}

#[test]
fn dynamic_import_no_ts1323_for_esnext() {
    assert!(!has_ts1323(DYNAMIC_IMPORT, ModuleKind::ESNext));
}

#[test]
fn dynamic_import_no_ts1323_for_commonjs() {
    assert!(!has_ts1323(DYNAMIC_IMPORT, ModuleKind::CommonJS));
}

#[test]
fn dynamic_import_no_ts1323_for_nodenext() {
    assert!(!has_ts1323(DYNAMIC_IMPORT, ModuleKind::NodeNext));
}

#[test]
fn message_text_matches_tsc() {
    let diags = get_diagnostics_with_module(DYNAMIC_IMPORT, ModuleKind::ES2015);
    let msg = &diags.iter().find(|d| d.0 == 1323).unwrap().1;
    assert!(
        msg.contains("'--module'"),
        "Expected '--module' in message, got: {msg}"
    );
    assert!(
        msg.contains("'es2020'"),
        "Expected 'es2020' in message, got: {msg}"
    );
}

#[test]
fn import_type_query_member_access_does_not_emit_ts1323_for_es2015() {
    let source = r#"
export declare class A {
    static foo(): void;
}

export const foo: typeof import("./a").A.foo;
"#;

    let diags = get_diagnostics_with_module_and_file(source, "index.d.ts", ModuleKind::ES2015);
    assert!(
        !diags.iter().any(|(code, _)| *code == 1323),
        "Did not expect TS1323 for typeof import(\"./a\").A.foo in a declaration file, got: {diags:?}"
    );
}
