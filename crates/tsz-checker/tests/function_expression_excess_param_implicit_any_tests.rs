//! Implicit-`any` (TS7006) for function-expression parameters that an enclosing
//! variable-declaration annotation cannot contextually type.
//!
//! Structural rule: when a function expression is the initializer of a variable
//! declaration with an explicit type annotation, the annotation only supplies a
//! contextual type for a parameter it actually covers. tsc applies contextual
//! parameter typing exactly as follows:
//!
//!   * if the function expression declares more *required* parameters than the
//!     annotated call signature accepts (and the signature has no rest
//!     parameter), contextual typing is discarded for *every* parameter; or
//!   * otherwise, contextual typing is positional, so a parameter whose position
//!     is beyond the signature's parameter count (e.g. a trailing optional
//!     parameter the signature does not declare) receives no contextual type.
//!
//! In both cases the uncovered parameters are implicit `any` and must raise
//! TS7006 under `noImplicitAny`. tsz previously deferred (and so dropped) that
//! diagnostic for any such initializer whose annotation resolved to a named
//! type, even though sibling contexts (call arguments, object-literal methods,
//! assignments, `as`/`satisfies`, array elements) already reported it.
//!
//! Every case varies its binder names (type alias, variable, and parameter
//! identifiers) so the behavior cannot be keyed to a particular spelling.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

fn strict_options() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        ..Default::default()
    }
}

fn count_7006(source: &str) -> usize {
    check_source(source, "test.ts", strict_options())
        .iter()
        .filter(|d| d.code == 7006)
        .count()
}

#[test]
fn excess_required_params_over_annotation_are_implicit_any() {
    // The signature `Handler` accepts one parameter; the initializer declares two
    // required parameters, so the arity mismatch discards contextual typing for
    // both — tsc emits TS7006 for `head` and `tail`.
    let source = r#"
        type Handler = (head: string) => void;
        const onEvent: Handler = (head, tail) => {};
    "#;
    assert_eq!(count_7006(source), 2);
}

#[test]
fn excess_required_params_renamed_binders_still_implicit_any() {
    // Same shape, entirely different identifiers — proves the result is not keyed
    // to any particular name.
    let source = r#"
        type Sink = (alpha: number) => void;
        const drain: Sink = (omega, kappa) => {};
    "#;
    assert_eq!(count_7006(source), 2);
}

#[test]
fn exact_arity_params_are_contextually_typed() {
    // The initializer's required arity matches the signature, so both parameters
    // are contextually typed and there is no TS7006.
    let source = r#"
        type Pair = (left: string, right: number) => void;
        const combine: Pair = (left, right) => {};
    "#;
    assert_eq!(count_7006(source), 0);
}

#[test]
fn fewer_params_than_annotation_are_contextually_typed() {
    // Declaring fewer parameters than the signature is allowed and the declared
    // parameter is still contextually typed — no TS7006.
    let source = r#"
        type Pair = (left: string, right: number) => void;
        const ignoreSecond: Pair = (left) => {};
    "#;
    assert_eq!(count_7006(source), 0);
}

#[test]
fn trailing_optional_param_beyond_annotation_is_implicit_any() {
    // The required arity (one) is accepted, so `first` is contextually typed, but
    // the trailing optional `second?` sits beyond the signature's single
    // parameter and receives no contextual type: exactly one TS7006 (`second`).
    let source = r#"
        type Notify = (first: string) => void;
        const ping: Notify = (first, second?) => {};
    "#;
    assert_eq!(count_7006(source), 1);
}

#[test]
fn rest_parameter_in_annotation_covers_all_positions() {
    // A rest parameter in the annotated signature contextually types every
    // positional parameter of the initializer — no TS7006.
    let source = r#"
        type Variadic = (...items: string[]) => void;
        const collect: Variadic = (first, second) => {};
    "#;
    assert_eq!(count_7006(source), 0);
}

#[test]
fn inline_function_type_annotation_excess_params_are_implicit_any() {
    // The annotation is an inline function type rather than a named alias; the
    // arity-exceeding parameters are still implicit `any`.
    let source = r#"
        const route: (path: string) => void = (path, handler) => {};
    "#;
    assert_eq!(count_7006(source), 2);
}

#[test]
fn this_parameter_does_not_count_toward_excess_arity() {
    // A leading `this` parameter is not a value parameter: the single value
    // parameter `value` matches the signature's one value parameter, so there is
    // no TS7006.
    let source = r#"
        type Bound = (this: { tag: string }, value: number) => void;
        const apply: Bound = function (value) {};
    "#;
    assert_eq!(count_7006(source), 0);
}

#[test]
fn explicitly_typed_excess_params_do_not_report_implicit_any() {
    // Excess parameters that carry their own annotations are not implicit `any`
    // (the arity mismatch is still a TS2322, but no TS7006 is raised).
    let source = r#"
        type Handler = (head: string) => void;
        const onEvent: Handler = (head: string, tail: number) => {};
    "#;
    assert_eq!(count_7006(source), 0);
}
