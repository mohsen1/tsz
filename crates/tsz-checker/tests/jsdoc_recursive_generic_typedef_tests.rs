//! Regression tests for self-recursive *generic* JSDoc `@typedef` resolution.
//!
//! Issue #9751: a generic `@typedef` whose body applies its own name with
//! type arguments (e.g. `@template T @typedef {{ next: Box<T> | null }} Box`)
//! previously overflowed the stack. `resolve_jsdoc_generic_typedef_type`
//! eagerly re-expanded the body for every self-application instead of
//! deferring to a lazy alias. The fix registers the alias as a lazy `DefId`
//! before constructing its body and records it in a re-entrancy guard, so the
//! inner self-application resolves to `Application(Lazy(DefId), args)` and the
//! solver resolves it coinductively.
//!
//! The rule: when a generic JSDoc `@typedef` references itself with type
//! arguments inside its own body, tsc resolves the reference lazily (no
//! crash, structural mismatches still surface); this makes tsz do the same.
//!
//! Note the type-parameter and alias names are varied across cases — if any
//! test breaks when the name changes, the fix would be hardcoded.

use tsz_checker::CheckerOptions;
use tsz_checker::test_utils::check_source;

fn strict_js_options() -> CheckerOptions {
    CheckerOptions {
        allow_js: true,
        check_js: true,
        strict: true,
        strict_null_checks: true,
        no_implicit_any: true,
        ..Default::default()
    }
}

#[test]
fn self_recursive_generic_typedef_does_not_crash() {
    // Pre-fix this overflowed the stack and aborted the process. The assertion
    // is simply that the check returns and accepts the well-formed value.
    let source = r"/**
 * @template T
 * @typedef {{ next: Box<T> | null }} Box
 */
/** @type {Box<number>} */
const b = { next: null };
";
    let diagnostics = check_source(source, "box.js", strict_js_options());
    assert!(
        diagnostics.is_empty(),
        "well-formed self-recursive generic typedef should type-check cleanly, got: {diagnostics:#?}"
    );
}

#[test]
fn self_recursive_generic_typedef_surfaces_member_mismatch() {
    // A mismatched member must still produce TS2322 rather than crashing or
    // being silenced by the cycle guard. `value` is typed `T` = `number`, so
    // assigning a string is an error (matching tsc).
    let source = r#"/**
 * @template T
 * @typedef {{ value: T, next: Box<T> | null }} Box
 */
/** @type {Box<number>} */
const b = { value: "x", next: null };
"#;
    let diagnostics = check_source(source, "box_mismatch.js", strict_js_options());
    assert!(
        diagnostics.iter().any(|d| d.code == 2322),
        "mismatched member should emit TS2322, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn self_recursive_generic_typedef_renamed_param_and_alias() {
    // Same shape, different alias (`Node`→`Lnk`) and type-parameter (`T`→`U`)
    // names. If the fix were keyed on a literal name it would break here.
    let source = r#"/**
 * @template U
 * @typedef {{ data: U, tail: Lnk<U> | null }} Lnk
 */
/** @type {Lnk<string>} */
const n = { data: "hi", tail: null };
"#;
    let diagnostics = check_source(source, "lnk.js", strict_js_options());
    assert!(
        diagnostics.is_empty(),
        "renamed self-recursive generic typedef should type-check cleanly, got: {diagnostics:#?}"
    );
}

#[test]
fn self_recursive_generic_typedef_multiple_type_params() {
    // Two type parameters carried through the recursive application.
    let source = r#"/**
 * @template K, V
 * @typedef {{ key: K, val: V, next: Pair<K, V> | null }} Pair
 */
/** @type {Pair<string, number>} */
const p = { key: "a", val: 2, next: null };
"#;
    let diagnostics = check_source(source, "pair.js", strict_js_options());
    assert!(
        diagnostics.is_empty(),
        "multi-parameter recursive generic typedef should type-check cleanly, got: {diagnostics:#?}"
    );
}

#[test]
fn mutually_recursive_generic_typedefs_do_not_crash() {
    // A<T> references B<T> which references A<T>. The guard must keep every
    // alias on the in-progress set until its own body is built.
    let source = r"/**
 * @template T
 * @typedef {{ b: Bee<T> | null, v: T }} Aye
 */
/**
 * @template T
 * @typedef {{ a: Aye<T> | null }} Bee
 */
/** @type {Aye<number>} */
const x = { b: null, v: 1 };
";
    let diagnostics = check_source(source, "mutual.js", strict_js_options());
    assert!(
        diagnostics.is_empty(),
        "mutually recursive generic typedefs should type-check cleanly, got: {diagnostics:#?}"
    );
}

#[test]
fn non_generic_recursive_typedef_still_resolves() {
    // Negative control / regression guard: the non-generic recursive typedef
    // path (protected by a separate guard) must keep working.
    let source = r"/** @typedef {{ next: List | null }} List */
/** @type {List} */
const b = { next: null };
";
    let diagnostics = check_source(source, "list.js", strict_js_options());
    assert!(
        diagnostics.is_empty(),
        "non-generic recursive typedef should type-check cleanly, got: {diagnostics:#?}"
    );
}
