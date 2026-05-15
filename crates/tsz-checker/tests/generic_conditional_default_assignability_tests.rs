//! Tests for TS2322 false-positive suppression when assigning to generic types
//! whose default type argument is a conditional that resolves to a concrete type.
//!
//! Rule: when a generic type parameter default is a conditional whose check and
//! extends types are both concrete after substitution, the default evaluates to a
//! single branch.  Structural comparison of an object literal against the wrapper
//! type must then succeed without a false TS2322.

use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_binder::lib_loader::LibFile;
use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::diagnostic_codes;
use tsz_checker::state::CheckerState;
use tsz_checker::test_utils::load_lib_files;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn load_map_lib_files() -> Vec<Arc<LibFile>> {
    load_lib_files(&[
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
        "es2019.array.d.ts",
        "dom.d.ts",
        "dom.generated.d.ts",
        "dom.iterable.d.ts",
        "esnext.d.ts",
    ])
}

fn compile_with_map_libs(source: &str) -> Vec<(u32, String)> {
    let file_name = "test.ts";
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    let lib_files = load_map_lib_files();

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
        file_name.to_string(),
        CheckerOptions::default(),
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
    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

fn ts2322_count(diags: &[(u32, String)]) -> usize {
    diags
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .count()
}

// ── True branch (K extends string → true) ──────────────────────────────────

/// Original repro: assigning `{ data: m }` where `m: Map<string,number>` to
/// `Test1<string, number>` whose default `M = K extends string ? Map<K,V> :
/// Map<string,V>` resolves to `Map<string, number>`.
#[test]
fn no_false_ts2322_generic_conditional_default_true_branch() {
    let libs = load_map_lib_files();
    if libs.is_empty() {
        return; // skip when lib files are not available
    }
    let diags = compile_with_map_libs(
        r#"
type Test1<K, V, M = K extends string ? Map<K, V> : Map<string, V>> = { data: M };
declare const m: Map<string, number>;
const t1: Test1<string, number> = { data: m };
"#,
    );
    assert_eq!(
        ts2322_count(&diags),
        0,
        "Expected no TS2322 when assigning Map<string,number> to Test1<string,number>: {diags:?}"
    );
}

/// Same shape but with renamed type parameters — proves the fix is not
/// sensitive to the names `K`, `V`, or `M`.
#[test]
fn no_false_ts2322_generic_conditional_default_renamed_params() {
    let libs = load_map_lib_files();
    if libs.is_empty() {
        return;
    }
    let diags = compile_with_map_libs(
        r#"
type Wrap<A, B, C = A extends string ? Map<A, B> : Map<string, B>> = { data: C };
declare const m: Map<string, number>;
const w: Wrap<string, number> = { data: m };
"#,
    );
    assert_eq!(
        ts2322_count(&diags),
        0,
        "Expected no TS2322 with renamed params A/B/C: {diags:?}"
    );
}

// ── False branch (K not extends string → false) ─────────────────────────────

/// When the check type does NOT extend the extends type the false branch is
/// chosen.  Assigning the correct false-branch type must also be error-free.
#[test]
fn no_false_ts2322_generic_conditional_default_false_branch() {
    let libs = load_map_lib_files();
    if libs.is_empty() {
        return;
    }
    let diags = compile_with_map_libs(
        r#"
type Test1<K, V, M = K extends string ? Map<K, V> : Map<string, V>> = { data: M };
declare const m: Map<string, string>;
const t2: Test1<number, string> = { data: m };
"#,
    );
    assert_eq!(
        ts2322_count(&diags),
        0,
        "Expected no TS2322 when assigning Map<string,string> to Test1<number,string>: {diags:?}"
    );
}

/// Same false-branch scenario with renamed type parameters.
#[test]
fn no_false_ts2322_generic_conditional_default_false_branch_renamed() {
    let libs = load_map_lib_files();
    if libs.is_empty() {
        return;
    }
    let diags = compile_with_map_libs(
        r#"
type Container<X, Y, Z = X extends string ? Map<X, Y> : Map<string, Y>> = { payload: Z };
declare const m: Map<string, string>;
const c: Container<number, string> = { payload: m };
"#,
    );
    assert_eq!(
        ts2322_count(&diags),
        0,
        "Expected no TS2322 (false branch, renamed params): {diags:?}"
    );
}

// ── Direct (non-conditional) generic default ────────────────────────────────

/// A direct Application default (no conditional) must also be assignable
/// without a false positive.
#[test]
fn no_false_ts2322_generic_direct_application_default() {
    let libs = load_map_lib_files();
    if libs.is_empty() {
        return;
    }
    let diags = compile_with_map_libs(
        r#"
type Box<K, V, M = Map<K, V>> = { store: M };
declare const m: Map<string, number>;
const b: Box<string, number> = { store: m };
"#,
    );
    assert_eq!(
        ts2322_count(&diags),
        0,
        "Expected no TS2322 for direct Map<K,V> default: {diags:?}"
    );
}

// ── Negative / regression guards ────────────────────────────────────────────

/// A genuinely wrong type must still produce TS2322 — the fix must not
/// suppress real errors.
#[test]
fn real_ts2322_is_still_reported_with_conditional_default() {
    let libs = load_map_lib_files();
    if libs.is_empty() {
        return;
    }
    let diags = compile_with_map_libs(
        r#"
type Test1<K, V, M = K extends string ? Map<K, V> : Map<string, V>> = { data: M };
const bad: Test1<string, number> = { data: "wrong" };
"#,
    );
    assert!(
        ts2322_count(&diags) >= 1,
        "Expected TS2322 when assigning string to Test1<string,number>.data: {diags:?}"
    );
}

/// Providing an explicit third argument that differs from the value type must
/// still produce a type error (TS2322 or a structural mismatch diagnostic).
#[test]
fn explicit_third_arg_mismatch_still_errors() {
    let libs = load_map_lib_files();
    if libs.is_empty() {
        return;
    }
    let diags = compile_with_map_libs(
        r#"
type Test1<K, V, M = K extends string ? Map<K, V> : Map<string, V>> = { data: M };
declare const m: Map<string, number>;
const t: Test1<string, number, Set<string>> = { data: m };
"#,
    );
    // tsc reports a structural mismatch (TS2322 or TS2741) when Map<string,number>
    // is provided for a property typed as Set<string>.
    assert!(
        !diags.is_empty(),
        "Expected a type error when Map<string,number> is provided but explicit M=Set<string>: {diags:?}"
    );
}
