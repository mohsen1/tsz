//! Regression: a user-declared global `Array<T>` under `--noLib` must be
//! registered as the array base type so array member access resolves through it
//! — including for a *union* of array-likes, not only a single array receiver.
//!
//! Structural rule: when no default lib is loaded but the program declares its
//! own ambient global `Array<T>` (and other core globals), the checker must wire
//! that declaration up as the boxed/array base type. Array property/method
//! lookup then routes through the `Array<T>` interface for every array-like, so
//! `(number[] | string[]).every(...)` resolves the method (and contextually
//! types its callback) exactly as a single `number[]` receiver does. Owner:
//! `CheckerState::register_boxed_types` — previously it bailed under `--noLib`,
//! leaving `get_array_base_type` unset; single-array access limped along through
//! the checker's apparent-type recovery while unions reported a false TS2339 (and
//! a cascading TS7006 on the callback parameter). Refs #15087.

use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;

/// A minimal but method-bearing core-global preamble. `Array<T>` carries a
/// plain data property, a non-generic method, and a generic method so the test
/// exercises all three member shapes that route through the array base.
const PREAMBLE: &str = r#"
interface Array<T> {
  length: number;
  every(p: (v: T) => boolean): boolean;
  map<U>(p: (v: T) => U): U[];
}
interface Boolean {}
interface Number {}
interface String { length: number; charAt(i: number): string; }
interface Object {}
interface Function {}
interface IArguments {}
interface RegExp {}
interface CallableFunction {}
interface NewableFunction {}
"#;

fn check_nolib(body: &str) -> Vec<Diagnostic> {
    let mut source = String::from(PREAMBLE);
    source.push_str(body);
    tsz_checker::test_utils::check_with_options(
        &source,
        CheckerOptions {
            no_lib: true,
            strict: true,
            ..CheckerOptions::default()
        },
    )
}

fn count_code(diags: &[Diagnostic], code: u32) -> usize {
    diags.iter().filter(|d| d.code == code).count()
}

#[test]
fn union_of_arrays_resolves_method_off_user_array_base() {
    let diags = check_nolib("declare const x: number[] | string[];\nx.every(c => true);\n");
    assert_eq!(
        count_code(&diags, 2339),
        0,
        "method must resolve off the user-declared Array<T> for a union receiver; got {diags:#?}"
    );
    // The callback parameter must be contextually typed from the resolved
    // method signature — no cascading implicit-any (TS7006).
    assert_eq!(
        count_code(&diags, 7006),
        0,
        "callback parameter must be contextually typed; got {diags:#?}"
    );
}

#[test]
fn union_of_tuples_resolves_method_off_user_array_base() {
    let diags = check_nolib("declare const x: [number] | [string];\nx.every(c => true);\n");
    assert_eq!(count_code(&diags, 2339), 0, "got {diags:#?}");
    assert_eq!(count_code(&diags, 7006), 0, "got {diags:#?}");
}

#[test]
fn union_resolves_generic_array_method() {
    let diags = check_nolib("declare const x: number[] | string[];\nconst r = x.map(v => v);\n");
    assert_eq!(count_code(&diags, 2339), 0, "got {diags:#?}");
    assert_eq!(count_code(&diags, 7006), 0, "got {diags:#?}");
}

#[test]
fn single_array_still_resolves_method() {
    let diags = check_nolib("declare const x: number[];\nx.every(c => true);\n");
    assert_eq!(count_code(&diags, 2339), 0, "got {diags:#?}");
    assert_eq!(count_code(&diags, 7006), 0, "got {diags:#?}");
}

#[test]
fn union_resolves_data_property() {
    let diags = check_nolib("declare const x: number[] | string[];\nconst n: number = x.length;\n");
    assert_eq!(count_code(&diags, 2339), 0, "got {diags:#?}");
}

#[test]
fn user_string_interface_resolves_boxed_method() {
    // The same registration powers boxed-primitive member access under --noLib.
    let diags = check_nolib("declare const s: string;\nconst r: string = s.charAt(0);\n");
    assert_eq!(count_code(&diags, 2339), 0, "got {diags:#?}");
}

#[test]
fn genuinely_missing_method_still_errors_on_union() {
    // The fix must not mask real missing-property errors: a method absent from
    // the user `Array<T>` interface must still report TS2339 on a union.
    let diags = check_nolib("declare const x: number[] | string[];\nx.nope();\n");
    assert_eq!(
        count_code(&diags, 2339),
        1,
        "absent method must still report TS2339; got {diags:#?}"
    );
}
