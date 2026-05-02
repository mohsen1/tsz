//! Locks in TS2345 display for arguments narrowed via `instanceof` and for
//! generic call parameters whose type parameters fall back to `unknown`.
//!
//! Structural rules covered here:
//!
//! 1. When a call argument is a flow-narrowed identifier whose narrowed type
//!    is a strict subset of the declared union's members, the argument display
//!    in TS2345 must use the narrowed type — not the declared union — even when
//!    one of the eliminated union members is structurally assignable to the
//!    surviving member (which happens for class identity vs empty-class
//!    structural compatibility, e.g. `class A { private a } | class B {}`
//!    after `if (x instanceof B)`).
//!
//! 2. When a generic call's type parameter is unconstrained and inference
//!    fails to bind it, the parameter type displayed in TS2345 must show the
//!    substituted form (`A<unknown>`) rather than the raw signature form
//!    (`A<T>`) — matching tsc which surfaces the inferred fallback explicitly.
//!
//! Both rules are exercised by the conformance test
//! `narrowingGenericTypeFromInstanceof01.ts`. These unit tests pin the rules
//! independently so that future refactors of the diagnostic source-display
//! and call-finalize layers cannot regress them silently.

use tsz_binder::BinderState;
use tsz_checker::CheckerState;
use tsz_checker::context::CheckerOptions;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn diagnostic_messages(source: &str) -> Vec<(u32, String)> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );

    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

/// Non-generic shape: `A | B` narrowed by `instanceof B` to `B`. `A` has a
/// private brand and `B` is empty, so `A` is structurally assignable to `B`,
/// which previously caused the assignability-only narrower check to treat
/// `B` as not strictly narrower than `A | B` and leak the declared union
/// into the diagnostic source display.
#[test]
fn ts2345_instanceof_narrowed_union_argument_display_uses_narrowed_member() {
    let src = r#"
class A { constructor(private a: string) { } }
class B { }
function acceptA(a: A) { }
function test(x: A | B) {
    if (x instanceof B) {
        acceptA(x);
    }
}
"#;
    let diags = diagnostic_messages(src);
    let ts2345 = diags
        .iter()
        .find(|(code, _)| *code == 2345)
        .expect("expected TS2345 for acceptA(x) under instanceof B narrowing");
    assert!(
        ts2345.1.contains("Argument of type 'B'"),
        "TS2345 argument should display the narrowed 'B', got: {}",
        ts2345.1
    );
    assert!(
        !ts2345.1.contains("'A | B'"),
        "TS2345 must not display the pre-narrowing union 'A | B', got: {}",
        ts2345.1
    );
}

/// Same structural shape, generic version. Narrowing collapses
/// `A<T> | B<T>` to `B<T>` and the unbound type parameter on `acceptA<U>`
/// must default to `unknown` in the parameter display.
#[test]
fn ts2345_instanceof_narrowed_generic_union_argument_and_parameter_display() {
    let src = r#"
class A<T> { constructor(private a: string) { } }
class B<T> { }
function acceptA<T>(a: A<T>) { }
function test<T>(x: A<T> | B<T>) {
    if (x instanceof B) {
        acceptA(x);
    }
}
"#;
    let diags = diagnostic_messages(src);
    let ts2345 = diags
        .iter()
        .find(|(code, _)| *code == 2345)
        .expect("expected TS2345 for acceptA(x) under generic instanceof B narrowing");
    assert!(
        ts2345.1.contains("Argument of type 'B<T>'"),
        "TS2345 argument should display the narrowed 'B<T>', got: {}",
        ts2345.1
    );
    assert!(
        ts2345.1.contains("parameter of type 'A<unknown>'"),
        "TS2345 parameter should default unbound T to 'unknown', got: {}",
        ts2345.1
    );
    assert!(
        !ts2345.1.contains("'A<T> | B<T>'") && !ts2345.1.contains("'A<unknown> | B<T>'"),
        "TS2345 must not display the pre-narrowing union, got: {}",
        ts2345.1
    );
}

/// The same generic test with renamed iteration variables. Locks the fix
/// against the anti-pattern of hardcoding identifier names: the rule is
/// structural (strict subset of the declared union's members; unconstrained
/// type parameter defaults to `unknown`), not name-based.
#[test]
fn ts2345_instanceof_narrowed_generic_union_robust_against_renaming() {
    let src = r#"
class Alpha<X> { constructor(private a: string) { } }
class Beta<Y> { }
function take<Z>(a: Alpha<Z>) { }
function dispatch<W>(value: Alpha<W> | Beta<W>) {
    if (value instanceof Beta) {
        take(value);
    }
}
"#;
    let diags = diagnostic_messages(src);
    let ts2345 = diags
        .iter()
        .find(|(code, _)| *code == 2345)
        .expect("expected TS2345 for take(value) under instanceof Beta narrowing");
    assert!(
        ts2345.1.contains("Argument of type 'Beta<W>'"),
        "TS2345 argument should display the narrowed 'Beta<W>', got: {}",
        ts2345.1
    );
    assert!(
        ts2345.1.contains("parameter of type 'Alpha<unknown>'"),
        "TS2345 parameter should default unbound Z to 'unknown', got: {}",
        ts2345.1
    );
}
