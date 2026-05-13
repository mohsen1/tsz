//! Issue #3061: a user-defined `type Required<T> = ...` must NOT
//! receive the lib-`Required<T>` mapped-utility shortcut. The shortcut
//! returned the source `T` as the constraint check target, so any type
//! satisfied a local `Required<Source>` constraint trivially. The
//! actual constraint check should run, which surfaces TS2344 when the
//! arg's shape doesn't match the user's alias body.

use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;

fn check_with_es2015(source: &str) -> Vec<Diagnostic> {
    let lib_files = tsz_checker::test_utils::load_lib_files(&[
        "es5.d.ts",
        "es2015.d.ts",
        "es2015.collection.d.ts",
        "es2015.core.d.ts",
        "es2015.iterable.d.ts",
        "es2015.symbol.d.ts",
    ]);
    assert!(!lib_files.is_empty());
    tsz_checker::test_utils::check_source_with_libs(
        source,
        "repro.ts",
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
        &lib_files,
    )
}

/// Local `type Required<T> = { marker: string }` is unrelated to the
/// lib's `Required<T>` mapped utility. The lib-`Required<T>` shortcut
/// must not silently accept `Box<Source>` — either the local alias is
/// reported (the issue's TS2344 surface) or the redeclaration is
/// rejected (TS2300 against the lib alias). The defect this test
/// guards against is the *silent* acceptance produced by the
/// name-only shortcut.
#[test]
fn local_required_alias_constraint_is_not_silently_accepted() {
    let source = r#"
type Required<T> = { marker: string };

interface Source {
  a: number;
}

type Box<T extends Required<Source>> = T;
type Use = Box<Source>;
"#;
    let diags = check_with_es2015(source);
    let saw_constraint_or_redecl = diags.iter().any(|d| d.code == 2344 || d.code == 2300);
    assert!(
        saw_constraint_or_redecl,
        "expected TS2344 (constraint failure) or TS2300 (lib redeclaration) for local Required<T>, got: {diags:?}"
    );
}

/// Sanity: a local alias unrelated to `Required` must keep the normal
/// constraint check unchanged.
#[test]
fn local_marker_alias_constraint_emits_ts2344() {
    let source = r#"
type Marker<T> = { marker: string };

interface Source {
  a: number;
}

type Box<T extends Marker<Source>> = T;
type Use = Box<Source>;
"#;
    let diags = check_with_es2015(source);
    let ts2344: Vec<&Diagnostic> = diags.iter().filter(|d| d.code == 2344).collect();
    assert!(
        !ts2344.is_empty(),
        "expected TS2344 for unrelated local alias, got: {diags:?}"
    );
}

/// Anchor: the genuine `Required<T>` from lib still benefits from the
/// shortcut — `Box<Source>` where `Source` has the required field
/// satisfies its own `Required<Source>` constraint, no TS2344.
#[test]
fn lib_required_constraint_with_satisfying_arg_emits_no_ts2344() {
    let source = r#"
interface Source {
  a: number;
}

type Box<T extends Required<Source>> = T;
type Use = Box<Source>;
"#;
    let diags = check_with_es2015(source);
    let ts2344: Vec<&Diagnostic> = diags.iter().filter(|d| d.code == 2344).collect();
    assert!(
        ts2344.is_empty(),
        "lib Required<T> shortcut must keep accepting matching arg, got: {diags:?}"
    );
}

#[test]
fn lib_required_indexed_by_mapped_keyof_key_emits_no_ts2536() {
    let source = r#"
type Test<T> = {
  [K in keyof T]: Required<T>[K];
};

type Obj = { a: number; b?: string };
type T1 = Test<Obj>;
const t1: T1 = { a: 1, b: 'x' };
"#;
    let diags = check_with_es2015(source);
    assert!(
        diags.is_empty(),
        "lib Required<T>[K] should accept K from keyof T, got: {diags:?}"
    );
}

#[test]
fn local_required_alias_indexed_by_unrelated_keyof_key_still_emits_ts2536() {
    let source = r#"
type Required<T> = { marker: string };
type Test<T> = {
  [K in keyof T]: Required<T>[K];
};
"#;
    let diags = tsz_checker::test_utils::check_source(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    );
    let ts2536: Vec<&Diagnostic> = diags.iter().filter(|d| d.code == 2536).collect();
    assert!(
        !ts2536.is_empty(),
        "local Required<T> must not receive the lib mapped key-space shortcut, got: {diags:?}"
    );
}
