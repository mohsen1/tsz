//! Tests for `infer A extends keyof T` constraint substitution during instantiation.
//!
//! When a conditional type has an `infer` variable with a constraint that references
//! type parameters (e.g., `infer A extends keyof T`), the constraint must be properly
//! substituted when the outer type parameters are instantiated. Previously, the
//! constraint TypeId was not substituted, causing `keyof T` to reference a stale
//! type parameter instead of the concrete type, making the infer pattern fail.

use rustc_hash::FxHashSet;
use std::path::Path;
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_binder::lib_loader::LibFile;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn check_strict(source: &str) -> Vec<tsz_checker::diagnostics::Diagnostic> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();

    let options = CheckerOptions {
        strict: true,
        ..Default::default()
    };

    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );

    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

fn load_lib_files_for_test() -> Vec<Arc<LibFile>> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let lib_roots = [
        manifest_dir.join("../../crates/tsz-core/src/lib-assets"),
        manifest_dir.join("../../crates/tsz-core/src/lib-assets-stripped"),
        manifest_dir.join("../../TypeScript/src/lib"),
    ];
    let lib_names = [
        "es5.d.ts",
        "es2015.d.ts",
        "es2015.core.d.ts",
        "es2015.collection.d.ts",
        "es2015.iterable.d.ts",
        "es2015.generator.d.ts",
        "es2015.promise.d.ts",
        "es2015.proxy.d.ts",
        "es2015.reflect.d.ts",
        "es2015.symbol.d.ts",
        "es2015.symbol.wellknown.d.ts",
    ];

    let mut lib_files = Vec::new();
    let mut seen_files = FxHashSet::default();
    for file_name in lib_names {
        for root in &lib_roots {
            let lib_path = root.join(file_name);
            if lib_path.exists()
                && let Ok(content) = std::fs::read_to_string(&lib_path)
            {
                if !seen_files.insert(file_name.to_string()) {
                    break;
                }
                lib_files.push(Arc::new(LibFile::from_source(
                    file_name.to_string(),
                    content,
                )));
                break;
            }
        }
    }

    lib_files
}

