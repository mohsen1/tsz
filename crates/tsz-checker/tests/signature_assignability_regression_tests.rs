use rustc_hash::FxHashSet;
use std::path::Path;
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_binder::lib_loader::LibFile;
use tsz_checker::context::{CheckerOptions, LibContext};
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::state::CheckerState;
use tsz_common::common::ScriptTarget;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

const DEFAULT_ES2015_LIBS: &[&str] = &[
    "lib.es6.d.ts",
    "lib.es2015.d.ts",
    "lib.dom.d.ts",
    "lib.dom.iterable.d.ts",
    "lib.webworker.importscripts.d.ts",
    "lib.scripthost.d.ts",
    "lib.es5.d.ts",
    "lib.es2015.core.d.ts",
    "lib.es2015.collection.d.ts",
    "lib.es2015.iterable.d.ts",
    "lib.es2015.generator.d.ts",
    "lib.es2015.promise.d.ts",
    "lib.es2015.proxy.d.ts",
    "lib.es2015.reflect.d.ts",
    "lib.es2015.symbol.d.ts",
    "lib.es2015.symbol.wellknown.d.ts",
    "lib.es2018.asynciterable.d.ts",
];

const ASSIGNMENT_COMPAT_WITH_CALL_SIGNATURES_2: &str = r#"
interface T {
    f(x: number): void;
}
declare var t: T;
declare var a: { f(x: number): void };

t = a;
a = t;

interface S {
    f(x: number): string;
}
declare var s: S;
declare var a2: { f(x: number): string };
t = s;
t = a2;
a = s;
a = a2;

t = { f: () => 1 };
t = { f: <T>(x:T) => 1 };
t = { f: function f() { return 1 } };
t = { f(x: number) { return ''; } }
a = { f: () => 1 }
a = { f: <T>(x: T) => 1 };
a = { f: function (x: number) { return ''; } }

t = () => 1;
t = function (x: number) { return ''; }
a = () => 1;
a = function (x: number) { return ''; }

interface S2 {
    f(x: string): void;
}
declare var s2: S2;
declare var a3: { f(x: string): void };
t = s2;
t = a3;
t = (x: string) => 1;
t = function (x: string) { return ''; }
a = s2;
a = a3;
a = (x: string) => 1;
a = function (x: string) { return ''; }
"#;
const ASSIGNMENT_COMPAT_WITH_CONSTRUCT_SIGNATURES_2: &str = r#"
interface T {
    f: new (x: number) => void;
}
declare var t: T;
declare var a: { f: new (x: number) => void };

t = a;
a = t;

interface S {
    f: new (x: number) => string;
}
declare var s: S;
declare var a2: { f: new (x: number) => string };
t = s;
t = a2;
a = s;
a = a2;

t = () => 1;
t = function (x: number) { return ''; }
a = () => 1;
a = function (x: number) { return ''; }

interface S2 {
    f(x: string): void;
}
declare var s2: S2;
declare var a3: { f(x: string): void };
t = s2;
t = a3;
t = (x: string) => 1;
t = function (x: string) { return ''; }
a = s2;
a = a3;
a = (x: string) => 1;
a = function (x: string) { return ''; }
"#;

fn get_codes_with_options(source: &str, options: CheckerOptions) -> Vec<u32> {
    tsz_checker::test_utils::check_source(source, "test.ts", options)
        .into_iter()
        .filter(|diag| diag.code != 2318)
        .map(|diag| diag.code)
        .collect()
}

