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

use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_checker::CheckerState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::module_resolution::build_module_resolution_maps;
use tsz_common::common::{ModuleKind, ScriptTarget};
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

fn diagnostics_for_entry(
    files: &[(&str, &str)],
    entry_idx: usize,
    options: CheckerOptions,
) -> Vec<(u32, String, u32, String)> {
    let mut arenas = Vec::with_capacity(files.len());
    let mut binders = Vec::with_capacity(files.len());
    let mut roots = Vec::with_capacity(files.len());
    let file_names: Vec<String> = files.iter().map(|(name, _)| (*name).to_string()).collect();

    for (name, source) in files {
        let mut parser = ParserState::new((*name).to_string(), (*source).to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);
        arenas.push(Arc::new(parser.get_arena().clone()));
        binders.push(Arc::new(binder));
        roots.push(root);
    }

    let (resolved_module_paths, resolved_modules) = build_module_resolution_maps(&file_names);
    let all_arenas = Arc::new(arenas);
    let all_binders = Arc::new(binders);
    let types = TypeInterner::new();

    let mut checker = CheckerState::new(
        all_arenas[entry_idx].as_ref(),
        all_binders[entry_idx].as_ref(),
        &types,
        file_names[entry_idx].clone(),
        options,
    );
    checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
    checker.ctx.set_all_binders(Arc::clone(&all_binders));
    checker.ctx.set_current_file_idx(entry_idx);
    checker.ctx.set_lib_contexts(Vec::new());
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));
    checker.ctx.set_resolved_modules(resolved_modules);

    checker.check_source_file(roots[entry_idx]);
    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.file.clone(), d.start, d.message_text.clone()))
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