fn check_strict_with_libs(source: &str) -> Vec<tsz_checker::diagnostics::Diagnostic> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let lib_files = load_lib_files_for_test();

    let mut binder = BinderState::new();
    if lib_files.is_empty() {
        binder.bind_source_file(parser.get_arena(), root);
    } else {
        binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);
    }

    let types = TypeInterner::new();
    let options = CheckerOptions {
        strict: true,
        ..Default::default()
    };
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );

    if !lib_files.is_empty() {
        let lib_contexts: Vec<tsz_checker::context::LibContext> = lib_files
            .iter()
            .map(|lib| tsz_checker::context::LibContext {
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

fn has_error(diags: &[tsz_checker::diagnostics::Diagnostic], code: u32) -> bool {
    diags.iter().any(|d| d.code == code)
}

/// `infer A extends keyof T` should work when T is a substituted type parameter.
/// `GetPath<T, P>` recursively walks a path through an object type.
#[test]
fn test_infer_extends_keyof_in_conditional_type() {
    let source = r#"
type Obj = { a: { b: { c: "123" } } };

type GetPath<T, P> =
    P extends readonly [] ? T :
    P extends readonly [infer A extends keyof T, ...infer Rest] ? GetPath<T[A], Rest> :
    never;

type Result = GetPath<Obj, readonly ['a', 'b', 'c']>;

declare let r: Result;
let n: number = r;  // should be TS2322: Type '"123"' is not assignable to type 'number'
"#;
    let diags = check_strict(source);
    assert!(
        has_error(&diags, 2322),
        "Expected TS2322 because GetPath<Obj, ['a', 'b', 'c']> should evaluate to '\"123\"', not 'never'. Got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// `infer A extends keyof T` in a tuple pattern should match correctly.
#[test]
fn test_infer_extends_keyof_tuple_pattern() {
    let source = r#"
type Obj = { a: number; b: string };

type FirstKey<T, P> = P extends [infer A extends keyof T, ...infer Rest] ? A : "no_match";
type R = FirstKey<Obj, ["a", "b"]>;

declare let r: R;
let c: "no_match" = r;  // should error: R is "a", not "no_match"
"#;
    let diags = check_strict(source);
    assert!(
        has_error(&diags, 2322),
        "Expected TS2322 because FirstKey<Obj, [\"a\", \"b\"]> should be \"a\", not \"no_match\". Got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// Non-tuple `infer extends keyof T` should also work after substitution.
#[test]
fn test_infer_extends_keyof_non_tuple() {
    let source = r#"
type Obj = { a: number; b: string };

type Test<T, X> = X extends infer A extends keyof T ? A : "no_match";
type R = Test<Obj, "a">;

declare let r: R;
let c: "no_match" = r;  // should error: R is "a", not "no_match"
"#;
    let diags = check_strict(source);
    assert!(
        has_error(&diags, 2322),
        "Expected TS2322 because Test<Obj, \"a\"> should be \"a\", not \"no_match\". Got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// When infer constraint is a concrete type (not referencing type params),
/// it should continue to work correctly.
#[test]
fn test_infer_extends_concrete_constraint_still_works() {
    let source = r#"
type Test<X> = X extends infer A extends string ? A : "no_match";
type R = Test<"hello">;

declare let r: R;
let c: "no_match" = r;  // should error: R is "hello"
"#;
    let diags = check_strict(source);
    assert!(
        has_error(&diags, 2322),
        "Expected TS2322 for concrete constraint case. Got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// Template literal `infer N extends number` should parse matching string
/// captures into numeric literals. This keeps tuple string keys like "0" and
/// "1" usable as ordinal indices.
#[test]
fn test_template_literal_infer_extends_number_extracts_tuple_indices() {
    let source = r#"
type IndexFor<S extends string> = S extends `${infer N extends number}` ? N : never;
type Extract<T, U> = T extends U ? T : never;
type IndicesOf<T> = IndexFor<Extract<keyof T, string>>;

declare function getIndex<I extends IndicesOf<[{ name: "x" }, { name: "y" }]>>(index: I): void;

getIndex(0);
getIndex(1);
getIndex(2);
"#;
    let diags = check_strict(source);
    let ts2345 = diags.iter().filter(|d| d.code == 2345).count();
    assert_eq!(
        ts2345, 1,
        "Expected only getIndex(2) to emit TS2345; valid tuple indices 0 and 1 should be accepted. Got: {diags:#?}"
    );
    let message = diags
        .iter()
        .find(|d| d.code == 2345)
        .map(|d| d.message_text.as_str())
        .unwrap_or("");
    assert!(
        message.contains("parameter of type '0 | 1'"),
        "Expected invalid tuple index diagnostic to display the evaluated index union, got: {message}"
    );
}

#[test]
fn test_template_literal_infer_extends_number_direct_union() {
    let source = r#"
type IndexFor<S extends string> = S extends `${infer N extends number}` ? N : never;
type R = IndexFor<"0" | "1">;

declare function getIndex<I extends R>(index: I): void;

getIndex(0);
getIndex(1);
getIndex(2);
"#;
    let diags = check_strict(source);
    let ts2345 = diags.iter().filter(|d| d.code == 2345).count();
    assert_eq!(
        ts2345, 1,
        "Expected only getIndex(2) to emit TS2345 for direct string numeric keys. Got: {diags:#?}"
    );
}

#[test]
fn test_template_literal_infer_extends_number_after_extract() {
    let source = r#"
type IndexFor<S extends string> = S extends `${infer N extends number}` ? N : never;
type Extract<T, U> = T extends U ? T : never;
type R = IndexFor<Extract<"0" | "1" | "length", string>>;

declare function getIndex<I extends R>(index: I): void;

getIndex(0);
getIndex(1);
getIndex(2);
"#;
    let diags = check_strict(source);
    let ts2345 = diags.iter().filter(|d| d.code == 2345).count();
    assert_eq!(
        ts2345, 1,
        "Expected only getIndex(2) to emit TS2345 after Extract. Got: {diags:#?}"
    );
}

#[test]
fn test_path_keys_accepts_readonly_tuple_numeric_string_index() {
    let source = r#"
type PropType<T, Path extends string> =
    string extends Path ? unknown :
    Path extends keyof T ? T[Path] :
    Path extends `${infer K}.${infer R}` ? K extends keyof T ? PropType<T[K], R> : unknown :
    unknown;

type PathKeys<T> =
    unknown extends T ? never :
    T extends readonly any[] ? Extract<keyof T, `${number}`> | SubKeys<T, Extract<keyof T, `${number}`>> :
    T extends object ? Extract<keyof T, string> | SubKeys<T, Extract<keyof T, string>> :
    never;

type SubKeys<T, K extends string> = K extends keyof T ? `${K}.${PathKeys<T[K]>}` : never;

declare function getProp<T, P extends PathKeys<T>>(obj: T, path: P): PropType<T, P>;

const obj2 = {
    name: 'John',
    age: 42,
    cars: [
        { make: 'Ford', age: 10 },
        { make: 'Trabant', age: 35 }
    ]
} as const;

getProp(obj2, 'cars.1.make');
"#;
    let diags = check_strict_with_libs(source);
    let ts2345: Vec<_> = diags.iter().filter(|d| d.code == 2345).collect();
    assert!(
        ts2345.is_empty(),
        "Expected readonly tuple numeric string path to be accepted without TS2345. Got: {diags:#?}"
    );
}

/// When infer constraint fails (value doesn't match keyof T), should get false branch.
#[test]
fn test_infer_extends_keyof_constraint_fails_correctly() {
    let source = r#"
type Obj = { a: number };

type Test<T, X> = X extends infer A extends keyof T ? A : "no_match";
type R = Test<Obj, "z">;  // "z" is not keyof Obj

declare let r: R;
let c: "no_match" = r;  // should NOT error: R should be "no_match"
"#;
    let diags = check_strict(source);
    assert!(
        !has_error(&diags, 2322),
        "Should NOT get TS2322: 'z' is not keyof Obj, so result should be 'no_match'. Got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}