fn load_named_lib_files_for_test(lib_names: &[&str]) -> Vec<Arc<LibFile>> {
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

fn check_source_with_named_libs(
    source: &str,
    options: CheckerOptions,
    lib_names: &[&str],
) -> Vec<Diagnostic> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let lib_files = load_named_lib_files_for_test(lib_names);
    assert!(
        !lib_files.is_empty(),
        "test libs should be available for {lib_names:?}"
    );

    let mut binder = BinderState::new();
    binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);

    let types = TypeInterner::new();
    let definition_store = Arc::new(tsz_solver::def::DefinitionStore::from_semantic_defs(
        &binder.semantic_defs,
        |s| types.intern_string(s),
    ));
    let mut checker = CheckerState::new_with_shared_def_store(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
        definition_store,
    );

    let lib_contexts: Vec<LibContext> = lib_files
        .iter()
        .map(|lib| LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    checker.ctx.set_lib_contexts(lib_contexts);
    checker.ctx.set_actual_lib_file_count(lib_files.len());

    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

fn get_code_messages_with_options_and_libs(
    source: &str,
    options: CheckerOptions,
) -> Vec<(u32, String)> {
    check_source_with_named_libs(source, options, DEFAULT_ES2015_LIBS)
        .into_iter()
        .filter(|diag| diag.code != 2318)
        .map(|diag| (diag.code, diag.message_text))
        .collect()
}

fn conformance_default_options() -> CheckerOptions {
    CheckerOptions {
        strict: false,
        strict_null_checks: false,
        strict_function_types: false,
        strict_property_initialization: false,
        no_implicit_any: false,
        no_implicit_this: false,
        use_unknown_in_catch_variables: false,
        target: ScriptTarget::ES2015,
        ..CheckerOptions::default()
    }
}

#[test]
fn method_only_generic_variance_is_bivariant() {
    let source = r#"
interface Animal { animal: void }
interface Dog extends Animal { dog: void }

interface Comparer<T> {
    compare(a: T, b: T): number;
}

declare let animalComparer: Comparer<Animal>;
declare let dogComparer: Comparer<Dog>;

animalComparer = dogComparer;
dogComparer = animalComparer;
"#;

    let codes = get_codes_with_options(
        source,
        CheckerOptions {
            strict: true,
            strict_function_types: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !codes.contains(&2322),
        "method-only generic comparer assignments should be bivariant, got {codes:?}"
    );
}

#[test]
fn interface_method_signature_satisfies_structural_method_property() {
    let source = r#"
interface T {
    f(x: number): void;
}
declare var t: T;
declare var a: { f(x: number): void };

t = a;
a = t;
"#;

    let codes = get_codes_with_options(source, conformance_default_options());

    assert!(
        codes.is_empty(),
        "interface method signature should satisfy matching structural method property, got {codes:?}"
    );
}

#[test]
fn interface_construct_signature_property_satisfies_structural_construct_property() {
    let source = r#"
interface T {
    f: new (x: number) => void;
}
declare var t: T;
declare var a: { f: new (x: number) => void };

t = a;
a = t;
"#;

    let codes = get_codes_with_options(source, conformance_default_options());

    assert!(
        codes.is_empty(),
        "interface construct signature property should satisfy matching structural construct property, got {codes:?}"
    );
}

#[test]
fn callable_sources_fail_against_interface_method_property_target() {
    let diagnostics = get_code_messages_with_options_and_libs(
        ASSIGNMENT_COMPAT_WITH_CALL_SIGNATURES_2,
        conformance_default_options(),
    );

    assert!(
        diagnostics.iter().all(|(code, _)| *code == 2322),
        "expected only TS2322 diagnostics, got {diagnostics:?}"
    );
    assert_eq!(
        diagnostics.len(),
        12,
        "expected one TS2322 for each invalid assignment to T and its structural twin, got {diagnostics:?}"
    );
    assert_eq!(
        diagnostics
            .iter()
            .filter(|(_, message)| message.contains("not assignable to type 'T'"))
            .count(),
        6,
        "expected diagnostics to target interface T, got {diagnostics:?}"
    );
}

#[test]
fn callable_sources_fail_against_interface_construct_property_target() {
    let diagnostics = get_code_messages_with_options_and_libs(
        ASSIGNMENT_COMPAT_WITH_CONSTRUCT_SIGNATURES_2,
        conformance_default_options(),
    );

    assert!(
        diagnostics.iter().all(|(code, _)| *code == 2322),
        "expected only TS2322 diagnostics, got {diagnostics:?}"
    );
    assert_eq!(
        diagnostics.len(),
        12,
        "expected one TS2322 for each invalid assignment to T and its structural twin, got {diagnostics:?}"
    );
    assert_eq!(
        diagnostics
            .iter()
            .filter(|(_, message)| message.contains("not assignable to type 'T'"))
            .count(),
        6,
        "expected diagnostics to target interface T, got {diagnostics:?}"
    );
}

#[test]
fn nested_call_signature_assignability_does_not_stack_overflow() {
    let source = r#"
class Base { foo: string; }
class Derived extends Base { bar: string; }
class Derived2 extends Derived { baz: string; }
class OtherDerived extends Base { bing: string; }
declare class Date {}

declare var a6: (x: (arg: Base) => Derived) => Base;
declare var a7: (x: (arg: Base) => Derived) => (r: Base) => Derived;
declare var a8: (x: (arg: Base) => Derived, y: (arg2: Base) => Derived) => (r: Base) => Derived;
declare var a9: (x: (arg: Base) => Derived, y: (arg2: Base) => Derived) => (r: Base) => Derived;
declare var a15: {
    (x: number): number[];
    (x: string): string[];
};
declare var a16: {
    <T extends Derived>(x: T): number[];
    <U extends Base>(x: U): number[];
};
declare var a17: {
    (x: (a: number) => number): number[];
    (x: (a: string) => string): string[];
};
declare var a18: {
    (x: {
        (a: number): number;
        (a: string): string;
    }): any[];
    (x: {
        (a: boolean): boolean;
        (a: Date): Date;
    }): any[];
}

declare var b6: <T extends Base, U extends Derived>(x: (arg: T) => U) => T;
a6 = b6;
b6 = a6;
declare var b7: <T extends Base, U extends Derived>(x: (arg: T) => U) => (r: T) => U;
a7 = b7;
b7 = a7;
declare var b8: <T extends Base, U extends Derived>(x: (arg: T) => U, y: (arg2: T) => U) => (r: T) => U;
a8 = b8;
b8 = a8;
declare var b9: <T extends Base, U extends Derived>(x: (arg: T) => U, y: (arg2: { foo: string; bing: number }) => U) => (r: T) => U;
a9 = b9;
b9 = a9;
declare var b15: <T>(x: T) => T[];
a15 = b15;
b15 = a15;
declare var b16: <T extends Base>(x: T) => number[];
a16 = b16;
b16 = a16;
declare var b17: <T>(x: (a: T) => T) => T[];
a17 = b17;
b17 = a17;
declare var b18: <T>(x: (a: T) => T) => T[];
a18 = b18;
b18 = a18;
"#;

    let codes = get_codes_with_options(source, conformance_default_options());

    assert!(
        codes.iter().all(|&code| code == 2322),
        "expected only TS2322 diagnostics, got {codes:?}"
    );
}
