//! Expression-bodied arrow return-mismatch elaboration when the body's own
//! type is callable.
//!
//! `tsc`'s `elaborateArrowFunction` anchors an expression-bodied arrow's
//! return-type mismatch at the *body expression*, regardless of the body
//! type's shape. It bails only for block bodies and for arrows whose
//! parameters carry explicit type annotations. `tsz` used to add an extra
//! special case: when the body's type was itself callable but the expected
//! return type was not, it skipped the body-level drill and fell back to the
//! whole-function `TS2322` (`Type '() => () => string' is not assignable to
//! type '() => number'.`). That callable-body carve-out has no `tsc` analog —
//! `tsc` reports `Type '() => string' is not assignable to type 'number'.`
//! anchored at the body — so it is removed here.
//!
//! These tests pin the parity in the variable-initializer, call-argument,
//! parameter, and call-expression-body forms, and guard the surrounding
//! behavior that must stay function-level (block bodies, annotated params) or
//! error-free (callable body assignable to the expected return type).

use tsz_checker::test_utils::{check_source_strict, check_source_strict_messages};

fn strict_ts2322(source: &str) -> Vec<String> {
    check_source_strict_messages(source)
        .into_iter()
        .filter(|(code, _)| *code == 2322)
        .map(|(_, message)| message)
        .collect()
}

/// Assert that `source` produces exactly one body-level `TS2322` whose message
/// is the return-vs-body mismatch (`Type '<body>' is not assignable to type
/// '<return>'.`), i.e. the arrow drilled into the body instead of falling back
/// to the whole-function message.
fn assert_body_drill(source: &str, expected_message: &str) {
    let messages = strict_ts2322(source);
    assert_eq!(
        messages,
        vec![expected_message.to_string()],
        "expected the body-level return mismatch, got: {messages:?}"
    );
}

/// `const binding: () => number = () => callable` anchors the mismatch at the
/// body (`producer`) with the body-vs-return message, not the whole-arrow
/// function-type message — and at the body's *position*, not the variable name.
/// The original divergence was both message and position.
#[test]
fn variable_initializer_callable_identifier_body_anchors_at_body() {
    let source = "declare const producer: () => string;\n\
         const consumer: () => number = () => producer;\n";
    assert_body_drill(
        source,
        "Type '() => string' is not assignable to type 'number'.",
    );

    let diagnostics = check_source_strict(source);
    let ts2322: Vec<_> = diagnostics.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(ts2322.len(), 1, "expected one TS2322, got: {diagnostics:?}");
    // The body `producer` is the last occurrence of the identifier in the
    // source (the first is its `declare const` binder).
    let body_offset = source.rfind("producer").expect("body identifier present");
    assert_eq!(
        ts2322[0].start as usize, body_offset,
        "TS2322 must anchor at the body expression `producer`, got start {}",
        ts2322[0].start
    );
}

/// A call-argument callback with a callable identifier body drills the same
/// way — `tsc` reports `TS2322` at the body, never the whole-callback `TS2345`.
#[test]
fn call_argument_callable_identifier_body_anchors_at_body() {
    assert_body_drill(
        "declare const producer: () => string;\n\
         declare function run(cb: () => number): void;\n\
         run(() => producer);\n",
        "Type '() => string' is not assignable to type 'number'.",
    );
}

/// A call-expression body whose result type is callable also drills to the
/// body.
#[test]
fn variable_initializer_callable_call_body_anchors_at_body() {
    assert_body_drill(
        "declare function make(): () => string;\n\
         const consumer: () => number = () => make();\n",
        "Type '() => string' is not assignable to type 'number'.",
    );
}

/// An unannotated parameter does not disqualify the drill (only *annotated*
/// parameters do, matching `tsc`'s `elaborateArrowFunction` parameter gate).
#[test]
fn unannotated_param_callable_body_still_anchors_at_body() {
    assert_body_drill(
        "declare const producer: () => string;\n\
         const consumer: (n: number) => number = (n) => producer;\n",
        "Type '() => string' is not assignable to type 'number'.",
    );
}

/// A property-access body whose type is callable (a method member) drills to
/// the body. Uses a user-defined method so the assertion does not depend on the
/// exact lib signature of a built-in.
#[test]
fn property_access_callable_body_anchors_at_body() {
    assert_body_drill(
        "declare const holder: { method: () => string };\n\
         const consumer: (n: number) => number = (n) => holder.method;\n",
        "Type '() => string' is not assignable to type 'number'.",
    );
}

/// Regression: a primitive body still drills (this always worked and must keep
/// working).
#[test]
fn primitive_body_still_anchors_at_body() {
    assert_body_drill(
        "const consumer: () => number = () => \"x\";\n",
        "Type 'string' is not assignable to type 'number'.",
    );
}

/// Regression: `tsc`'s `elaborateArrowFunction` never drills a *block* body, so
/// a block-bodied arrow keeps the whole-function message.
#[test]
fn block_body_stays_function_level() {
    let messages = strict_ts2322(
        "declare const producer: () => string;\n\
         const consumer: () => number = () => { return producer; };\n",
    );
    assert_eq!(
        messages,
        vec!["Type '() => () => string' is not assignable to type '() => number'.".to_string()],
        "block-bodied arrows must stay function-level, got: {messages:?}"
    );
}

/// Regression: an *annotated* parameter disqualifies the body drill, so the
/// message stays function-level (matching `tsc`'s parameter gate).
#[test]
fn annotated_param_stays_function_level() {
    let messages = strict_ts2322(
        "declare const producer: () => string;\n\
         const consumer: (n: number) => number = (n: number) => producer;\n",
    );
    assert_eq!(
        messages,
        vec![
            "Type '(n: number) => () => string' is not assignable to type '(n: number) => number'."
                .to_string()
        ],
        "annotated-param arrows must stay function-level, got: {messages:?}"
    );
}

/// Regression: a callable body that *is* assignable to the expected return type
/// (`Function`, `unknown`, `object`) produces no diagnostic — the drill only
/// fires on a genuine mismatch.
#[test]
fn callable_body_assignable_to_expected_return_is_ok() {
    let messages = strict_ts2322(
        "declare const producer: () => string;\n\
         const a: () => Function = () => producer;\n\
         const b: () => unknown = () => producer;\n\
         const c: () => object = () => producer;\n",
    );
    assert!(
        messages.is_empty(),
        "callable body assignable to the expected return must not error, got: {messages:?}"
    );
}
