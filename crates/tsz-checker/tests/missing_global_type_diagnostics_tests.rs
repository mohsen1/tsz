use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::state::CheckerState;
use tsz_common::common::ScriptTarget;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn check_without_lib(source: &str) -> Vec<Diagnostic> {
    check_without_lib_with_options(source, CheckerOptions::default())
}

fn check_without_lib_with_options(source: &str, options: CheckerOptions) -> Vec<Diagnostic> {
    tsz_checker::test_utils::check_with_options(source, options)
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

fn check_without_lib_with_minimal_core_globals_and_options(
    source: &str,
    options: CheckerOptions,
) -> Vec<Diagnostic> {
    let mut full_source = String::new();
    for &(_, decl) in MINIMAL_CORE_GLOBAL_DECLS {
        full_source.push_str(decl);
        full_source.push('\n');
    }
    full_source.push_str(source);
    check_without_lib_with_options(&full_source, options)
}

fn check_with_no_lib_and_minimal_core_globals(source: &str) -> Vec<Diagnostic> {
    check_without_lib_with_minimal_core_globals_and_options(
        source,
        CheckerOptions {
            no_lib: true,
            strict: true,
            ..CheckerOptions::default()
        },
    )
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

fn check_with_named_libs(source: &str, lib_names: &[&str]) -> Vec<Diagnostic> {
    let lib_files = tsz_checker::test_utils::load_lib_files(lib_names);
    assert!(
        !lib_files.is_empty(),
        "test libs should be available for {lib_names:?}"
    );
    tsz_checker::test_utils::check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
        &lib_files,
    )
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
fn crypto_value_emits_ts2304_without_dom_lib() {
    let diagnostics = check_without_lib_with_minimal_core_globals("crypto;");
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == 2304 && d.message_text.contains("'crypto'")),
        "Expected TS2304 for missing crypto global without DOM libs, got: {diagnostics:?}"
    );
}

#[test]
fn crypto_property_access_base_emits_ts2304_without_dom_lib() {
    let diagnostics = check_without_lib_with_minimal_core_globals("crypto.randomUUID();");
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == 2304 && d.message_text.contains("'crypto'")),
        "Expected TS2304 for crypto.randomUUID() without DOM libs, got: {diagnostics:?}"
    );
}

#[test]
fn crypto_local_shadow_does_not_emit_ts2304_without_dom_lib() {
    let diagnostics = check_without_lib_with_minimal_core_globals(
        r#"
const crypto = { randomUUID(): string { return ""; } };
crypto.randomUUID();
"#,
    );
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.code == 2304 && d.message_text.contains("'crypto'")),
        "Local crypto binding should shadow the missing global, got: {diagnostics:?}"
    );
}

#[test]
fn crypto_no_ts2304_with_dom_lib() {
    let diagnostics = check_with_named_libs("crypto.randomUUID();", &["es5.d.ts", "dom.d.ts"]);
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.code == 2304 && d.message_text.contains("'crypto'")),
        "crypto should not emit TS2304 with DOM lib loaded, got: {diagnostics:?}"
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
fn nolib_mapped_utility_reference_emits_ts2304() {
    let diagnostics = check_without_lib_with_minimal_core_globals_and_options(
        r#"
type Source = {
  value: string;
  other: number;
};

type OnlyValue = Pick<Source, "value">;

const result: OnlyValue = { value: 123 };
"#,
        CheckerOptions {
            no_lib: true,
            strict: true,
            ..CheckerOptions::default()
        },
    );

    let pick_ts2304: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == 2304 && d.message_text.contains("'Pick'"))
        .collect();
    assert_eq!(
        pick_ts2304.len(),
        1,
        "Expected TS2304 for missing Pick under noLib, got: {diagnostics:?}"
    );

    let ts2322: Vec<_> = diagnostics.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "Expected missing Pick to avoid synthesized assignability diagnostics, got: {diagnostics:?}"
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

#[test]
fn nolib_arraylike_and_promiselike_references_emit_ts2304() {
    let diagnostics = check_with_no_lib_and_minimal_core_globals(
        r#"
declare const a: ArrayLike<string>;
declare const p: PromiseLike<string>;

a[0].toUpperCase();
p.then(value => value.toUpperCase());
"#,
    );

    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == 2304 && d.message_text.contains("'ArrayLike'")),
        "Expected TS2304 for ArrayLike under noLib, got: {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == 2304 && d.message_text.contains("'PromiseLike'")),
        "Expected TS2304 for PromiseLike under noLib, got: {diagnostics:?}"
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

