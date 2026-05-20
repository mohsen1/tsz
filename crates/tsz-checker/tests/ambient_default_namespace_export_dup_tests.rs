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

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source_codes_named;
use tsz_common::common::{ModuleKind, ScriptTarget};

fn diagnostics_for_entry(
    files: &[(&str, &str)],
    entry_idx: usize,
    options: CheckerOptions,
) -> Vec<(u32, String, u32, String)> {
    let entry_file = files[entry_idx].0;
    tsz_checker::test_utils::check_multi_file(files, entry_file, options)
        .into_iter()
        .map(|d| (d.code, d.file, d.start, d.message_text))
        .collect()
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
    let codes = check_source_codes_named(source, "test.d.ts");
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
    let codes = check_source_codes_named(source, "test.d.ts");
    assert!(
        codes.contains(&2300),
        "expected TS2300 for type-only namespace + export default identifier; got: {codes:?}"
    );
}

#[test]
fn ambient_value_default_export_conflicts_with_same_named_default_import_alias() {
    let package_root = r#"
declare module "highlight.js" {
  export interface HighlightAPI {
    highlight(code: string): string;
  }
  const hljs: HighlightAPI;
  export default hljs;
}
"#;
    let submodule = r#"
import hljs from "highlight.js";
export default hljs;
"#;
    let diagnostics = diagnostics_for_entry(
        &[
            ("/node_modules/highlight.js/index.d.ts", package_root),
            ("/node_modules/highlight.js/lib/core.d.ts", submodule),
        ],
        0,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            module: ModuleKind::ES2020,
            es_module_interop: true,
            allow_synthetic_default_imports: true,
            no_lib: true,
            ..CheckerOptions::default()
        },
    );
    assert!(
        diagnostics
            .iter()
            .any(|(code, file, start, message)| *code == 2300
                && file == "/node_modules/highlight.js/index.d.ts"
                && *start
                    == package_root.find("export default hljs").unwrap() as u32
                        + "export default ".len() as u32
                && message == "Duplicate identifier 'hljs'."),
        "expected TS2300 for ambient value default export/default import alias conflict; got: {diagnostics:?}"
    );
}
