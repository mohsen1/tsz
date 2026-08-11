//! Tests for TS-syntax `type X = import("./mod").Member` references to
//! CommonJS expando class exports (`module.exports.Member = Member` /
//! `exports.Member = Member`).
//!
//! Structural rule (oracle: typescript@7.0.2): a CommonJS expando assignment
//! records no SymbolId in the binder's syntactic export tables — those only
//! track ES `export` syntax. When the target is a JS module and the expando
//! RHS is a class declaration's own identifier, the exported member carries
//! type meaning, so the import-type reference resolves to the class instance
//! type instead of reporting TS2694. A plain function expando export stays
//! value-only (TS2694), and the same-shaped assignment inside a TS file is
//! not an export at all (TS2694 stays). Mirrors the JSDoc-path fix from
//! #17167 on the TS-syntax alias resolver.

use crate::context::CheckerOptions;
use crate::state::CheckerState;
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_common::diagnostics::Diagnostic;
use tsz_parser::parser::ParserState;
use tsz_solver::construction::TypeInterner;

fn local_module_specifiers(file_name: &str) -> Vec<String> {
    let base = file_name
        .rsplit('/')
        .next()
        .unwrap_or(file_name)
        .rsplit('\\')
        .next()
        .unwrap_or(file_name);
    let mut specs = vec![format!("./{base}")];
    for suffix in [
        ".d.ts", ".d.tsx", ".d.mts", ".d.cts", ".ts", ".tsx", ".mts", ".cts", ".js", ".jsx",
        ".mjs", ".cjs",
    ] {
        if let Some(stem) = base.strip_suffix(suffix) {
            specs.push(format!("./{stem}"));
            break;
        }
    }
    specs
}

fn check_ts_consumer_with_module_source(
    module_name: &str,
    module_source: &str,
    consumer_source: &str,
) -> Vec<Diagnostic> {
    let options = CheckerOptions {
        allow_js: true,
        check_js: true,
        ..Default::default()
    };

    let mut parser_module = ParserState::new(module_name.to_string(), module_source.to_string());
    let root_module = parser_module.parse_source_file();
    let mut binder_module = BinderState::new();
    binder_module.bind_source_file(parser_module.get_arena(), root_module);

    let consumer_name = "consumer.ts";
    let mut parser_consumer =
        ParserState::new(consumer_name.to_string(), consumer_source.to_string());
    let root_consumer = parser_consumer.parse_source_file();
    let mut binder_consumer = BinderState::new();
    binder_consumer.bind_source_file(parser_consumer.get_arena(), root_consumer);

    let arena_module = Arc::new(parser_module.get_arena().clone());
    let arena_consumer = Arc::new(parser_consumer.get_arena().clone());
    let all_arenas = Arc::new(vec![Arc::clone(&arena_module), Arc::clone(&arena_consumer)]);

    let binder_module = Arc::new(binder_module);
    let binder_consumer = Arc::new(binder_consumer);
    let all_binders = Arc::new(vec![
        Arc::clone(&binder_module),
        Arc::clone(&binder_consumer),
    ]);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena_consumer.as_ref(),
        binder_consumer.as_ref(),
        &types,
        consumer_name.to_string(),
        options,
    );
    checker.ctx.set_all_arenas(all_arenas);
    checker.ctx.set_all_binders(all_binders);
    checker.ctx.set_current_file_idx(1);
    checker.ctx.set_lib_contexts(Vec::new());

    let mut resolved_module_paths: FxHashMap<(usize, String), usize> = FxHashMap::default();
    let mut resolved_modules: FxHashSet<String> = FxHashSet::default();
    for specifier in local_module_specifiers(module_name) {
        resolved_module_paths.insert((1, specifier.clone()), 0);
        resolved_modules.insert(specifier);
    }
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));
    checker.ctx.set_resolved_modules(resolved_modules);

    checker.check_source_file(root_consumer);
    checker.ctx.diagnostics
}

fn diagnostics_with_code(diagnostics: &[Diagnostic], code: u32) -> Vec<&Diagnostic> {
    diagnostics.iter().filter(|d| d.code == code).collect()
}

#[test]
fn ts_import_type_alias_resolves_module_exports_expando_class() {
    let diagnostics = check_ts_consumer_with_module_source(
        "types.js",
        r#"
class C {
    s() {}
}
module.exports.C = C
"#,
        r#"
type X = import('./types.js').C;
declare const c: X;
c.s();
"#,
    );
    assert!(
        diagnostics.is_empty(),
        "Expected no diagnostics for a `module.exports.C = C` expando class export \
         referenced as `import(...).C`, got: {diagnostics:?}"
    );
}

#[test]
fn ts_import_type_alias_resolves_bare_exports_expando_class_renamed_binder() {
    // Adjacent case: the short `exports.X = X` form (no `module.` prefix),
    // extensionless specifier, and a differently named binder — the fix must
    // not depend on the identifier `C` or the `module.exports.` spelling.
    let diagnostics = check_ts_consumer_with_module_source(
        "widget.js",
        r#"
class Widget {
    render() {}
}
exports.Widget = Widget
"#,
        r#"
type W = import('./widget').Widget;
declare const w: W;
w.render();
"#,
    );
    assert!(
        diagnostics.is_empty(),
        "Expected no diagnostics for an `exports.Widget = Widget` expando class export \
         referenced as `import(...).Widget`, got: {diagnostics:?}"
    );
}

#[test]
fn ts_import_type_alias_expando_function_export_still_reports_ts2694() {
    // Negative control: an expando export whose RHS is a plain function is
    // value-only — TS7 dropped constructor-function inference, so the bare
    // type-position reference must still report TS2694.
    let diagnostics = check_ts_consumer_with_module_source(
        "modf.js",
        r#"
function bar() {}
module.exports.bar = bar
"#,
        r#"
type X = import('./modf').bar;
"#,
    );
    assert!(
        !diagnostics_with_code(&diagnostics, 2694).is_empty(),
        "Expected TS2694 for a value-only `module.exports.bar = bar` function export, \
         got: {diagnostics:?}"
    );
}

#[test]
fn ts_import_type_alias_expando_class_in_ts_file_still_reports_ts2694() {
    // Negative control for the JS gate: the same expando-shaped assignment in
    // a TS module is not an export (tsc keeps TS2694; the assignment itself
    // only draws a "cannot find name 'module'" error in the target file), so
    // the fallback must not resolve through it.
    let diagnostics = check_ts_consumer_with_module_source(
        "types.ts",
        r#"
export {};
class C {
    s() {}
}
module.exports.C = C
"#,
        r#"
type X = import('./types').C;
"#,
    );
    assert!(
        !diagnostics_with_code(&diagnostics, 2694).is_empty(),
        "Expected TS2694 for an expando-shaped assignment inside a TS target file, \
         got: {diagnostics:?}"
    );
}