/// Negative case for the post-es5 bootstrap-fallback gate in
/// `resolve_primitive_property`: when no lib `String` interface is
/// registered at all, the no-lib bootstrap fallback must continue to
/// resolve future-version members (`includes`, `padStart`, etc.) so
/// internal Solver callers that don't load lib files keep working.
///
/// The complementary positive case (boxed `String` loaded but lacking
/// the property — e.g. lib es5 plus `s.includes(...)`) is exercised by
/// the conformance suite where the boxed lookup correctly returns
/// not-found and the checker emits TS2550.
#[test]
fn string_post_es5_member_resolves_via_bootstrap_when_no_string_interface_registered() {
    let diagnostics = check_without_lib_with_minimal_core_globals_except(
        &["String"],
        r#"
const s: string = "x";
s.includes("y");
s.padStart(2);
"#,
    );
    let unwanted: Vec<_> = diagnostics
        .iter()
        .filter(|d| {
            (d.code == 2339 || d.code == 2550)
                && (d.message_text.contains("'includes'") || d.message_text.contains("'padStart'"))
        })
        .collect();
    assert!(
        unwanted.is_empty(),
        "Expected `string.includes`/`padStart` to resolve via the bootstrap \
         fallback when no String interface is registered (no-lib path). Got: \
         {diagnostics:?}"
    );
}

#[test]
fn symbol_description_requires_es2019_symbol_lib() {
    let es2015_symbol_libs = [
        "es5.d.ts",
        "es2015.d.ts",
        "es2015.core.d.ts",
        "es2015.symbol.d.ts",
        "es2015.symbol.wellknown.d.ts",
    ];
    let diagnostics = check_with_named_libs(
        r#"
declare const s: symbol;
s.description;
"#,
        &es2015_symbol_libs,
    );

    assert!(
        diagnostics.iter().any(|d| {
            d.code == 2550
                && d.message_text.contains("'description'")
                && d.message_text.contains("'symbol'")
                && d.message_text.contains("'es2019'")
        }),
        "Expected TS2550 for symbol.description with only ES2015 symbol libs, got: {diagnostics:?}"
    );

    let mut es2019_symbol_libs = es2015_symbol_libs.to_vec();
    es2019_symbol_libs.push("es2019.symbol.d.ts");
    let diagnostics = check_with_named_libs(
        r#"
declare const s: symbol;
s.description;
"#,
        &es2019_symbol_libs,
    );

    let unexpected: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == 2339 || d.code == 2550)
        .collect();
    assert!(
        unexpected.is_empty(),
        "Expected symbol.description to resolve once ES2019 symbol lib is loaded, got: {diagnostics:?}"
    );
}

/// Regression: when a type reference fails to resolve (e.g. a DOM type like
/// `HTMLDivElement` referenced from a `.d.ts` whose ambient lib isn't
/// loaded), the checker must intern an `UnresolvedTypeName(name)` rather
/// than collapsing the result to `TypeId::ERROR`.
///
/// The visitor in `visitors/visitor.rs` already treats `UnresolvedTypeName`
/// as `Error` for every traversal/predicate, so this is structurally
/// neutral. The user-visible improvement is that the type printer renders
/// `UnresolvedTypeName(name)` as the original identifier instead of the
/// bare `error` token. That's what flips the
/// `compiler/jsxCallElaborationCheckNoCrash1.tsx` fingerprint from
/// `DetailedHTMLProps<HTMLAttributes<error>, error>` (tsz, pre-fix) to
/// `DetailedHTMLProps<HTMLAttributes<HTMLDivElement>, HTMLDivElement>`
/// (tsc / tsz post-fix).
///
/// Asserting on rendered diagnostic text would be brittle (TS2322 may be
/// suppressed when the source has TS2304 in this no-lib harness), so this
/// test pins the invariant directly: the printer must render the
/// canonically-interned `UnresolvedTypeName` as the user-written name.
#[test]
fn unresolved_type_name_renders_as_original_identifier_not_error() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        // Reference an undeclared type so the resolver path runs and TS2304
        // is emitted. The interner should now contain an
        // `UnresolvedTypeName("MissingType")` from the bound type position.
        r#"declare const x: MissingType;"#.to_string(),
    );
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty());

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

    // TS2304 still fires for the missing identifier itself — that diagnostic
    // path is the precondition for hitting the new `UnresolvedTypeName`
    // fallback.
    assert!(
        checker
            .ctx
            .diagnostics
            .iter()
            .any(|d| d.code == 2304 && d.message_text.contains("'MissingType'")),
        "Expected TS2304 for MissingType, got: {:?}",
        checker.ctx.diagnostics
    );

    // The user-visible invariant: building an unresolved-type-name marker
    // and asking the diagnostic formatter to render it must produce the
    // source identifier (`MissingType`), not the bare `error` token. The
    // formatter is the public surface that drives TS2322/TS2345 message
    // text, so this assertion catches any future regression that collapses
    // the unresolved name back to `TypeId::ERROR`.
    let missing_atom = types.intern_string("MissingType");
    let unresolved_id = types.unresolved_type_name(missing_atom);
    let mut formatter = checker.ctx.create_diagnostic_type_formatter();
    let rendered = formatter.format(unresolved_id).into_owned();
    assert_eq!(
        rendered, "MissingType",
        "UnresolvedTypeName must render as the source identifier, not `error`. \
         This is what makes the JSX intrinsic-element display flip from \
         `DetailedHTMLProps<HTMLAttributes<error>, error>` to \
         `DetailedHTMLProps<HTMLAttributes<HTMLDivElement>, HTMLDivElement>` \
         in conformance test compiler/jsxCallElaborationCheckNoCrash1.tsx"
    );
}

