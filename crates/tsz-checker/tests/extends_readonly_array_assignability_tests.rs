//! An interface/class that declares `extends Array<T>` or `extends
//! ReadonlyArray<T>` is a heritage-flattened object shape, not a syntactic array.
//! The solver subtype dispatch accepts such a source against a *mutable* array
//! target via a covariant element check (PR #13928). These tests cover the
//! symmetric `readonly U[]` / `ReadonlyArray<U>` target side and guard the
//! readonly-direction discipline (mutable source -> readonly target ok; readonly
//! source -> mutable target rejected).

use tsz_checker::context::CheckerOptions;

fn strict_diagnostics(source: &str) -> Vec<(u32, String)> {
    let libs = tsz_checker::test_utils::load_default_lib_files();
    tsz_checker::test_utils::check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
        &libs,
    )
    .into_iter()
    .map(|d| (d.code, d.message_text))
    .collect()
}

fn assignment_codes(diags: &[(u32, String)]) -> Vec<u32> {
    diags
        .iter()
        .filter(|(code, _)| matches!(*code, 2322 | 2345 | 2740 | 2741))
        .map(|(code, _)| *code)
        .collect()
}

#[test]
fn readonly_nonempty_array_assignable_to_readonly_array_target() {
    let diagnostics = strict_diagnostics(
        r#"
interface ReadonlyNonEmptyArray<A> extends ReadonlyArray<A> {
  readonly 0: A;
}
declare const r: ReadonlyNonEmptyArray<string>;
const a: readonly string[] = r;
const b: ReadonlyArray<string> = r;
"#,
    );
    let codes = assignment_codes(&diagnostics);
    assert!(
        codes.is_empty(),
        "extends-ReadonlyArray source should satisfy a readonly array target, got: {diagnostics:#?}"
    );
}

#[test]
fn mutable_nonempty_array_assignable_to_readonly_array_target() {
    // A mutable `extends Array` source is assignable to a readonly target
    // (mutable -> readonly is allowed).
    let diagnostics = strict_diagnostics(
        r#"
interface NonEmptyArray<A> extends Array<A> {
  0: A;
}
declare const n: NonEmptyArray<number>;
const a: readonly number[] = n;
const b: ReadonlyArray<number> = n;
"#,
    );
    let codes = assignment_codes(&diagnostics);
    assert!(
        codes.is_empty(),
        "extends-Array source should satisfy a readonly array target, got: {diagnostics:#?}"
    );
}

#[test]
fn readonly_nonempty_array_covariant_widening_to_readonly_target() {
    let diagnostics = strict_diagnostics(
        r#"
interface ReadonlyNonEmptyArray<A> extends ReadonlyArray<A> {
  readonly 0: A;
}
declare const r: ReadonlyNonEmptyArray<string>;
const a: readonly (string | number)[] = r;
"#,
    );
    let codes = assignment_codes(&diagnostics);
    assert!(
        codes.is_empty(),
        "covariant widening to a readonly target should be accepted, got: {diagnostics:#?}"
    );
}

#[test]
fn readonly_source_rejected_against_mutable_array_target() {
    // A `ReadonlyArray`-derived source must NOT be assignable to a *mutable*
    // array target: readonly -> mutable loses the readonly guarantee.
    let diagnostics = strict_diagnostics(
        r#"
interface ReadonlyNonEmptyArray<A> extends ReadonlyArray<A> {
  readonly 0: A;
}
declare const r: ReadonlyNonEmptyArray<string>;
const a: string[] = r;
"#,
    );
    let codes = assignment_codes(&diagnostics);
    assert!(
        !codes.is_empty(),
        "readonly-derived source must be rejected against a mutable array target, got: {diagnostics:#?}"
    );
}

#[test]
fn readonly_target_covariant_element_mismatch_rejected() {
    let diagnostics = strict_diagnostics(
        r#"
interface ReadonlyNonEmptyArray<A> extends ReadonlyArray<A> {
  readonly 0: A;
}
declare const r: ReadonlyNonEmptyArray<string | number>;
const a: readonly string[] = r;
"#,
    );
    let codes = assignment_codes(&diagnostics);
    assert!(
        !codes.is_empty(),
        "element-type mismatch must still be rejected on a readonly target, got: {diagnostics:#?}"
    );
}
