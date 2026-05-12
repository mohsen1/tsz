//! Tests for TS1203 emission in Node module modes.
//! "Export assignment cannot be used when targeting ECMAScript modules."
//!
//! In node16/nodenext, whether a file is ESM or CJS depends on file extension
//! and the nearest package.json "type" field. TS1203 should fire for ESM files
//! even when the --module option is node16/nodenext.

use tsz_checker::context::CheckerOptions;
use tsz_common::common::ModuleKind;

fn get_codes(source: &str, module: ModuleKind, file_is_esm: Option<bool>) -> Vec<u32> {
    get_diagnostics(source, module, file_is_esm)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn get_diagnostics(
    source: &str,
    module: ModuleKind,
    file_is_esm: Option<bool>,
) -> Vec<tsz_checker::diagnostics::Diagnostic> {
    get_diagnostics_with_file_name(source, module, file_is_esm, "test.ts")
}

fn get_diagnostics_with_file_name(
    source: &str,
    module: ModuleKind,
    file_is_esm: Option<bool>,
    file_name: &str,
) -> Vec<tsz_checker::diagnostics::Diagnostic> {
    let options = CheckerOptions {
        module,
        ..CheckerOptions::default()
    };

    let mut parser =
        tsz_parser::parser::ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = tsz_solver::TypeInterner::new();
    let mut checker = tsz_checker::state::CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        options,
    );

    checker.ctx.set_lib_contexts(Vec::new());
    checker.ctx.file_is_esm = file_is_esm;
    checker.check_source_file(root);

    checker.ctx.diagnostics.clone()
}

const EXPORT_ASSIGNMENT_SRC: &str = "const a = {}; export = a;";
const NON_IDENTIFIER_EXPORT_ASSIGNMENT_SRC: &str = "const value = 1;\nexport = value + 1;\n";

#[test]
fn ts1203_emitted_for_node16_esm_file() {
    let codes = get_codes(EXPORT_ASSIGNMENT_SRC, ModuleKind::Node16, Some(true));
    assert!(
        codes.contains(&1203),
        "TS1203 should fire for export= in Node16 ESM file, got: {codes:?}"
    );
}

#[test]
fn ts1203_emitted_for_nodenext_esm_file() {
    let codes = get_codes(EXPORT_ASSIGNMENT_SRC, ModuleKind::NodeNext, Some(true));
    assert!(
        codes.contains(&1203),
        "TS1203 should fire for export= in NodeNext ESM file, got: {codes:?}"
    );
}

#[test]
fn ts1203_not_emitted_for_node16_cjs_file() {
    let codes = get_codes(EXPORT_ASSIGNMENT_SRC, ModuleKind::Node16, Some(false));
    assert!(
        !codes.contains(&1203),
        "TS1203 should NOT fire for export= in Node16 CJS file, got: {codes:?}"
    );
}

#[test]
fn ts1203_not_emitted_for_nodenext_cjs_file() {
    let codes = get_codes(EXPORT_ASSIGNMENT_SRC, ModuleKind::NodeNext, Some(false));
    assert!(
        !codes.contains(&1203),
        "TS1203 should NOT fire for export= in NodeNext CJS file, got: {codes:?}"
    );
}

#[test]
fn ts1203_not_emitted_for_node16_unknown_format() {
    // When file_is_esm is None (not determined), don't emit TS1203
    let codes = get_codes(EXPORT_ASSIGNMENT_SRC, ModuleKind::Node16, None);
    assert!(
        !codes.contains(&1203),
        "TS1203 should NOT fire when file format is unknown (None), got: {codes:?}"
    );
}

#[test]
fn ts1203_still_emitted_for_esnext() {
    // Existing behavior: TS1203 fires for pure ESM module kinds
    let codes = get_codes(EXPORT_ASSIGNMENT_SRC, ModuleKind::ESNext, None);
    assert!(
        codes.contains(&1203),
        "TS1203 should fire for ESNext module, got: {codes:?}"
    );
}

#[test]
fn export_equals_with_named_export_emits_ts2309_even_when_ts1203_fires() {
    let source = "export const named = 1;\nexport = {};\n";
    let codes = get_codes(source, ModuleKind::ESNext, None);
    assert!(
        codes.contains(&1203),
        "TS1203 should still fire for export= in ESNext, got: {codes:?}"
    );
    assert!(
        codes.contains(&2309),
        "TS2309 should fire alongside TS1203 when export= is mixed with named exports, got: {codes:?}"
    );
}

#[test]
fn export_equals_with_named_export_emits_ts2309_in_commonjs() {
    let source = "export const named = 1;\nexport = {};\n";
    let codes = get_codes(source, ModuleKind::CommonJS, None);
    assert!(
        !codes.contains(&1203),
        "TS1203 should not fire for export= in CommonJS, got: {codes:?}"
    );
    assert!(
        codes.contains(&2309),
        "TS2309 should fire when export= is mixed with named exports in CommonJS, got: {codes:?}"
    );
}

#[test]
fn export_assignment_identifier_does_not_emit_ts2686_for_umd_definition_site() {
    let source = r#"
declare namespace React {
    export interface Node {}
}
export = React;
export as namespace React;
"#;

    let diagnostics = get_diagnostics(source, ModuleKind::CommonJS, None);
    assert!(
        diagnostics.iter().all(|diag| diag.code != 2686),
        "TS2686 should not fire on `export = React` in the defining UMD file, got: {diagnostics:?}"
    );
}

#[test]
fn export_assignment_in_d_dot_directory_is_not_ambient() {
    let diagnostics = get_diagnostics_with_file_name(
        NON_IDENTIFIER_EXPORT_ASSIGNMENT_SRC,
        ModuleKind::CommonJS,
        None,
        "/tmp/pkg.d.folder/main.ts",
    );
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2714),
        "TS2714 should not fire for a normal .ts file under a .d. directory, got: {diagnostics:?}"
    );
}

#[test]
fn export_assignment_in_declaration_file_still_checks_ambient_expression() {
    let diagnostics = get_diagnostics_with_file_name(
        NON_IDENTIFIER_EXPORT_ASSIGNMENT_SRC,
        ModuleKind::CommonJS,
        None,
        "/tmp/pkg.d.folder/main.d.ts",
    );
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2714),
        "TS2714 should still fire for non-identifier export assignment expressions in .d.ts files, got: {diagnostics:?}"
    );
}
