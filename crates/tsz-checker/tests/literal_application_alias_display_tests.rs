//! Tests for tsc-style display of generic type-alias applications in TS2345
//! ("Argument of type 'X' is not assignable to parameter of type 'Y'.").
//!
//! When a generic type-alias application (e.g. `KeysExtendedBy<M, number>`)
//! reduces to a literal/primitive form (e.g. `"b"`), tsc drops the alias
//! name and shows the resolved form in the parameter slot. Object/interface
//! results keep the alias form. Before the fix, the call-parameter
//! formatter unconditionally printed the unevaluated `Application` as
//! `KeysExtendedBy<M, number>`, regardless of what it reduced to.
//!
//! Conformance test touched by this fix:
//! - `mappedTypeAsClauses.ts` (the `f("a")` line at the bottom of the file).
//!
//! Structural rule: when an `Application(alias, args)` is the parameter type
//! at a call site, evaluate it via the type environment; if the result is a
//! literal/primitive (or a union/intersection of those), display the
//! resolved form. Otherwise, keep the alias.

use crate::context::CheckerOptions;
use crate::state::CheckerState;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn check_strict(source: &str) -> Vec<(u32, String)> {
    let options = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        no_implicit_any: true,
        ..Default::default()
    };
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
        options,
    );
    checker.ctx.set_lib_contexts(Vec::new());
    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

/// `KeysExtendedBy<M, number>` evaluates to the literal `"b"`. tsc drops
/// the alias and shows `'"b"'` in the TS2345 parameter slot.
///
/// Repro from `mappedTypeAsClauses.ts` (#44019).
#[test]
fn ts2345_keys_extended_by_alias_resolves_to_literal_in_param_display() {
    let source = r#"
interface M {
    a: boolean;
    b: number;
}
type KeysExtendedBy<T, U> = keyof { [K in keyof T as U extends T[K] ? K : never] : T[K] };
function f(x: KeysExtendedBy<M, number>) {
    return x;
}
f("a");
"#;
    let diags = check_strict(source);
    let ts2345: Vec<&(u32, String)> = diags.iter().filter(|(c, _)| *c == 2345).collect();
    assert_eq!(
        ts2345.len(),
        1,
        "expected exactly one TS2345 for f(\"a\"); got: {diags:?}"
    );
    let msg = &ts2345[0].1;
    assert!(
        msg.contains("parameter of type '\"b\"'"),
        "TS2345 must show the resolved literal '\"b\"' in the parameter slot, \
         not the unevaluated alias 'KeysExtendedBy<M, number>'. Got: {msg:?}"
    );
    assert!(
        !msg.contains("KeysExtendedBy"),
        "TS2345 must drop the alias name when it reduces to a literal. \
         Got: {msg:?}"
    );
}

/// A generic indexed-access alias `Get<T, K> = T[K]` instantiated with
/// concrete args reduces to a literal (`1`). tsc shows the literal.
#[test]
fn ts2345_generic_indexed_access_alias_resolves_to_literal_in_param_display() {
    let source = r#"
type Get<T, K extends keyof T> = T[K];
function f(x: Get<{a: 1, b: 2}, "a">) {}
f(2);
"#;
    let diags = check_strict(source);
    let ts2345: Vec<&(u32, String)> = diags.iter().filter(|(c, _)| *c == 2345).collect();
    assert_eq!(
        ts2345.len(),
        1,
        "expected exactly one TS2345 for f(2); got: {diags:?}"
    );
    let msg = &ts2345[0].1;
    assert!(
        msg.contains("parameter of type '1'"),
        "TS2345 must show the resolved literal '1' in the parameter slot, \
         not 'Get<{{ a: 1; b: 2; }}, \"a\">'. Got: {msg:?}"
    );
    assert!(
        !msg.contains("Get<"),
        "TS2345 must drop the 'Get' alias when it reduces to a literal. \
         Got: {msg:?}"
    );
}

/// An alias whose body produces a union of literals (e.g. `keyof Mapped`
/// where the mapped result has multiple keys) shows the resolved union.
#[test]
fn ts2345_alias_resolving_to_union_of_literals_shows_resolved_form() {
    let source = r#"
type Keys<T> = keyof T;
function f(x: Keys<{ a: 1; b: 2 }>) {}
f("c");
"#;
    let diags = check_strict(source);
    let ts2345: Vec<&(u32, String)> = diags.iter().filter(|(c, _)| *c == 2345).collect();
    assert_eq!(
        ts2345.len(),
        1,
        "expected exactly one TS2345 for f(\"c\"); got: {diags:?}"
    );
    let msg = &ts2345[0].1;
    assert!(
        msg.contains('"') && msg.contains('a') && msg.contains('b'),
        "TS2345 should mention the resolved literal members. Got: {msg:?}"
    );
}

/// Alias whose body reduces to an *object* shape keeps its alias form
/// (matching tsc behaviour for `Pick`, `Partial`, `Record`, etc.).
///
/// This is the negative-rule guard: the fix must not over-eagerly expand
/// every Application — only when the result is literal/primitive.
#[test]
fn ts2345_alias_resolving_to_object_keeps_alias_form() {
    let source = r#"
type Pick2<T, K extends keyof T> = { [P in K]: T[P] };
function f(x: Pick2<{a: 1, b: 2}, "a">) {}
f(2 as any as number);
"#;
    let diags = check_strict(source);
    let ts2345: Vec<&(u32, String)> = diags.iter().filter(|(c, _)| *c == 2345).collect();
    assert_eq!(
        ts2345.len(),
        1,
        "expected exactly one TS2345 for f(number); got: {diags:?}"
    );
    let msg = &ts2345[0].1;
    assert!(
        msg.contains("Pick2<"),
        "TS2345 should keep the 'Pick2<...>' alias form when the alias body \
         reduces to an object shape (not a literal/primitive). Got: {msg:?}"
    );
}

/// Verify that bound-variable name choice in the alias body does not affect
/// the rule. `K`, `P`, `X` should all behave identically.
///
/// This guards against the §25 anti-hardcoding rule: the fix must operate on
/// the structural shape of the result, not on user-chosen identifiers in the
/// printed alias body.
#[test]
fn ts2345_alias_resolves_independent_of_iteration_variable_name() {
    for var in ["K", "P", "X"] {
        let source = format!(
            r#"
interface M {{ a: boolean; b: number; }}
type KeysExtendedBy<T, U> = keyof {{ [{var} in keyof T as U extends T[{var}] ? {var} : never]: T[{var}] }};
function f(x: KeysExtendedBy<M, number>) {{ return x; }}
f("a");
"#,
        );
        let diags = check_strict(&source);
        let ts2345: Vec<&(u32, String)> = diags.iter().filter(|(c, _)| *c == 2345).collect();
        assert_eq!(
            ts2345.len(),
            1,
            "expected exactly one TS2345 with var={var}; got: {diags:?}"
        );
        let msg = &ts2345[0].1;
        assert!(
            msg.contains("parameter of type '\"b\"'"),
            "fix must work regardless of iteration variable name '{var}'. \
             Got: {msg:?}"
        );
        assert!(
            !msg.contains("KeysExtendedBy"),
            "alias must be dropped regardless of var name '{var}'. Got: {msg:?}"
        );
    }
}
