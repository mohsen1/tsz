//! Locks in canonicalization of `Array<T>` / `ReadonlyArray<T>` in tuple rest
//! position so the rest element type is the array's element type, not the
//! whole array.
//!
//! Regression: #3988 — `[T, ...Array<U>]` and bare `[T, ...Array]` stored the
//! generic `Array<U>` reference itself as the rest element type, which the
//! tuple machinery does not unwrap. This produced false TS2322 on tuple
//! initialization (`Type 'U' is not assignable to type 'Array<U>'`) and false
//! TS2339 on destructured rest elements
//! (`Property 'm' does not exist on type 'Array<U> | undefined'`).

use std::path::Path;
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_binder::lib_loader::LibFile;
use tsz_checker::context::CheckerOptions;
use tsz_checker::context::LibContext;
use tsz_checker::diagnostics::Diagnostic;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn load_lib_files_for_test() -> Vec<Arc<LibFile>> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let lib_roots = [
        manifest_dir.join("../../crates/tsz-core/src/lib-assets"),
        manifest_dir.join("../../crates/tsz-core/src/lib-assets-stripped"),
        manifest_dir.join("../../TypeScript/src/lib"),
    ];
    let lib_names = ["es5.d.ts", "es2015.d.ts", "es2015.core.d.ts"];

    let mut lib_files = Vec::new();
    for file_name in lib_names {
        for root in &lib_roots {
            let lib_path = root.join(file_name);
            if lib_path.exists()
                && let Ok(content) = std::fs::read_to_string(&lib_path)
            {
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

fn check_with_libs(source: &str, options: CheckerOptions) -> Vec<Diagnostic> {
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
    let mut checker = tsz_checker::state::CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );

    if !lib_files.is_empty() {
        let lib_contexts: Vec<LibContext> = lib_files
            .iter()
            .map(|lib| LibContext {
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

fn check_codes(source: &str) -> Vec<u32> {
    check_with_libs(source, CheckerOptions::default())
        .into_iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn rest_array_generic_does_not_emit_false_ts2322_on_tuple_init() {
    let source = r#"
type T1 = [string, ...Array<number>];
const t1: T1 = ["a", 1, 2, 3];
"#;
    let codes = check_codes(source);
    assert!(
        !codes.contains(&2322),
        "expected no TS2322 on `[string, ...Array<number>]` init; got {codes:?}",
    );
}

#[test]
fn rest_array_generic_direct_annotation_does_not_emit_false_ts2322() {
    // No alias indirection — the tuple type is used directly as the variable
    // annotation. This exercises the `TypeNodeChecker::get_type_from_tuple_type`
    // path rather than the alias-body lowering path.
    let source = r#"
const t: [string, ...Array<number>] = ["a", 1, 2, 3];
"#;
    let codes = check_codes(source);
    assert!(
        !codes.contains(&2322),
        "expected no TS2322 on direct `[string, ...Array<number>]` init; got {codes:?}",
    );
}

#[test]
fn rest_readonly_array_generic_canonicalizes() {
    let source = r#"
const t: [string, ...ReadonlyArray<boolean>] = ["a", true, false];
"#;
    let codes = check_codes(source);
    assert!(
        !codes.contains(&2322),
        "expected no TS2322 on `[string, ...ReadonlyArray<boolean>]` init; got {codes:?}",
    );
}

#[test]
fn destructured_rest_element_via_array_generic_has_array_methods() {
    let source = r#"
type T = [string, ...Array<number>];
const t: T = ["a", 1, 2, 3];
const [head, ...rest] = t;
const x: number = rest[0];
const y: number[] = rest;
"#;
    let codes = check_codes(source);
    assert!(
        !codes.contains(&2322) && !codes.contains(&2339),
        "expected no TS2322/TS2339 on destructured rest; got {codes:?}",
    );
}

#[test]
fn bare_array_rest_recovers_as_array_any() {
    // `...Array` (no type argument) must keep emitting TS2314, but the type
    // should recover as `Array<any>` so initialization and destructuring do
    // not cascade further diagnostics.
    let source = r#"
type T = [string, ...Array];
const t: T = ["a", 1, "x", true];
const [head, ...rest] = t;
rest[0].toString();
"#;
    let codes = check_codes(source);
    assert!(
        codes.contains(&2314),
        "expected TS2314 for bare `...Array`; got {codes:?}",
    );
    assert!(
        !codes.contains(&2322) && !codes.contains(&2339),
        "expected recovery to suppress TS2322/TS2339 cascades; got {codes:?}",
    );
}

#[test]
fn named_rest_member_array_generic_canonicalizes() {
    let source = r#"
type T = [first: string, ...rest: Array<number>];
const t: T = ["a", 1, 2];
"#;
    let codes = check_codes(source);
    assert!(
        !codes.contains(&2322),
        "expected no TS2322 on named tuple `...rest: Array<number>`; got {codes:?}",
    );
}

#[test]
fn rest_array_generic_works_independently_of_type_param_name() {
    // Sanity check that the fix is structural and does not depend on the
    // user's type-argument identifier. Using `K` instead of `T` must produce
    // the same canonicalization.
    let source = r#"
type T<K> = [string, ...Array<K>];
const t: T<number> = ["a", 1, 2, 3];
"#;
    let codes = check_codes(source);
    assert!(
        !codes.contains(&2322),
        "expected no TS2322 with type-parameter K; got {codes:?}",
    );
}

#[test]
fn rest_array_alias_indirection_still_works() {
    // Type alias to `Array<T>` used in rest position must continue to work
    // (it already did before the fix because alias bodies are lowered through
    // a different path; this test guards against future regressions).
    let source = r#"
type N = Array<number>;
type T = [string, ...N];
const t: T = ["a", 1, 2, 3];
"#;
    let codes = check_codes(source);
    assert!(
        !codes.contains(&2322),
        "expected no TS2322 with alias-indirected rest; got {codes:?}",
    );
}
