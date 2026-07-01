//! An expression-bodied arrow whose body is a conditional
//! (`() => cond ? a : b`) passed as a generic-call argument must anchor the
//! return-type mismatch at the conditional expression with `TS2322`, mirroring
//! `tsc`'s `elaborateArrowFunction`.
//!
//! Regression: `tsz` reported the coarse whole-callback `TS2345`
//! (`Argument of type '() => …[] | …[]' is not assignable to parameter of type
//! '() => …[]'`) at the arrow instead of drilling into the body. The
//! elaborator's `CONDITIONAL_EXPRESSION` body arm returned `false` and deferred
//! to the argument-level diagnostic, which never drilled a conditional. Now the
//! conditional body routes through the same expression-body elaboration as a
//! simple return expression, so the diagnostic lands on the conditional with
//! the union-of-branches source type — the shape `tsc` emits.
//!
//! Witness (`inferFromGenericFunctionReturnTypes3`, `#14141`):
//! ```ts
//! declare function bar<T>(f: () => T[]): T[];
//! let x: Foo[] = bar(() => !!true ? [{ state: State.A }] : [{ state: State.B }]);
//! //                       ^ tsc: TS2322 here, not TS2345 on the whole arrow
//! ```

use tsz_checker::test_utils::check_source_diagnostics;

const CALL_ARG_CONDITIONAL: &str = "enum State { A, B }\n\
     type Foo = { state: State }\n\
     declare function bar<T>(f: () => T[]): T[];\n\
     let x: Foo[] = bar(() => !!true ? [{ state: State.A }] : [{ state: State.B }]);\n";

/// The single diagnostic is a `TS2322` anchored at the conditional expression
/// with the union-of-branches source type and the inferred single-array target
/// — never the whole-callback `TS2345`.
#[test]
fn call_argument_conditional_body_anchors_ts2322_at_conditional() {
    let diagnostics = check_source_diagnostics(CALL_ARG_CONDITIONAL);
    assert_eq!(
        diagnostics.len(),
        1,
        "expected exactly one diagnostic, got {diagnostics:#?}"
    );
    let diagnostic = &diagnostics[0];
    assert_eq!(
        diagnostic.code, 2322,
        "return mismatch must drill to TS2322, not the coarse whole-callback TS2345: {diagnostic:#?}"
    );
    assert_eq!(
        diagnostic.message_text,
        "Type '{ state: State.A; }[] | { state: State.B; }[]' is not assignable to type '{ state: State.A; }[]'.",
        "source must be the union of both branches, target the inferred single-array element"
    );

    // Anchored at the conditional expression, not the arrow function.
    let conditional_start = CALL_ARG_CONDITIONAL
        .find("!!true")
        .expect("fixture contains the conditional") as u32;
    assert_eq!(
        diagnostic.start, conditional_start,
        "diagnostic must anchor at the conditional expression"
    );
}

/// The drill-in is structural, not keyed on any identifier: varying the enum,
/// the alias, the type parameter, and the call name still yields the same
/// TS2322-at-the-conditional shape.
#[test]
fn renamed_binders_still_anchor_ts2322_at_conditional() {
    let source = "enum Mode { On, Off }\n\
         type Cfg = { mode: Mode }\n\
         declare function pick<U>(g: () => U[]): U[];\n\
         let y: Cfg[] = pick(() => !!true ? [{ mode: Mode.On }] : [{ mode: Mode.Off }]);\n";
    let diagnostics = check_source_diagnostics(source);
    assert_eq!(
        diagnostics.len(),
        1,
        "expected exactly one diagnostic, got {diagnostics:#?}"
    );
    let diagnostic = &diagnostics[0];
    assert_eq!(diagnostic.code, 2322, "{diagnostic:#?}");
    assert_eq!(
        diagnostic.message_text,
        "Type '{ mode: Mode.On; }[] | { mode: Mode.Off; }[]' is not assignable to type '{ mode: Mode.On; }[]'.",
        "{diagnostic:#?}"
    );
    let conditional_start = source.find("!!true").expect("conditional") as u32;
    assert_eq!(diagnostic.start, conditional_start, "{diagnostic:#?}");
}

/// A parenthesized conditional body (`() => (cond ? a : b)`) still drills to a
/// TS2322, anchored — as `tsc` reports it — at the whole parenthesized
/// expression (the opening paren), not the coarse whole-callback TS2345.
#[test]
fn parenthesized_conditional_body_anchors_ts2322_at_parenthesized_expr() {
    let source = "enum State { A, B }\n\
         type Foo = { state: State }\n\
         declare function bar<T>(f: () => T[]): T[];\n\
         let x: Foo[] = bar(() => (!!true ? [{ state: State.A }] : [{ state: State.B }]));\n";
    let diagnostics = check_source_diagnostics(source);
    assert_eq!(
        diagnostics.len(),
        1,
        "expected exactly one diagnostic, got {diagnostics:#?}"
    );
    let diagnostic = &diagnostics[0];
    assert_eq!(diagnostic.code, 2322, "{diagnostic:#?}");
    assert_eq!(
        diagnostic.message_text,
        "Type '{ state: State.A; }[] | { state: State.B; }[]' is not assignable to type '{ state: State.A; }[]'.",
        "{diagnostic:#?}"
    );
    // Anchored at the parenthesized expression (the opening paren).
    let paren_start = source.find("(!!true").expect("parenthesized conditional") as u32;
    assert_eq!(diagnostic.start, paren_start, "{diagnostic:#?}");
}

/// A conditional body whose union type *is* assignable to the inferred return
/// element produces no error — the elaboration only fires on a genuine
/// mismatch and never manufactures a false positive.
#[test]
fn assignable_conditional_body_produces_no_error() {
    let source = "enum State { A, B }\n\
         type Foo = { state: State }\n\
         declare function bar<T>(f: () => T[]): T[];\n\
         let x: Foo[] = bar(() => !!true ? [{ state: State.A }] : [{ state: State.A }]);\n";
    let diagnostics = check_source_diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "assignable conditional body must not error, got {diagnostics:#?}"
    );
}

/// `tsc`'s `elaborateArrowFunction` gate keys on parameter annotations: when a
/// callback parameter is explicitly annotated, the mismatch stays at the
/// argument level (`TS2345`) rather than drilling into the body.
#[test]
fn explicit_param_annotation_keeps_argument_level_ts2345() {
    let source = "enum State { A, B }\n\
         type Foo = { state: State }\n\
         declare function bar<T>(f: (n: number) => T[]): T[];\n\
         let x: Foo[] = bar((n: number) => !!true ? [{ state: State.A }] : [{ state: State.B }]);\n";
    let diagnostics = check_source_diagnostics(source);
    assert_eq!(
        diagnostics.len(),
        1,
        "expected exactly one diagnostic, got {diagnostics:#?}"
    );
    let diagnostic = &diagnostics[0];
    assert_eq!(
        diagnostic.code, 2345,
        "an explicitly-annotated callback parameter keeps the argument-level TS2345: {diagnostic:#?}"
    );
    assert!(
        !diagnostic.message_text.is_empty(),
        "TS2345 carries the callback source/target message"
    );
}
