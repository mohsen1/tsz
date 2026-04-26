//! Regression tests for assignability of intersections that contain primitive
//! members alongside objects with literal property types.
//!
//! Conformance source: `commonTypeIntersection.ts`
//!
//! The bug: `{ __typename?: 'TypeTwo' } & string` was being treated as
//! assignable to `{ __typename?: 'TypeOne' } & string` (and to the bare
//! `{ __typename?: 'TypeOne' }`), even though the literal property values
//! are disjoint. The structural check missed the literal mismatch when the
//! source intersection had a primitive member that "satisfied" the weak
//! object via the suppressed weak-type check inside intersection iteration.
//!
//! These tests run with the real lib.es5.d.ts loaded so the boxed `String`
//! interface is registered (as in the conformance environment), and they
//! pin down the expected TS2322 emissions.

use std::path::Path;
use std::sync::Arc;
use tsz_binder::state::LibContext as BinderLibContext;
use tsz_binder::{BinderState, lib_loader::LibFile};
use tsz_checker::context::CheckerOptions;
use tsz_checker::context::LibContext as CheckerLibContext;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn load_lib_file(name: &str) -> Option<Arc<LibFile>> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let candidates = [
        manifest_dir.join(format!("../../TypeScript/lib/{name}")),
        manifest_dir.join(format!(
            "scripts/conformance/node_modules/typescript/lib/{name}"
        )),
        manifest_dir.join(format!(
            "../scripts/conformance/node_modules/typescript/lib/{name}"
        )),
        manifest_dir.join(format!(
            "../../scripts/conformance/node_modules/typescript/lib/{name}"
        )),
    ];
    for candidate in &candidates {
        if candidate.exists()
            && let Ok(content) = std::fs::read_to_string(candidate)
        {
            let file_name = candidate.file_name().unwrap().to_string_lossy().to_string();
            return Some(Arc::new(LibFile::from_source(file_name, content)));
        }
    }
    None
}

fn load_es5_lib_files() -> Vec<Arc<LibFile>> {
    [
        "lib.es5.d.ts",
        // Match the conformance es2015 lib chain so `String` is augmented with
        // its iterable members and `Iterable<T>`/`IteratorResult<T>` are visible.
        "lib.es2015.core.d.ts",
        "lib.es2015.collection.d.ts",
        "lib.es2015.generator.d.ts",
        "lib.es2015.iterable.d.ts",
        "lib.es2015.promise.d.ts",
        "lib.es2015.proxy.d.ts",
        "lib.es2015.reflect.d.ts",
        "lib.es2015.symbol.d.ts",
        "lib.es2015.symbol.wellknown.d.ts",
    ]
    .into_iter()
    .filter_map(load_lib_file)
    .collect()
}

