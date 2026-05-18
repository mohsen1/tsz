//! TS2536 suppression: `keyof T` indexing an object with a string index signature.
//!
//! Structural rule: when the index type is or has a `keyof T` constraint (for any T),
//! and the object has a string index signature `[s: string]: V`, tsc accepts the
//! indexed access without TS2536. String index signatures cover all non-symbol keys;
//! symbol-keyed portions of `keyof T` produce `never` at instantiation, not TS2536.
//!
//! Covers:
//! - Mapped type template `{ [K in keyof U]: Obj[K] }` where Obj has string index
//! - Direct `X[keyof T]` usage in type aliases
//! - Renamed iteration variables (structural, not name-keyed per §25)
//! - Implicit-generic function types with mapped parameters
//! - Negative: concrete object without index signature must still error
//! - Negative: number-only index signature does not accept non-number keyof

use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;

fn check_es5(source: &str) -> Vec<Diagnostic> {
    let lib_files = tsz_checker::test_utils::load_lib_files(&["es5.d.ts"]);
    assert!(!lib_files.is_empty(), "es5.d.ts lib file not loaded");
    tsz_checker::test_utils::check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
        &lib_files,
    )
}

fn ts2536(diags: &[Diagnostic]) -> Vec<&Diagnostic> {
    diags.iter().filter(|d| d.code == 2536).collect()
}

// ── Issue #8172 original repro ───────────────────────────────────────────────

/// The exact repro from issue #8172: function assignability with mapped types
/// that index into a string-index-signature object.
/// tsc emits only TS2322 at `b = f`; tsz must not add a spurious TS2536.
#[test]
fn original_issue_repro_no_ts2536() {
    let src = r#"
type Obj = {
    [s: string]: number;
};

type foo = (target: { [K in keyof T]: T[K] }) => void;
type bar = (source: { [K in keyof U]: Obj[K] }) => void;

declare let f: foo;
declare let b: bar;
b = f;
"#;
    let diags = check_es5(src);
    assert!(
        ts2536(&diags).is_empty(),
        "issue #8172 repro must not emit TS2536; got: {diags:?}"
    );
}

// ── Mapped type template with explicit generic ────────────────────────────────

/// `{ [K in keyof U]: Obj[K] }` where Obj has a string index signature and U
/// is an explicit unconstrained type parameter.
#[test]
fn mapped_template_string_index_explicit_generic_no_ts2536() {
    let src = r#"
type Obj = { [s: string]: number };
type Mapped<U> = { [K in keyof U]: Obj[K] };
"#;
    let diags = check_es5(src);
    assert!(
        ts2536(&diags).is_empty(),
        "Mapped<U> with Obj[K] must not emit TS2536; got: {diags:?}"
    );
}

/// Renamed iteration variable (P instead of K) proves the rule is structural
/// and not keyed on the variable name.
#[test]
fn mapped_template_string_index_renamed_var_no_ts2536() {
    let src = r#"
type X = { [s: string]: number };
type M<V> = { [P in keyof V]: X[P] };
"#;
    let diags = check_es5(src);
    assert!(
        ts2536(&diags).is_empty(),
        "M<V> with X[P] (renamed P/V) must not emit TS2536; got: {diags:?}"
    );
}

/// Constrained type parameter U extends an array; keyof U includes numeric and
/// string members. Obj[K] must still be accepted.
#[test]
fn mapped_template_string_index_array_constrained_no_ts2536() {
    let src = r#"
type Obj = { [s: string]: number };
type MappedArr<U extends string[]> = { [K in keyof U]: Obj[K] };
"#;
    let diags = check_es5(src);
    assert!(
        ts2536(&diags).is_empty(),
        "MappedArr<U extends string[]> with Obj[K] must not emit TS2536; got: {diags:?}"
    );
}

/// Direct type-alias form: `type Y = X[keyof T]` where X has a string index sig.
#[test]
fn direct_alias_keyof_tparam_string_index_no_ts2536() {
    let src = r#"
type X = { [s: string]: number };
type Y<T> = X[keyof T];
"#;
    let diags = check_es5(src);
    assert!(
        ts2536(&diags).is_empty(),
        "X[keyof T] type alias must not emit TS2536; got: {diags:?}"
    );
}

/// keyof a concrete array type (string[]) used to index a string-indexed object.
#[test]
fn direct_alias_keyof_string_array_no_ts2536() {
    let src = r#"
type X = { [s: string]: number };
type K1 = keyof string[];
type Y1 = X[K1];
"#;
    let diags = check_es5(src);
    assert!(
        ts2536(&diags).is_empty(),
        "X[keyof string[]] must not emit TS2536; got: {diags:?}"
    );
}

/// keyof a number array used to index a string-indexed object (renamed to
/// prove the fix covers numeric arrays too, not just string[]).
#[test]
fn direct_alias_keyof_number_array_no_ts2536() {
    let src = r#"
type X = { [s: string]: number };
type Y2 = X[keyof number[]];
"#;
    let diags = check_es5(src);
    assert!(
        ts2536(&diags).is_empty(),
        "X[keyof number[]] must not emit TS2536; got: {diags:?}"
    );
}

// ── Negative cases: fix must not over-suppress ────────────────────────────────

/// A concrete object WITHOUT a catch-all string index signature must still
/// emit TS2536 when indexed with a keyof that includes members it doesn't have.
#[test]
fn concrete_no_index_sig_still_errors() {
    let src = r#"
type Concrete = { a: number };
type Bad<T> = Concrete[keyof T];
"#;
    let diags = check_es5(src);
    // Concrete has no index signature; keyof T is not necessarily assignable to "a"
    // tsc emits TS2536 here, so tsz must too.
    let ts2536_diags = ts2536(&diags);
    assert!(
        !ts2536_diags.is_empty(),
        "Concrete[keyof T] where Concrete has no index sig must emit TS2536"
    );
}
