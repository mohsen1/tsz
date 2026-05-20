//! Regression tests for self-namespace imports leaking local imported aliases.
//!
//! Issue #3585. When a module imports itself with `import * as self from
//! "./self.mjs"`, tsz used to expose the file's local imported aliases
//! (default and named imports from other modules) on the `self` namespace
//! type. tsc only exposes real exports, so accessing those imports on the
//! self-namespace must report `TS2339`.

use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_parser::parser::ParserState;
use tsz_solver::construction::TypeInterner;

/// Bind `lib_*.mts` and `main.mts` together, with module specifiers wired so
/// `./lib.mjs` resolves to lib.mts and `./main.mjs` resolves to main.mts.
/// Returns the diagnostics produced when checking `main.mts`.
fn diagnostics_for_self_namespace(lib_source: &str, main_source: &str) -> Vec<(u32, String)> {
    let mut parser_lib = ParserState::new("lib.mts".to_string(), lib_source.to_string());
    let root_lib = parser_lib.parse_source_file();
    let mut binder_lib = BinderState::new();
    binder_lib.bind_source_file(parser_lib.get_arena(), root_lib);

    let mut parser_main = ParserState::new("main.mts".to_string(), main_source.to_string());
    let root_main = parser_main.parse_source_file();
    let mut binder_main = BinderState::new();
    binder_main.bind_source_file(parser_main.get_arena(), root_main);

    let arena_lib = Arc::new(parser_lib.get_arena().clone());
    let arena_main = Arc::new(parser_main.get_arena().clone());
    let all_arenas = Arc::new(vec![Arc::clone(&arena_lib), Arc::clone(&arena_main)]);

    let binder_lib = Arc::new(binder_lib);
    let binder_main = Arc::new(binder_main);
    let all_binders = Arc::new(vec![Arc::clone(&binder_lib), Arc::clone(&binder_main)]);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena_main.as_ref(),
        binder_main.as_ref(),
        &types,
        "main.mts".to_string(),
        CheckerOptions {
            no_lib: true,
            target: ScriptTarget::ES2022,
            module: ModuleKind::Node18,
            ..Default::default()
        },
    );

    checker.ctx.set_all_arenas(all_arenas);
    checker.ctx.set_all_binders(all_binders);
    checker.ctx.set_current_file_idx(1);

    let mut resolved_module_paths: FxHashMap<(usize, String), usize> = FxHashMap::default();
    resolved_module_paths.insert((1, "./lib.mjs".to_string()), 0);
    resolved_module_paths.insert((1, "./main.mjs".to_string()), 1);
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));

    let mut resolved_modules: FxHashSet<String> = FxHashSet::default();
    resolved_modules.insert("./lib.mjs".to_string());
    resolved_modules.insert("./main.mjs".to_string());
    checker.ctx.set_resolved_modules(resolved_modules);

    checker.check_source_file(root_main);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

#[test]
fn self_namespace_import_default_alias_does_not_appear_on_self_surface() {
    let diagnostics = diagnostics_for_self_namespace(
        "export default 1;\n",
        r#"
import localDefault from "./lib.mjs";
import * as own from "./main.mjs";

localDefault;

own.default;
"#,
    );

    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert_eq!(
        ts2339.len(),
        1,
        "expected exactly one TS2339 for self.default, got: {diagnostics:#?}"
    );
    assert!(
        ts2339[0].1.contains("'default'"),
        "expected TS2339 to mention 'default', got: {:?}",
        ts2339[0].1
    );
}

#[test]
fn self_namespace_import_named_alias_does_not_appear_on_self_surface() {
    let diagnostics = diagnostics_for_self_namespace(
        "export const imported = 1;\n",
        r#"
import { imported } from "./lib.mjs";
import * as own from "./main.mjs";

imported;

own.imported;
"#,
    );

    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert_eq!(
        ts2339.len(),
        1,
        "expected exactly one TS2339 for self.imported, got: {diagnostics:#?}"
    );
    assert!(
        ts2339[0].1.contains("'imported'"),
        "expected TS2339 to mention 'imported', got: {:?}",
        ts2339[0].1
    );
}

#[test]
fn self_namespace_import_real_export_remains_visible() {
    let diagnostics = diagnostics_for_self_namespace(
        "export const ignored = 1;\n",
        r#"
import * as own from "./main.mjs";

export const realExport = 1;

own.realExport;
"#,
    );

    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert!(
        ts2339.is_empty(),
        "real exports must remain visible on the self namespace, got: {diagnostics:#?}"
    );
}

#[test]
fn self_namespace_import_renamed_alias_does_not_appear_under_local_or_original_name() {
    let diagnostics = diagnostics_for_self_namespace(
        "export const imported = 1;\n",
        r#"
import { imported as renamed } from "./lib.mjs";
import * as own from "./main.mjs";

renamed;

own.renamed;
own.imported;
"#,
    );

    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert_eq!(
        ts2339.len(),
        2,
        "expected TS2339 for both self.renamed and self.imported, got: {diagnostics:#?}"
    );
}