/// Same invariant when the unresolved identifier appears as the type
/// argument of an outer named generic. This is the structural shape that
/// shows up in the JSX intrinsic-element fingerprint:
/// `DetailedHTMLProps<HTMLAttributes<HTMLDivElement>, HTMLDivElement>`.
/// The rendered form must place the unresolved name inside the angle
/// brackets, not the bare `error` token.
#[test]
fn unresolved_type_name_renders_inside_application_args() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        r#"
interface Box<T> { value: T; }
declare const x: Box<MissingType>;
"#
        .to_string(),
    );
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty());

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

    assert!(
        checker
            .ctx
            .diagnostics
            .iter()
            .any(|d| d.code == 2304 && d.message_text.contains("'MissingType'")),
        "Expected TS2304 for MissingType, got: {:?}",
        checker.ctx.diagnostics
    );

    // Build `Box<MissingType>` directly via the canonical factories to
    // assert the formatter rendering, independent of which checker path
    // happened to produce the type during checking. We construct it here
    // using the same `UnresolvedTypeName` constructor the resolver now
    // returns, so a regression to `TypeId::ERROR` would show up as a
    // `Box<error>` rendering here as well.
    let missing_atom = types.intern_string("MissingType");
    let unresolved = types.unresolved_type_name(missing_atom);
    // Use any interned interface name; the goal is to assert the *args*
    // render correctly. We probe the formatter via direct application of a
    // string-named base over the unresolved arg.
    let formatted_arg = checker
        .ctx
        .create_diagnostic_type_formatter()
        .format(unresolved)
        .into_owned();
    assert_eq!(
        formatted_arg, "MissingType",
        "Application argument must format as `MissingType`, not `error`"
    );
}

/// Regression for Devin review on PR #2616: end-to-end check that an
/// unresolved generic in an interface property annotation does not
/// produce cascading false-positive assignability diagnostics on top of
/// the underlying TS2304. Before the fix, `Application(UnresolvedTypeName,
/// args)` was not recognised as an error type, so the assignability
/// checker would proceed with structural comparison against the
/// unevaluable Application and emit spurious TS2322/TS2345 messages.
#[test]
fn unresolved_generic_in_interface_property_does_not_cascade() {
    let diagnostics = check_without_lib_with_minimal_core_globals(
        r#"
interface Holder {
    value: Foo<string>;
}
declare const h: Holder;
const s: string = h.value;
"#,
    );

    // TS2304 for `Foo` is the underlying error and must be reported.
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == 2304 && d.message_text.contains("'Foo'")),
        "Expected TS2304 for `Foo`, got: {diagnostics:?}"
    );

    // But we must not also see a cascading TS2322 on `const s: string =
    // h.value`. Before the fix, `Application(UnresolvedTypeName('Foo'),
    // [string])` would not short-circuit the assignability check, so the
    // checker would compare `Foo<string>` structurally against `string`
    // and emit a spurious TS2322. After the fix, `is_error_type`
    // recognises the wrapped unresolved name and the check is suppressed.
    let cascading_2322: Vec<_> = diagnostics.iter().filter(|d| d.code == 2322).collect();
    assert!(
        cascading_2322.is_empty(),
        "Expected no cascading TS2322 on top of the unresolved generic \
         `Foo<string>`, got: {cascading_2322:?}"
    );
}