/// Run the checker with lib.es5.d.ts loaded and the given `strict` setting.
///
/// The conformance harness leaves `strict` absent (tsc default = false) when
/// the test source has no `@strict` pragma, so reproducing the bug requires
/// `strict: false` (which disables `strict_null_checks`). The default
/// `CheckerOptions::default()` has `strict: true`, which is too lenient for
/// reproducing the conformance failure.
fn check_with_lib_strict(source: &str, strict: bool) -> Vec<Diagnostic> {
    let lib_files = load_es5_lib_files();
    // Tests below assert via `count_code(...)` — when lib.es5 is missing the
    // boxed `String` path won't fire and the assertion message will surface
    // the empty diagnostic vector, making the misconfiguration visible.

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    if !lib_files.is_empty() {
        let lib_contexts: Vec<_> = lib_files
            .iter()
            .map(|lib| BinderLibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        binder.merge_lib_contexts_into_binder(&lib_contexts);
    }
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut options = CheckerOptions::default();
    if !strict {
        // Match the conformance harness when no `@strict` directive is present:
        // `strict: false` disables strict_null_checks (TSC default behaviour).
        // Mirror exactly what tsz-server's `build_checker_options` does:
        // every strict-family flag falls back to `options.strict` (false here).
        options.strict = false;
        options.strict_null_checks = false;
        options.strict_function_types = false;
        options.strict_bind_call_apply = false;
        options.strict_property_initialization = false;
        options.no_implicit_any = false;
        options.no_implicit_this = false;
        options.use_unknown_in_catch_variables = false;
        options.always_strict = false;
        options.strict_builtin_iterator_return = false;
        // tsz-server defaults `module` to CommonJS when none is specified, so
        // we mirror that here to match the conformance environment.
        options.module = tsz_common::common::ModuleKind::CommonJS;
    }

    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
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
    }

    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

fn check_with_lib(source: &str) -> Vec<Diagnostic> {
    // Default to non-strict to match the conformance harness when `@strict`
    // is absent — which is the configuration where the bug originally fires.
    check_with_lib_strict(source, false)
}

fn count_code(diagnostics: &[Diagnostic], code: u32) -> usize {
    diagnostics.iter().filter(|d| d.code == code).count()
}

/// `{ __typename?: 'TypeTwo' } & { a: boolean }` is NOT assignable to
/// `{ __typename?: 'TypeOne' } & { a: boolean }` — the literal property
/// types differ. Exercises the all-object intersection path (no primitive
/// member); this should already work and serves as a control.
#[test]
fn intersection_two_objects_literal_mismatch_emits_ts2322() {
    let source = r#"
        declare let x1: { __typename?: 'TypeTwo' } & { a: boolean };
        let y1: { __typename?: 'TypeOne' } & { a: boolean } = x1;
    "#;
    let diags = check_with_lib(source);
    assert!(
        count_code(&diags, 2322) >= 1,
        "Two-object intersection with disjoint literal `__typename` must emit TS2322. \
         Diagnostics: {diags:?}"
    );
}

/// `{ __typename?: 'TypeTwo' } & string` is NOT assignable to
/// `{ __typename?: 'TypeOne' } & string` — even when both intersections
/// share a primitive member, the disjoint literal property types must
/// still trigger TS2322. This is the failing case from
/// `commonTypeIntersection.ts`.
#[test]
fn intersection_object_and_string_literal_mismatch_emits_ts2322() {
    let source = r#"
        declare let x2: { __typename?: 'TypeTwo' } & string;
        let y2: { __typename?: 'TypeOne' } & string = x2;
    "#;
    let diags = check_with_lib(source);
    assert!(
        count_code(&diags, 2322) >= 1,
        "Intersection `{{ __typename?: 'TypeTwo' }} & string` assigned to \
         `{{ __typename?: 'TypeOne' }} & string` must emit TS2322 (literal \
         property mismatch). Diagnostics: {diags:?}"
    );
}

/// Reduced form: the target is just the bare object, not an intersection.
/// `{ __typename?: 'TypeTwo' } & string` is NOT assignable to
/// `{ __typename?: 'TypeOne' }`. The primitive `string` member of the source
/// must not silence the literal property mismatch via weak-type-check
/// suppression inside intersection-member iteration.
#[test]
fn intersection_with_string_assigned_to_bare_object_emits_ts2322() {
    let source = r#"
        declare let x: { __typename?: 'TypeTwo' } & string;
        let y: { __typename?: 'TypeOne' } = x;
    "#;
    let diags = check_with_lib(source);
    assert!(
        count_code(&diags, 2322) >= 1,
        "Intersection `{{ __typename?: 'TypeTwo' }} & string` assigned to a bare \
         `{{ __typename?: 'TypeOne' }}` must emit TS2322. Diagnostics: {diags:?}"
    );
}

/// Conformance test (`commonTypeIntersection.ts`) full source: assigning
/// `{ __typename?: 'TypeTwo' } & string` to
/// `{ __typename?: 'TypeOne' } & string` must emit TS2322 even with the
/// es2015 lib chain that the conformance runner loads. This pins the bug
/// reproduced under the real conformance environment.
#[test]
fn conformance_common_type_intersection_emits_two_ts2322() {
    let lib_files = load_es5_lib_files();
    assert!(
        !lib_files.is_empty(),
        "lib.es5.d.ts must be discoverable for this test to exercise the boxed-String path"
    );
    let source = r#"
        declare let x1: { __typename?: 'TypeTwo' } & { a: boolean };
        let y1: { __typename?: 'TypeOne' } & { a: boolean} = x1;
        declare let x2: { __typename?: 'TypeTwo' } & string;
        let y2: { __typename?: 'TypeOne' } & string = x2;
    "#;
    let diags = check_with_lib(source);
    assert!(
        count_code(&diags, 2322) >= 2,
        "commonTypeIntersection.ts ({} lib files loaded): both assignments must \
         emit TS2322. Diagnostics ({} of code 2322): {diags:?}",
        lib_files.len(),
        count_code(&diags, 2322)
    );
}
