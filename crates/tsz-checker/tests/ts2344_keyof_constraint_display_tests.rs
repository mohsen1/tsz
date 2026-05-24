//! Regression tests for #9704: a failing `K extends keyof O` constraint must
//! render the constraint as `keyof <typeArgument>` (matching tsc), not as the
//! eagerly-evaluated literal key union.
//!
//! The structural rule: when a type argument fails a `keyof`-of-type-parameter
//! constraint, tsc substitutes the type *argument* (preserving its reference
//! name) into the constraint and keeps the `keyof` operator un-evaluated for
//! display. tsz used to substitute the evaluated alias body, collapsing
//! `keyof T` to `"foo" | "bar"`.

use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_checker::test_utils::diagnostic_code_messages;
use tsz_parser::parser::ParserState;
use tsz_solver::construction::TypeInterner;

fn compile_and_get_diagnostics(source: &str) -> Vec<(u32, String)> {
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

    diagnostic_code_messages(checker.ctx.diagnostics)
}

fn ts2344_messages(source: &str) -> Vec<String> {
    compile_and_get_diagnostics(source)
        .into_iter()
        .filter(|(code, _)| *code == 2344)
        .map(|(_, msg)| msg)
        .collect()
}

#[test]
fn keyof_constraint_displays_keyof_of_named_alias_argument() {
    let messages = ts2344_messages(
        r#"
type MyPick<O, K extends keyof O> = { [P in K]: O[P] };
type T = { foo: 1; bar: 2 };
type Bad = MyPick<T, boolean>;
"#,
    );
    assert_eq!(messages.len(), 1, "expected one TS2344, got: {messages:?}");
    assert!(
        messages[0].contains("keyof T"),
        "constraint should display as `keyof T`, got: {:?}",
        messages[0]
    );
    assert!(
        !messages[0].contains("\"foo\""),
        "constraint must not expand to the literal key union, got: {:?}",
        messages[0]
    );
}

#[test]
fn keyof_constraint_uses_argument_name_not_parameter_name() {
    // The constraint type parameter is `Sel extends keyof Obj`; the argument is
    // `Data`. tsc renders `keyof Data` (the argument name), not `keyof Obj`.
    let messages = ts2344_messages(
        r#"
type MyPick<Obj, Sel extends keyof Obj> = { [P in Sel]: Obj[P] };
type Data = { a: 1; b: 2 };
type Bad = MyPick<Data, boolean>;
"#,
    );
    assert_eq!(messages.len(), 1, "expected one TS2344, got: {messages:?}");
    assert!(
        messages[0].contains("keyof Data"),
        "constraint should display as `keyof Data`, got: {:?}",
        messages[0]
    );
    assert!(
        !messages[0].contains("\"a\""),
        "constraint must not expand to the literal key union, got: {:?}",
        messages[0]
    );
}

#[test]
fn keyof_constraint_preserves_interface_argument_name() {
    let messages = ts2344_messages(
        r#"
type MyPick<O, K extends keyof O> = { [P in K]: O[P] };
interface I { a: 1; b: 2 }
type Bad = MyPick<I, boolean>;
"#,
    );
    assert_eq!(messages.len(), 1, "expected one TS2344, got: {messages:?}");
    assert!(
        messages[0].contains("keyof I"),
        "constraint should display as `keyof I`, got: {:?}",
        messages[0]
    );
}

#[test]
fn inline_anonymous_arg_does_not_borrow_sibling_alias_name() {
    // Regression for the structural-interning hazard: an inline anonymous
    // object argument shares a `TypeId` with the same-shape alias `T`. The
    // constraint display must NOT recover `keyof T` here — the user wrote no
    // `T` reference, so the recovery must be gated on the written AST node
    // being a type reference, not on the shared structural `TypeId`.
    let messages = ts2344_messages(
        r#"
type MyPick<O, K extends keyof O> = { [P in K]: O[P] };
type T = { foo: 1; bar: 2 };
type Bad = MyPick<{ foo: 1; bar: 2 }, boolean>;
"#,
    );
    assert_eq!(messages.len(), 1, "expected one TS2344, got: {messages:?}");
    assert!(
        !messages[0].contains("keyof T"),
        "inline anonymous arg must not borrow the sibling alias name `T`, got: {:?}",
        messages[0]
    );
}

#[test]
fn plain_alias_constraint_still_displays_alias_name() {
    // Negative control: a non-`keyof` alias constraint must keep showing the
    // alias name (`Keys`), proving the fix is scoped to operator preservation.
    let messages = ts2344_messages(
        r#"
type Keys = "a" | "b";
type G<K extends Keys> = K;
type Bad = G<boolean>;
"#,
    );
    assert_eq!(messages.len(), 1, "expected one TS2344, got: {messages:?}");
    assert!(
        messages[0].contains("Keys"),
        "plain alias constraint should display `Keys`, got: {:?}",
        messages[0]
    );
}
