//! Regression tests for the ambient-module `export default <Identifier>` +
//! sibling namespace duplicate-identifier diagnostic
//! (`check_ambient_default_namespace_export_duplicates`).
//!
//! tsc emits TS2300 only for the *type-only namespace* + `export default`
//! shape (the original `elidedJSImport1.ts` motivator). When a sibling value
//! declaration with the same name (function / var / class) is also present in
//! the ambient module body, tsc rejects the merge via TS2395
//! ("Individual declarations in merged declaration must be all exported or
//! all local") instead and does *not* emit TS2300 at the default-export
//! identifier reference. The previous tsz behavior emitted both, producing a
//! spurious extra TS2300 at the export-default position
//! (`namespaceNotMergedWithFunctionDefaultExport.ts`).

use tsz_binder::BinderState;
use tsz_checker::CheckerState;
use tsz_checker::context::CheckerOptions;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn diagnostics(source: &str, file_name: &str) -> Vec<u32> {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let opts = CheckerOptions::default();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        opts,
    );

    checker.check_source_file(root);
    checker.ctx.diagnostics.iter().map(|d| d.code).collect()
}

/// `export function X` + `export default X` + `namespace X` inside an ambient
/// external module: tsc emits TS2395 (twice) and *not* TS2300. tsz used to
/// emit a spurious TS2300 at the `export default X` identifier reference
/// because `check_ambient_default_namespace_export_duplicates` only checked
/// for namespace + default-export pairing without considering whether a
/// sibling value declaration (the exported function) provided the value side
/// of the conflict.
#[test]
fn export_default_with_sibling_function_no_extra_ts2300() {
    let source = "declare module 'replace-in-file' {\n  export function replaceInFile(config: unknown): Promise<unknown[]>;\n  export default replaceInFile;\n\n  namespace replaceInFile {\n    export function sync(config: unknown): unknown[];\n  }\n}\n";
    let codes = diagnostics(source, "test.d.ts");
    assert!(
        !codes.contains(&2300),
        "did not expect TS2300 when an exported function provides the value side of the merge conflict; got: {codes:?}"
    );
    assert!(
        codes.contains(&2395),
        "expected TS2395 (merged-declaration export-visibility mismatch); got: {codes:?}"
    );
}

/// Type-only namespace + bare `export default` retains TS2300 (the original
/// `elidedJSImport1.ts` motivation behind this check). No sibling value
/// declaration exists, so the merge truly is symbol-duplicate territory.
#[test]
fn type_only_namespace_export_default_still_emits_ts2300() {
    let source = "declare module '@truffle/contract' {\n  namespace TruffleContract { export type Contract = {} }\n  export default TruffleContract;\n}\n";
    let codes = diagnostics(source, "test.d.ts");
    assert!(
        codes.contains(&2300),
        "expected TS2300 for type-only namespace + export default identifier; got: {codes:?}"
    );
}
