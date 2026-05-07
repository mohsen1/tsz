//! Regression tests for contextual typing of arrow-function callbacks
//! assigned through an indexed access on the global `window`/`globalThis`
//! receiver where the index is a typed identifier whose type is a single
//! string literal or a union of string literals.
//!
//! ```ts
//! const actions = ['resizeTo', 'resizeBy'] as const;
//! for (const action of actions) {
//!     window[action] = (x, y) => {
//!         window[action](x, y);
//!     };
//! }
//! ```
//!
//! tsc resolves the LHS write target to an intersection of the matching
//! `Window` methods so the RHS arrow function's parameters receive a real
//! contextual function shape (`(x: number, y: number) => void`). Before the
//! fix, tsz fell through to the general indexed-access path which resolved
//! the property on the full `Window & typeof globalThis` intersection and
//! lost the callable shape, causing the callback parameters to fall back to
//! implicit-any (TS7006). Conformance test
//! `conformance/types/keyof/keyofAndIndexedAccess2.ts` triggers this exact
//! shape for the `// Repro from #32038` block.

use std::path::Path;
use std::sync::Arc;

use rustc_hash::FxHashSet;
use tsz_binder::BinderState;
use tsz_binder::lib_loader::LibFile;
use tsz_checker::context::{CheckerOptions, ScriptTarget};
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

const DOM_LIB_NAMES: &[&str] = &[
    "lib.es5.d.ts",
    "lib.es2015.iterable.d.ts",
    "lib.es2015.symbol.d.ts",
    "lib.es2015.symbol.wellknown.d.ts",
    "lib.es2015.collection.d.ts",
    "lib.es2015.core.d.ts",
    "lib.es2015.generator.d.ts",
    "lib.es2015.promise.d.ts",
    "lib.es2015.proxy.d.ts",
    "lib.es2015.reflect.d.ts",
    "lib.es2016.array.include.d.ts",
    "lib.es2017.object.d.ts",
    "lib.es2017.string.d.ts",
    "lib.es2018.regexp.d.ts",
    "lib.es2019.array.d.ts",
    "lib.es2019.object.d.ts",
    "lib.es2019.string.d.ts",
    "lib.es2020.bigint.d.ts",
    "lib.es2020.string.d.ts",
    "lib.es2020.symbol.wellknown.d.ts",
    "lib.dom.d.ts",
];

fn load_named_lib_files(lib_names: &[&str]) -> Vec<Arc<LibFile>> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let lib_roots = [
        manifest_dir.join("../../TypeScript/lib"),
        manifest_dir.join("../../TypeScript/src/lib"),
        manifest_dir.join("../../crates/tsz-core/src/lib-assets-stripped"),
        manifest_dir.join("../../crates/tsz-core/src/lib-assets"),
    ];

    let mut lib_files = Vec::new();
    let mut seen_files = FxHashSet::default();
    for file_name in lib_names {
        for root in &lib_roots {
            let lib_path = root.join(file_name);
            if lib_path.exists()
                && let Ok(content) = std::fs::read_to_string(&lib_path)
            {
                if !seen_files.insert((*file_name).to_string()) {
                    break;
                }
                lib_files.push(Arc::new(LibFile::from_source(
                    (*file_name).to_string(),
                    content,
                )));
                break;
            }
        }
    }

    lib_files
}

fn check_with_dom_libs(source: &str) -> Vec<Diagnostic> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let lib_files = load_named_lib_files(DOM_LIB_NAMES);
    if lib_files.is_empty() {
        // The lib corpus is not on disk in this environment; skip silently.
        return Vec::new();
    }

    let mut binder = BinderState::new();
    binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ESNext,
            ..CheckerOptions::default()
        },
    );

    let lib_contexts: Vec<tsz_checker::context::LibContext> = lib_files
        .iter()
        .map(|lib| tsz_checker::context::LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    checker.ctx.set_lib_contexts(lib_contexts);
    checker.ctx.set_actual_lib_file_count(lib_files.len());

    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

fn ts7006_codes(diagnostics: &[Diagnostic]) -> Vec<&Diagnostic> {
    diagnostics.iter().filter(|d| d.code == 7006).collect()
}

/// Single string-literal index variable: assigning an arrow function to
/// `window[k]` (where `k: 'resizeTo'`) must contextually type the callback
/// parameters and not emit TS7006.
#[test]
fn window_assignment_with_single_literal_index_contextually_types_callback() {
    let source = r#"
declare const k: 'resizeTo';
window[k] = (x, y) => {};
"#;
    let diagnostics = check_with_dom_libs(source);
    let ts7006 = ts7006_codes(&diagnostics);
    assert!(
        ts7006.is_empty(),
        "callback (x, y) on `window[k]` must receive a contextual function \
         shape from `Window['resizeTo']`; got TS7006: {ts7006:?}",
    );
}

/// Union of string-literal indexes: tsc resolves `window['resizeTo' | 'resizeBy']`
/// to an intersection of the matching `Window` methods. The contextual function
/// shape collapses to `(x: number, y: number) => void` (both methods share
/// that signature shape), so the callback parameters must not be implicit-any.
#[test]
fn window_assignment_with_union_literal_index_contextually_types_callback() {
    let source = r#"
declare const k: 'resizeTo' | 'resizeBy';
window[k] = (x, y) => {};
"#;
    let diagnostics = check_with_dom_libs(source);
    let ts7006 = ts7006_codes(&diagnostics);
    assert!(
        ts7006.is_empty(),
        "callback (x, y) on `window[k]` (union literal) must receive a \
         contextual function shape; got TS7006: {ts7006:?}",
    );
}

/// `for-of` loop replicates the conformance shape from
/// `keyofAndIndexedAccess2.ts` (Repro from #32038). The loop variable's
/// type is the element type of `readonly ['resizeTo', 'resizeBy']`, i.e.
/// `'resizeTo' | 'resizeBy'` — the same union covered above. Locking this
/// in so the conformance test stays green.
#[test]
fn window_assignment_inside_for_of_loop_contextually_types_callback() {
    let source = r#"
const actions = ['resizeTo', 'resizeBy'] as const;
for (const action of actions) {
    window[action] = (x, y) => {
        window[action](x, y);
    };
}
"#;
    let diagnostics = check_with_dom_libs(source);
    let ts7006 = ts7006_codes(&diagnostics);
    assert!(
        ts7006.is_empty(),
        "for-of `window[action]` callback must not emit TS7006; got {ts7006:?}",
    );
}

/// Read-context indexed access through a literal-typed variable should also
/// yield a usable callable type. Calling the result with the right arity
/// must not emit any `TS230x` / `TS2554` diagnostics.
#[test]
fn window_read_with_literal_index_yields_callable_type() {
    let source = r#"
declare const k: 'resizeTo' | 'resizeBy';
const fn = window[k];
fn(1, 2);
"#;
    let diagnostics = check_with_dom_libs(source);
    let unexpected: Vec<_> = diagnostics
        .iter()
        .filter(|d| matches!(d.code, 7006 | 2349 | 2554 | 2722))
        .collect();
    assert!(
        unexpected.is_empty(),
        "reading `window[k]` and calling the result must not emit TS7006/TS234x; got {unexpected:?}",
    );
}
