//! Regression tests for the ambient-module `export default <Identifier>` +
//! sibling declaration duplicate-identifier diagnostic.
//!
//! `declare module "x" { <decl> V; export default V; }` — a declaration and
//! its own default-export identifier reference in the same ambient module
//! block — is not a duplicate-identifier conflict in tsc, whether `V` is a
//! namespace (`elidedJSImport1.ts`), a value (`impliedNodeFormatInterop1.ts`),
//! or anything else. An ordinary consumer `import V from "x"` elsewhere does
//! not collide with it either. tsz previously emitted a spurious TS2300 for
//! both shapes; see the corpus fixtures above (verified against the pinned
//! `typescript@7.0.2` oracle) and issue #16222.

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
/// external module: tsc emits TS2395 (twice) and *not* TS2300, because the
/// exported function and the namespace fail to merge on export-visibility
/// grounds (`namespaceNotMergedWithFunctionDefaultExport.ts`), not because
/// `X` is duplicated.
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

/// A type-only namespace referenced by a bare `export default` in the same
/// ambient module block is not a duplicate identifier
/// (`elidedJSImport1.ts`'s motivating shape).
#[test]
fn type_only_namespace_export_default_no_ts2300() {
    let source = "declare module '@truffle/contract' {\n  namespace TruffleContract { export type Contract = {} }\n  export default TruffleContract;\n}\n";
    let codes = check_source_codes_named(source, "test.d.ts");
    assert!(
        !codes.contains(&2300),
        "did not expect TS2300 for type-only namespace + export default identifier; got: {codes:?}"
    );
}

/// An ambient value default-exported under its own name, then imported
/// normally by another file, is not a duplicate-identifier conflict
/// (`impliedNodeFormatInterop1.ts`'s motivating shape).
#[test]
fn ambient_value_default_export_no_conflict_with_default_import_alias() {
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
        !diagnostics.iter().any(|(code, ..)| *code == 2300),
        "did not expect TS2300 for an ordinary ambient value default export consumed by another file's default import; got: {diagnostics:?}"
    );
}
