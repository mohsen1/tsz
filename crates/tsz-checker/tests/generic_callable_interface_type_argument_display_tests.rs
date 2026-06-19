//! A *named generic interface or class whose instance is callable* (it carries
//! a call or construct signature) must keep its `Name<Args>` surface in
//! assignability diagnostics, exactly as `tsc` renders it.
//!
//! Structural rule:
//!   When the source/argument of a `TS2322`/`TS2345` mismatch is an
//!   instantiated named generic interface/class that also has a call (or
//!   construct) signature — e.g. prop-types' `Validator<T>`:
//!   `interface Validator<T> { (x: object): Error | null; [brand]?: T }` —
//!   `tsc` prints `Validator<string>`, not a bare `Validator`.
//!
//! Owner layer: checker diagnostic display
//! (`error_reporter::core::type_display::widen_function_like_display_type`).
//! That helper evaluated the generic application to its structural (callable)
//! instance *before* the generic-application guard ran, so the nominal head and
//! the type arguments were discarded for callable interfaces — non-callable
//! generic interfaces (`Box<string>`) reconstructed `Name<Args>` downstream and
//! were unaffected, which is why only the callable shape regressed. The fix
//! preserves a generic interface/class application before that evaluation; a
//! reducing *type-alias* application (`DeepReadonly<X>`) still drops its name,
//! matching `tsc`.
//!
//! Verified against `tsc` 5.9 (`propTypeValidatorInference.ts` family).
//!
//! Tests deliberately vary the type-parameter and interface identifiers to keep
//! the rule structural, not bound to a particular name (`.claude/CLAUDE.md`
//! anti-hardcoding gate).

use tsz_checker::test_utils::check_source_code_messages as diagnostics;

fn messages(source: &str, code: u32) -> Vec<String> {
    diagnostics(source)
        .into_iter()
        .filter_map(|(c, msg)| (c == code).then_some(msg))
        .collect()
}

fn assert_any_contains(msgs: &[String], needle: &str) {
    assert!(
        msgs.iter().any(|m| m.contains(needle)),
        "expected a message containing {needle:?}, got: {msgs:#?}"
    );
}

fn assert_none_contains(msgs: &[String], needle: &str) {
    assert!(
        !msgs.iter().any(|m| m.contains(needle)),
        "did not expect a message containing {needle:?}, got: {msgs:#?}"
    );
}

/// The witness: a generic callable interface whose type parameter only appears
/// in a non-signature member. Must render `Validator<string>` (was a bare
/// `Validator`).
#[test]
fn generic_callable_interface_assignment_source_keeps_type_argument() {
    let source = r#"
interface Err { message: string }
interface Validator<T> { (x: object): Err; brand?: T; }
declare const v: Validator<string>;
const r: 0 = v;
"#;
    let msgs = messages(source, 2322);
    assert_any_contains(&msgs, "Validator<string>");
    // The bug rendered the args-stripped `'Validator'` token.
    assert_none_contains(&msgs, "Type 'Validator' ");
}

/// Anti-hardcoding: a renamed interface and type parameter behave identically.
#[test]
fn renamed_generic_callable_interface_keeps_type_argument() {
    let source = r#"
interface Failure { reason: string }
interface Checker<Elem> { (input: object): Failure; tag?: Elem; }
declare const c: Checker<number>;
const r: 0 = c;
"#;
    let msgs = messages(source, 2322);
    assert_any_contains(&msgs, "Checker<number>");
    assert_none_contains(&msgs, "Type 'Checker' ");
}

/// Multiple type arguments are all preserved.
#[test]
fn generic_callable_interface_multiple_type_arguments_preserved() {
    let source = r#"
interface Err { message: string }
interface Pair<A, B> { (x: object): Err; a?: A; b?: B; }
declare const p: Pair<string, number>;
const r: 0 = p;
"#;
    let msgs = messages(source, 2322);
    assert_any_contains(&msgs, "Pair<string, number>");
}

/// The same rule applies in `TS2345` argument position.
#[test]
fn generic_callable_interface_call_argument_keeps_type_argument() {
    let source = r#"
interface Err { message: string }
interface Validator<T> { (x: object): Err; brand?: T; }
declare const v: Validator<string>;
function take(n: number): void {}
take(v);
"#;
    let msgs = messages(source, 2345);
    assert_any_contains(&msgs, "Validator<string>");
}

/// Control: an anonymous generic function type has no nominal name, so it must
/// keep rendering its structural signature — the fix must not invent a name.
#[test]
fn anonymous_generic_function_type_stays_structural() {
    let source = r#"
declare const f: <T>(x: T) => T;
const r: 0 = f;
"#;
    let msgs = messages(source, 2322);
    assert_any_contains(&msgs, "<T>(x: T) => T");
}

/// Control: a *non-generic* callable interface has no type arguments to show
/// and must keep rendering its bare name.
#[test]
fn non_generic_callable_interface_unaffected() {
    let source = r#"
interface Err { message: string }
interface Handler { (x: object): Err; tag?: string; }
declare const h: Handler;
const r: 0 = h;
"#;
    let msgs = messages(source, 2322);
    assert_any_contains(&msgs, "Type 'Handler'");
}

/// Control: a non-callable generic interface already rendered `Name<Args>`
/// correctly; guard that the fix leaves it intact.
#[test]
fn non_callable_generic_interface_keeps_type_argument() {
    let source = r#"
interface Box<T> { value?: T; }
declare const b: Box<string>;
const r: 0 = b;
"#;
    let msgs = messages(source, 2322);
    assert_any_contains(&msgs, "Box<string>");
}
