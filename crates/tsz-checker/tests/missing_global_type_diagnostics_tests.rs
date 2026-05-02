use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn check_without_lib(source: &str) -> Vec<Diagnostic> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );

    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

const MINIMAL_CORE_GLOBAL_DECLS: &[(&str, &str)] = &[
    ("Array", "interface Array<T> {}"),
    ("Boolean", "interface Boolean {}"),
    ("CallableFunction", "interface CallableFunction {}"),
    ("Function", "interface Function {}"),
    ("IArguments", "interface IArguments {}"),
    ("NewableFunction", "interface NewableFunction {}"),
    ("Number", "interface Number {}"),
    ("Object", "interface Object {}"),
    ("RegExp", "interface RegExp {}"),
    ("String", "interface String {}"),
];

fn check_without_lib_with_minimal_core_globals(source: &str) -> Vec<Diagnostic> {
    check_without_lib_with_minimal_core_globals_except(&[], source)
}

fn check_without_lib_with_minimal_core_globals_except(
    omitted: &[&str],
    source: &str,
) -> Vec<Diagnostic> {
    let mut full_source = String::new();
    for &(name, decl) in MINIMAL_CORE_GLOBAL_DECLS {
        if omitted.iter().any(|omitted_name| omitted_name == &name) {
            continue;
        }
        full_source.push_str(decl);
        full_source.push('\n');
    }
    full_source.push_str(source);
    check_without_lib(&full_source)
}

#[test]
fn document_type_reference_emits_ts2304_with_minimal_core_globals() {
    let diagnostics = check_without_lib_with_minimal_core_globals("let x: Document;");
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == 2304 && d.message_text.contains("'Document'")),
        "Expected TS2304 for Document type reference without DOM libs, got: {diagnostics:?}"
    );
}

#[test]
fn arraylike_type_reference_emits_ts2304_with_minimal_core_globals() {
    let diagnostics = check_without_lib_with_minimal_core_globals("let x: ArrayLike<number>;");
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == 2304 && d.message_text.contains("'ArrayLike'")),
        "Expected TS2304 for ArrayLike type reference without ES2015 libs, got: {diagnostics:?}"
    );
}

#[test]
fn promise_constructor_type_reference_emits_ts2304_with_minimal_core_globals() {
    let diagnostics = check_without_lib_with_minimal_core_globals("let x: PromiseConstructor;");
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == 2304 && d.message_text.contains("'PromiseConstructor'")),
        "Expected TS2304 for PromiseConstructor type reference without ES2015 libs, got: {diagnostics:?}"
    );
}

#[test]
fn promise_type_reference_emits_ts2583_with_minimal_core_globals() {
    let diagnostics = check_without_lib_with_minimal_core_globals("let x: Promise<number>;");
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == 2583 && d.message_text.contains("'Promise'")),
        "Expected TS2583 for Promise type reference without ES2015 libs, got: {diagnostics:?}"
    );
}

#[test]
fn reflect_type_reference_emits_ts2583_with_minimal_core_globals() {
    let diagnostics = check_without_lib_with_minimal_core_globals("let x: Reflect;");
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == 2583 && d.message_text.contains("'Reflect'")),
        "Expected TS2583 for Reflect in type position without ES2015 libs, got: {diagnostics:?}"
    );
}

#[test]
fn async_iterable_iterator_type_reference_emits_ts2583_with_minimal_core_globals() {
    let diagnostics =
        check_without_lib_with_minimal_core_globals("let x: AsyncIterableIterator<number>;");
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == 2583 && d.message_text.contains("'AsyncIterableIterator'")),
        "Expected TS2583 for AsyncIterableIterator without ES2018 libs, got: {diagnostics:?}"
    );
}

#[test]
fn regexp_type_reference_emits_ts2318_when_core_global_missing() {
    let diagnostics =
        check_without_lib_with_minimal_core_globals_except(&["RegExp"], "let x: RegExp;");
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == 2318 && d.message_text.contains("'RegExp'")),
        "Expected TS2318 for missing RegExp global type, got: {diagnostics:?}"
    );
}

#[test]
fn iarguments_type_reference_emits_ts2318_when_core_global_missing() {
    let diagnostics =
        check_without_lib_with_minimal_core_globals_except(&["IArguments"], "let x: IArguments;");
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == 2318 && d.message_text.contains("'IArguments'")),
        "Expected TS2318 for missing IArguments global type, got: {diagnostics:?}"
    );
}

#[test]
fn promise_like_type_reference_emits_ts2304_with_minimal_core_globals() {
    let diagnostics = check_without_lib_with_minimal_core_globals("let x: PromiseLike<number>;");
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == 2304 && d.message_text.contains("'PromiseLike'")),
        "Expected TS2304 for PromiseLike type reference without libs, got: {diagnostics:?}"
    );
}

/// `Function.name` was added to the lib `Function` interface in
/// `lib.es2015.core.d.ts`. When no `Function` interface is registered
/// at all (no-lib bootstrap), the hardcoded `name => string` fallback
/// inside `resolve_function_property` must still fire so internal
/// callers (Solver tests that don't load lib files) keep working —
/// otherwise common idioms like `foo.name` start emitting spurious
/// TS2339 in no-lib bootstrap scenarios.
///
/// The complementary case — when the lib is loaded and the boxed
/// `Function` interface is missing `name` (e.g. `lib.es5.d.ts` only,
/// pre-es2015) — is verified by the conformance suite
/// (`compiler/modularizeLibrary_ErrorFromUsingES6FeaturesWithOnlyES5Lib.ts`,
/// among others) where the boxed lookup correctly reports the
/// property as absent and TS2339 fires.
#[test]
fn function_name_resolves_via_bootstrap_when_no_function_interface_registered() {
    let diagnostics = check_without_lib_with_minimal_core_globals_except(
        &["Function"],
        r#"
function foo() {}
foo.name;
"#,
    );
    let ts2339_for_name: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == 2339 && d.message_text.contains("'name'"))
        .collect();
    assert!(
        ts2339_for_name.is_empty(),
        "Expected `Function.name` to resolve via the bootstrap fallback when no \
         Function interface is registered (no-lib path). Got: {diagnostics:?}"
    );
}
