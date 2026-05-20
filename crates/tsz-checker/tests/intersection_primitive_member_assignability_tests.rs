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

use std::sync::Arc;
use tsz_binder::state::LibContext as BinderLibContext;
use tsz_binder::{BinderState, lib_loader::LibFile};
use tsz_checker::context::CheckerOptions;
use tsz_checker::context::LibContext as CheckerLibContext;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn load_es5_lib_files() -> Vec<Arc<LibFile>> {
    tsz_checker::test_utils::load_compiled_lib_files(&[
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
    ])
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

fn messages_for_code(diagnostics: &[Diagnostic], code: u32) -> Vec<&str> {
    diagnostics
        .iter()
        .filter(|diag| diag.code == code)
        .map(|diag| diag.message_text.as_str())
        .collect()
}

#[test]
fn branded_primitive_application_source_displays_structural_intersection_in_ts2739() {
    let source = r#"
        interface ViewStyle {
            view: number
            styleMedia: string
        }
        type Brand<T> = number & { __brand: T }
        declare function create<T extends { [s: string]: ViewStyle }>(styles: T): { [P in keyof T]: Brand<T[P]> };
        const wrapped = create({ first: { view: 0, styleMedia: "???" } });
        const vs: ViewStyle = wrapped.first;
    "#;
    let diags = check_with_lib(source);
    let ts2739 = diags
        .iter()
        .find(|diag| diag.code == 2739)
        .unwrap_or_else(|| panic!("expected TS2739 for branded number source, got {diags:?}"));
    assert!(
        ts2739
            .message_text
            .contains("Type 'Number & { __brand: { view: number; styleMedia: string; }; }'"),
        "TS2739 should display the evaluated branded primitive intersection, got: {}",
        ts2739.message_text
    );
    assert!(
        !ts2739.message_text.contains("Brand<"),
        "TS2739 should not preserve the generic Brand alias for branded primitive sources, got: {}",
        ts2739.message_text
    );
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
    let diags = check_with_lib_strict(source, true);
    let ts2322_messages = messages_for_code(&diags, 2322);
    assert!(
        ts2322_messages.len() >= 2,
        "commonTypeIntersection.ts ({} lib files loaded): both assignments must \
         emit TS2322. Diagnostics ({} of code 2322): {diags:?}",
        lib_files.len(),
        ts2322_messages.len()
    );
    let expected_object_intersection = "Type '{ __typename?: \"TypeTwo\" | undefined; } & { a: boolean; }' is not assignable to type '{ __typename?: \"TypeOne\" | undefined; } & { a: boolean; }'.";
    let expected_primitive_intersection = "Type '{ __typename?: \"TypeTwo\" | undefined; } & string' is not assignable to type '{ __typename?: \"TypeOne\" | undefined; } & string'.";
    assert!(
        ts2322_messages
            .iter()
            .any(|message| message.contains(expected_object_intersection)),
        "object-intersection TS2322 should use semantic declared-intersection display. \
         TS2322 messages: {ts2322_messages:?}"
    );
    assert!(
        ts2322_messages
            .iter()
            .any(|message| message.contains(expected_primitive_intersection)),
        "primitive-intersection TS2322 should preserve declared member order while \
         formatting members semantically. TS2322 messages: {ts2322_messages:?}"
    );
    assert!(
        ts2322_messages.iter().all(|message| {
            !message.contains("{ __typename?: 'TypeTwo'; }")
                && !message.contains("string & { __typename?: string | undefined; }")
        }),
        "commonTypeIntersection.ts should not leak raw single-quoted annotations or \
         widened primitive-first target displays. TS2322 messages: {ts2322_messages:?}"
    );
}
