//! Regression tests for assignability of `IteratorResult<T>` (default
//! `TReturn = any`) against `IteratorResult<T, void>`.
//!
//! See: TypeScript/tests/cases/compiler/customAsyncIterator.ts (false-
//! positive TS2416 in CLI / conformance runs).
//!
//! The conformance test asserts that
//!
//! ```ts
//! class ConstantIterator<T> implements AsyncIterator<T, void, T | undefined> {
//!     async next(value?: T): Promise<IteratorResult<T>> { ... }
//! }
//! ```
//!
//! does NOT emit TS2416. tsc accepts the override because the class
//! method's return type `Promise<IteratorResult<T, /*default*/ any>>` is
//! structurally assignable to the interface's instantiated return type
//! `Promise<IteratorResult<T, void>>` (the `value: any` member of the
//! return-result branch absorbs the `void`).
//!
//! The baseline below covers the underlying assignability invariant in
//! a unit-level shape. Reproducing the actual TS2416 emitted by CLI runs
//! requires the comprehensive default-target lib graph and the
//! implements-clause path; that is exercised as a conformance test.

use std::path::Path;
use std::sync::Arc;

use tsz_binder::{BinderState, lib_loader::LibFile};
use tsz_checker::context::CheckerOptions;
use tsz_checker::context::LibContext as CheckerLibContext;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::state::CheckerState;
use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn load_lib_files(names: &[&str]) -> Vec<Arc<LibFile>> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut loaded = Vec::new();
    for name in names {
        let candidates = [
            manifest_dir.join(format!("../../scripts/node_modules/typescript/lib/{name}")),
            manifest_dir.join(format!(
                "../../scripts/conformance/node_modules/typescript/lib/{name}"
            )),
            manifest_dir.join(format!("../../TypeScript/lib/{name}")),
        ];
        for path in candidates {
            if path.exists()
                && let Ok(content) = std::fs::read_to_string(&path)
            {
                loaded.push(Arc::new(LibFile::from_source((*name).to_string(), content)));
                break;
            }
        }
    }
    loaded
}

fn check_with_iterable_libs(source: &str) -> Vec<Diagnostic> {
    // The minimum set required to make `IteratorResult<T>`,
    // `Promise<T>`, and `AsyncIterator<T, ...>` resolvable.
    let lib_files = load_lib_files(&[
        "lib.es5.d.ts",
        "lib.es2015.core.d.ts",
        "lib.es2015.collection.d.ts",
        "lib.es2015.iterable.d.ts",
        "lib.es2015.symbol.d.ts",
        "lib.es2015.symbol.wellknown.d.ts",
        "lib.es2015.promise.d.ts",
        "lib.es2015.proxy.d.ts",
        "lib.es2015.reflect.d.ts",
        "lib.es2015.generator.d.ts",
        "lib.es2016.array.include.d.ts",
        "lib.es2018.asynciterable.d.ts",
        "lib.es2018.asyncgenerator.d.ts",
    ]);

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions {
            target: ScriptTarget::ESNext,
            module: ModuleKind::CommonJS,
            ..CheckerOptions::default()
        },
    );

    if !lib_files.is_empty() {
        let lib_contexts: Vec<_> = lib_files
            .iter()
            .map(|lib| CheckerLibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        checker.ctx.set_lib_contexts(lib_contexts);
        checker.ctx.set_actual_lib_file_count(lib_files.len());
    }

    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

fn semantic_error_codes(diagnostics: &[Diagnostic]) -> Vec<u32> {
    diagnostics
        .iter()
        .filter(|d| {
            d.category == tsz_checker::diagnostics::DiagnosticCategory::Error && d.code != 2318 // TS2318 = Cannot find global type (test infra noise)
        })
        .map(|d| d.code)
        .collect()
}

/// `IteratorResult<string>` (default `TReturn = any`) must be assignable
/// to `IteratorResult<string, void>` directly.
#[test]
fn iterator_result_default_assignable_to_iterator_result_void() {
    let diagnostics = check_with_iterable_libs(
        r#"
declare const c: IteratorResult<string>;
const x: IteratorResult<string, void> = c;
"#,
    );
    let codes = semantic_error_codes(&diagnostics);
    assert!(
        codes.is_empty(),
        "IteratorResult<string> should be assignable to IteratorResult<string, void>; got: {diagnostics:#?}"
    );
}

/// Wrapping in an object property must preserve the assignability.
#[test]
fn property_of_iterator_result_default_assignable_to_iterator_result_void() {
    let diagnostics = check_with_iterable_libs(
        r#"
declare const c: { val: IteratorResult<string> };
const x: { val: IteratorResult<string, void> } = c;
"#,
    );
    let codes = semantic_error_codes(&diagnostics);
    assert!(
        codes.is_empty(),
        "{{val: IteratorResult<string>}} should be assignable to {{val: IteratorResult<string, void>}}; got: {diagnostics:#?}"
    );
}

/// Wrapping in `Promise<...>` must preserve the assignability — this
/// is the form actually used in the lib `AsyncIterator.next` return
/// type.
#[test]
fn promise_of_iterator_result_default_assignable_to_iterator_result_void() {
    let diagnostics = check_with_iterable_libs(
        r#"
declare const c: Promise<IteratorResult<string>>;
const x: Promise<IteratorResult<string, void>> = c;
"#,
    );
    let codes = semantic_error_codes(&diagnostics);
    assert!(
        codes.is_empty(),
        "Promise<IteratorResult<string>> should be assignable to Promise<IteratorResult<string, void>>; got: {diagnostics:#?}"
    );
}
