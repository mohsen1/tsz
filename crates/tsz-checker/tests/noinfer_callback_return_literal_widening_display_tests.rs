//! A fresh literal returned from a callback whose contextual return type is a
//! `NoInfer<T>` over an *inferred* type parameter is widened by tsc's
//! `getReturnTypeFromBody` while `T` is unfixed, so the TS2322 return-mismatch
//! renders the widened base type (`string`, `number`, `boolean`) as the source.
//! An *explicit* type argument never reaches that unfixed phase, so an explicit
//! `NoInfer<"foo">` keeps the literal spelling. A plain concrete literal
//! contextual return (no `NoInfer`) also keeps the literal. See #17501.

use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::diagnostic_codes;

fn strict_messages(source: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_source(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|diagnostic| (diagnostic.code, diagnostic.message_text))
    .collect()
}

fn ts2322_messages(source: &str) -> Vec<String> {
    strict_messages(source)
        .into_iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .map(|(_, message)| message)
        .collect()
}

#[test]
fn inferred_noinfer_callback_return_literal_widens_to_string() {
    let messages = ts2322_messages(
        r#"
declare function f<T>(cb: () => NoInfer<T>, v: T): T;
const r = f(() => "bar", "foo");
"#,
    );
    assert_eq!(messages.len(), 1, "expected one TS2322, got {messages:?}");
    assert_eq!(
        messages[0], "Type 'string' is not assignable to type '\"foo\"'.",
        "inferred NoInfer callback return should widen the fresh literal to its base"
    );
}

#[test]
fn inferred_noinfer_callback_return_number_widens_to_number() {
    let messages = ts2322_messages(
        r#"
declare function f<T>(cb: () => NoInfer<T>, v: T): T;
const r = f(() => 1, 2);
"#,
    );
    assert_eq!(messages.len(), 1, "expected one TS2322, got {messages:?}");
    assert_eq!(
        messages[0], "Type 'number' is not assignable to type '2'.",
        "a numeric fresh return literal widens to `number`"
    );
}

#[test]
fn inferred_noinfer_callback_return_boolean_widens_to_boolean() {
    let messages = ts2322_messages(
        r#"
declare function f<T>(cb: () => NoInfer<T>, v: T): T;
const r = f(() => true, false);
"#,
    );
    assert_eq!(messages.len(), 1, "expected one TS2322, got {messages:?}");
    assert_eq!(
        messages[0], "Type 'boolean' is not assignable to type 'false'.",
        "a boolean fresh return literal widens to `boolean`"
    );
}

#[test]
fn inferred_noinfer_callback_return_widen_is_binder_name_independent() {
    // The widen keys on the structural NoInfer-over-inferred-parameter shape, not
    // on any particular type-parameter, function, or parameter identifier.
    let messages = ts2322_messages(
        r#"
declare function combine<Elem>(make: () => NoInfer<Elem>, seed: Elem): Elem;
const q = combine(() => "zzz", "yyy");
"#,
    );
    assert_eq!(messages.len(), 1, "expected one TS2322, got {messages:?}");
    assert_eq!(
        messages[0], "Type 'string' is not assignable to type '\"yyy\"'.",
        "renamed binders must produce the same widened source display"
    );
}

#[test]
fn inferred_noinfer_callback_return_widen_survives_swapped_argument_order() {
    // The value argument that fixes `T` comes before the callback; the widen must
    // still apply regardless of positional order.
    let messages = ts2322_messages(
        r#"
declare function f<T>(v: T, cb: () => NoInfer<T>): T;
const r = f("foo", () => "bar");
"#,
    );
    assert_eq!(messages.len(), 1, "expected one TS2322, got {messages:?}");
    assert_eq!(
        messages[0], "Type 'string' is not assignable to type '\"foo\"'.",
        "argument order must not change the widened source display"
    );
}

#[test]
fn explicit_noinfer_type_argument_keeps_the_literal_spelling() {
    // With `T` supplied explicitly there is no unfixed inference phase, so tsc
    // (and tsz) keep the fresh literal `"bar"` rather than widening it.
    let messages = ts2322_messages(
        r#"
declare function f<T>(cb: () => NoInfer<T>, v: T): T;
const r = f<"foo">(() => "bar", "foo");
"#,
    );
    assert_eq!(messages.len(), 1, "expected one TS2322, got {messages:?}");
    assert_eq!(
        messages[0], "Type '\"bar\"' is not assignable to type '\"foo\"'.",
        "an explicit NoInfer type argument keeps the literal source display"
    );
}

#[test]
fn concrete_literal_contextual_return_keeps_the_literal_spelling() {
    // A concrete literal contextual return (no `NoInfer`, no inference) is
    // literal-preferring, so the fresh return literal is preserved.
    let messages = ts2322_messages(
        r#"
declare function h(cb: () => "foo"): void;
h(() => "bar");
"#,
    );
    assert_eq!(messages.len(), 1, "expected one TS2322, got {messages:?}");
    assert_eq!(
        messages[0], "Type '\"bar\"' is not assignable to type '\"foo\"'.",
        "a concrete literal contextual return keeps the literal source display"
    );
}
