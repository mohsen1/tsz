//! Regression tests for value+namespace merging where the namespace body is
//! type-only (e.g. `export { TypeAlias }`).
//!
//! Background
//! ----------
//! `getModuleInstanceState` in `tsc` resolves each `export { name }` specifier
//! to determine whether the named export carries value meaning. A namespace
//! whose only runtime statements are named exports of type-only entities is
//! NOT value-instantiated, so it can merge with a `const`/`let`/`var` of the
//! same name without producing TS2451 ("Cannot redeclare block-scoped
//! variable").
//!
//! Conformance test that motivated this regression: `compiler/
//! namespacesWithTypeAliasOnlyExportsMerge.ts`. Before this fix the
//! checker conservatively treated every `NAMED_EXPORTS` clause as
//! value-instantiating and emitted a duplicate-identifier error against
//! both the const and the namespace.

use std::path::Path;
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_binder::lib_loader::LibFile;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn load_lib_files() -> Vec<Arc<LibFile>> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let lib_paths = [
        manifest_dir.join("../../TypeScript/lib/lib.es5.d.ts"),
        manifest_dir.join("../../TypeScript/lib/lib.es2015.d.ts"),
    ];
    lib_paths
        .iter()
        .filter_map(|p| {
            if p.exists() {
                let content = std::fs::read_to_string(p).ok()?;
                let name = p.file_name()?.to_string_lossy().to_string();
                Some(Arc::new(LibFile::from_source(name, content)))
            } else {
                None
            }
        })
        .collect()
}

fn check(source: &str) -> Vec<tsz_checker::diagnostics::Diagnostic> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let lib_files = load_lib_files();
    let mut binder = BinderState::new();
    if lib_files.is_empty() {
        binder.bind_source_file(parser.get_arena(), root);
    } else {
        binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);
    }
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );
    if !lib_files.is_empty() {
        let lib_contexts: Vec<_> = lib_files
            .iter()
            .map(|lib| tsz_checker::context::LibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        checker.ctx.set_lib_contexts(lib_contexts);
    }
    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

fn ts2451_diagnostics(
    diags: &[tsz_checker::diagnostics::Diagnostic],
) -> Vec<&tsz_checker::diagnostics::Diagnostic> {
    diags.iter().filter(|d| d.code == 2451).collect()
}

#[test]
fn const_merges_with_type_only_namespace() {
    // A namespace whose only body is `export { TypeAlias }` is NOT
    // value-instantiated and must not conflict with a const of the same name.
    let source = "\
        type A = number;\n\
        declare const Q: number;\n\
        declare namespace Q {\n\
        \x20   export { A };\n\
        }\n\
        declare const try1: Q.A;\n\
        export {};\n\
    ";
    let diags = check(source);
    let ts2451 = ts2451_diagnostics(&diags);
    assert!(
        ts2451.is_empty(),
        "expected no TS2451 for const+type-only-namespace merge, got: {:?}",
        ts2451
            .iter()
            .map(|d| format!("TS2451 @ {} :: {}", d.start, d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn const_merges_with_aliased_type_only_namespace() {
    // `export { A as B }` re-exports a type alias under a different name.
    // The namespace body is still type-only.
    let source = "\
        type A = number;\n\
        declare const Q3: number;\n\
        declare namespace Q3 {\n\
        \x20   export { A as B };\n\
        }\n\
        declare const try3: Q3.B;\n\
        export {};\n\
    ";
    let diags = check(source);
    let ts2451 = ts2451_diagnostics(&diags);
    assert!(
        ts2451.is_empty(),
        "expected no TS2451 for const+aliased-type-only-namespace merge, got: {ts2451:?}"
    );
}

#[test]
fn const_conflicts_with_value_exporting_namespace() {
    // Sanity: when the namespace body re-exports a value, the namespace IS
    // value-instantiated and the merge is illegal — TS2451 must still fire.
    // The exported `inner` is a value (a function declaration).
    let source = "\
        function inner() {}\n\
        declare const X: number;\n\
        namespace X {\n\
        \x20   export { inner };\n\
        }\n\
        export {};\n\
    ";
    let diags = check(source);
    let ts2451 = ts2451_diagnostics(&diags);
    assert!(
        !ts2451.is_empty(),
        "expected TS2451 when namespace re-exports a value alongside a const \
         of the same name, got: {diags:?}"
    );
}

#[test]
fn const_conflicts_with_namespace_containing_variable() {
    // Sanity: a namespace whose body declares a runtime variable IS
    // value-instantiated and must conflict with a same-name const.
    let source = "\
        declare const Y: number;\n\
        namespace Y {\n\
        \x20   export const inner = 0;\n\
        }\n\
        export {};\n\
    ";
    let diags = check(source);
    let ts2451 = ts2451_diagnostics(&diags);
    assert!(
        !ts2451.is_empty(),
        "expected TS2451 when namespace contains a runtime variable, got: {diags:?}"
    );
}
