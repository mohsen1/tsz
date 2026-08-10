//! TS1278/TS1279 elaboration + TS6210/TS6236 missing-argument pointer for
//! decorator signature-resolution failures.
//!
//! Structural rule: when a decorator's call resolution fails specifically
//! because of an argument-count mismatch, tsc attaches, beneath the primary
//! TS1238/1239/1240/1241 ("Unable to resolve signature of ... decorator when
//! called as an expression."):
//! - TS1278 ("The runtime will invoke the decorator with {1} arguments, but
//!   the decorator expects {0}.") when the decorator's declared arity has a
//!   concrete upper bound, or TS1279 ("...expects at least {0}.") when too few
//!   arguments were supplied and the arity is open-ended (`...rest: any[]`); and
//! - a *cross-location pointer* at the first declared parameter the fixed
//!   decorator calling convention leaves unsupplied — TS6210 ("An argument for
//!   '{0}' was not provided.") for a named parameter, or TS6236 ("Arguments for
//!   the rest parameter '{0}' were not provided.") when that parameter is
//!   variadic. This second line fires only for a *too-few* failure; a *too-many*
//!   failure supplies every declared parameter and gets only the TS1278 line.
//!
//! A non-arity failure (e.g. an argument type mismatch) is deliberately left
//! untouched: tsc attaches a different elaboration there (a `TS2345`-style "not
//! assignable" line) that this change does not wire, so those diagnostics keep
//! their pre-existing shape (no related information).
//!
//! Oracle-verified against pinned `typescript@7.0.2` for the class-decorator
//! shapes; the member/parameter shapes reuse the identical shared helper.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

fn diagnostics_experimental(source: &str) -> Vec<tsz_checker::diagnostics::Diagnostic> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            experimental_decorators: true,
            ..CheckerOptions::default()
        },
    )
}

fn diagnostics_es(source: &str) -> Vec<tsz_checker::diagnostics::Diagnostic> {
    check_source(source, "test.ts", CheckerOptions::default())
}

fn find(
    diags: &[tsz_checker::diagnostics::Diagnostic],
    code: u32,
) -> Option<&tsz_checker::diagnostics::Diagnostic> {
    diags.iter().find(|d| d.code == code)
}

/// Byte offset of the first occurrence of `needle` in `source`, used to check
/// the missing-argument pointer anchors at the parameter declaration.
fn offset_of(source: &str, needle: &str) -> u32 {
    u32::try_from(source.find(needle).expect("needle present in source")).expect("offset fits u32")
}

/// Assert the primary diagnostic carries exactly the TS1278/TS1279 elaboration
/// line, then the TS6210/TS6236 pointer at `param_start`.
fn assert_arity_chain(
    primary: &tsz_checker::diagnostics::Diagnostic,
    expect_runtime_code: u32,
    expect_runtime_message: &str,
    expect_pointer_code: u32,
    expect_pointer_message: &str,
    param_start: u32,
) {
    let related = &primary.related_information;
    assert_eq!(
        related.len(),
        2,
        "expected the runtime-arity line and the missing-argument pointer, got: {related:?}"
    );
    assert_eq!(related[0].code, expect_runtime_code);
    assert_eq!(related[0].message_text, expect_runtime_message);
    assert_eq!(related[1].code, expect_pointer_code);
    assert_eq!(related[1].message_text, expect_pointer_message);
    assert_eq!(
        related[1].start, param_start,
        "TS{expect_pointer_code} must anchor at the missing parameter declaration"
    );
}

// ───────────────────── class decorator (TS1238, legacy) ─────────────────

#[test]
fn ts1278_class_decorator_too_few_args_exact() {
    // `classDec` declares 2 required params; a legacy class decorator is
    // invoked with exactly 1 (the class constructor) -> TS1278 "expects 2",
    // plus a TS6210 pointer at the unsupplied `extra` parameter.
    let source = r#"
function classDec(target: Function, extra: string) {}

@classDec
class C {}
"#;
    let diags = diagnostics_experimental(source);
    let primary = find(&diags, 1238).expect("expected TS1238");
    assert_arity_chain(
        primary,
        1278,
        "The runtime will invoke the decorator with 1 arguments, but the decorator expects 2.",
        6210,
        "An argument for 'extra' was not provided.",
        offset_of(source, "extra: string"),
    );
}

#[test]
fn ts1279_class_decorator_too_few_args_at_least() {
    // A genuinely variadic decorator (`...rest: any[]`) with a required
    // `name` param that isn't supplied -> TS1279 "expects at least 2". The
    // first unsupplied parameter is the ordinary `name`, so the pointer is
    // TS6210 (named), not TS6236 (rest).
    let source = r#"
function classDec(target: Function, name: string, ...rest: any[]) {}

@classDec
class C {}
"#;
    let diags = diagnostics_experimental(source);
    let primary = find(&diags, 1238).expect("expected TS1238");
    assert_arity_chain(
        primary,
        1279,
        "The runtime will invoke the decorator with 1 arguments, but the decorator expects at least 2.",
        6210,
        "An argument for 'name' was not provided.",
        offset_of(source, "name: string"),
    );
}

#[test]
fn ts1278_class_decorator_fixed_length_rest_stays_exact() {
    // A fixed-length tuple rest param (`...rest: [string, number]`) has a
    // concrete max arity, so this stays TS1278 ("expects 3"), not TS1279 --
    // the discriminator is a concrete upper bound, not the presence of `...`.
    // The first unsupplied position is the tuple rest, so the pointer is
    // TS6236 anchored at it.
    let source = r#"
function classDec(target: Function, ...rest: [string, number]) {}

@classDec
class C {}
"#;
    let diags = diagnostics_experimental(source);
    let primary = find(&diags, 1238).expect("expected TS1238");
    assert_arity_chain(
        primary,
        1278,
        "The runtime will invoke the decorator with 1 arguments, but the decorator expects 3.",
        6236,
        "Arguments for the rest parameter 'rest' were not provided.",
        // tsz's normalized parameter anchor starts at the binding name, past the
        // `...` rest token.
        offset_of(source, "rest: [string, number]"),
    );
}

#[test]
fn ts1238_class_decorator_type_mismatch_stays_unelaborated() {
    // Control: a non-arity failure (wrong parameter type, not wrong count)
    // must not gain related information -- tsc's elaboration there is a
    // different, not-yet-wired shape ("Argument of type X is not assignable
    // to parameter of type Y."), so this stays exactly as before.
    let diags = diagnostics_experimental(
        r#"
function classDec(target: string) {}

@classDec
class C {}
"#,
    );
    let primary = find(&diags, 1238).expect("expected TS1238");
    assert!(
        primary.related_information.is_empty(),
        "expected no related info for a type-mismatch failure, got: {:?}",
        primary.related_information
    );
}

#[test]
fn ts6210_class_decorator_binder_name_varies() {
    // The pointer is driven by the decorator's declared parameters, not by any
    // fixed identifier: a renamed binder still names its own missing parameter.
    let source = r#"
function wrap(ctor: Function, label: string) {}

@wrap
class C {}
"#;
    let diags = diagnostics_experimental(source);
    let primary = find(&diags, 1238).expect("expected TS1238");
    assert_arity_chain(
        primary,
        1278,
        "The runtime will invoke the decorator with 1 arguments, but the decorator expects 2.",
        6210,
        "An argument for 'label' was not provided.",
        offset_of(source, "label: string"),
    );
}

#[test]
fn ts6210_class_decorator_arrow_const_declaration() {
    // The declaration is reachable through a `const` initialized with an arrow,
    // not only a `function` statement.
    let source = r#"
const classDec = (target: Function, extra: string) => {};

@classDec
class C {}
"#;
    let diags = diagnostics_experimental(source);
    let primary = find(&diags, 1238).expect("expected TS1238");
    assert_arity_chain(
        primary,
        1278,
        "The runtime will invoke the decorator with 1 arguments, but the decorator expects 2.",
        6210,
        "An argument for 'extra' was not provided.",
        offset_of(source, "extra: string"),
    );
}

#[test]
fn ts6210_class_decorator_skips_explicit_this_parameter() {
    // An explicit `this` parameter is not a value argument, so the pointer must
    // land on `extra`, not shift by one onto `this`.
    let source = r#"
function classDec(this: unknown, target: Function, extra: string) {}

@classDec
class C {}
"#;
    let diags = diagnostics_experimental(source);
    let primary = find(&diags, 1238).expect("expected TS1238");
    let pointer = primary
        .related_information
        .iter()
        .find(|r| r.code == 6210)
        .expect("expected TS6210");
    assert_eq!(
        pointer.message_text,
        "An argument for 'extra' was not provided."
    );
    assert_eq!(pointer.start, offset_of(source, "extra: string"));
}

// ───────────────────── ES class decorator (TS1238, stage-3) ─────────────

#[test]
fn ts1278_es_class_decorator_too_few_args() {
    // Gap 2: the ES (stage-3) class-decorator arity path never resolved a
    // `CallResult`, so it emitted a bare TS1238 with no elaboration. It now
    // routes through the shared helper: `(value, context)` = 2 supplied args,
    // a 3-required-param decorator -> TS1278 "expects 3" + TS6210 at the third
    // parameter (the first the two-argument convention cannot fill).
    let source = r#"
function classDec(value: any, context: any, extra: string) {}

@classDec
class C {}
"#;
    let diags = diagnostics_es(source);
    let primary = find(&diags, 1238).expect("expected TS1238");
    assert_arity_chain(
        primary,
        1278,
        "The runtime will invoke the decorator with 2 arguments, but the decorator expects 3.",
        6210,
        "An argument for 'extra' was not provided.",
        offset_of(source, "extra: string"),
    );
}

#[test]
fn ts1279_es_class_decorator_variadic_too_few_args() {
    // ES class decorator with an open-ended arity (`...rest: any[]`) and a
    // third required parameter -> TS1279 "at least 3" + TS6210 at `extra`.
    let source = r#"
function classDec(value: any, context: any, extra: string, ...rest: any[]) {}

@classDec
class C {}
"#;
    let diags = diagnostics_es(source);
    let primary = find(&diags, 1238).expect("expected TS1238");
    assert_arity_chain(
        primary,
        1279,
        "The runtime will invoke the decorator with 2 arguments, but the decorator expects at least 3.",
        6210,
        "An argument for 'extra' was not provided.",
        offset_of(source, "extra: string"),
    );
}

// ───────────────────── ES member decorator (TS1240) ─────────────────

#[test]
fn ts1278_es_field_decorator_too_few_args() {
    // ES (TC39) field decorators are invoked with `(value, context)` -- at
    // most 2 synthetic args, capped by `es_member_decorator_argument_count`.
    // A decorator declaring 3+ required params always underflows that cap.
    let source = r#"
function d(value: any, context: any, extra: string): any { return value; }
class C { @d field = 1; }
"#;
    let diags = diagnostics_es(source);
    let primary = find(&diags, 1240).expect("expected TS1240");
    assert_arity_chain(
        primary,
        1278,
        "The runtime will invoke the decorator with 2 arguments, but the decorator expects 3.",
        6210,
        "An argument for 'extra' was not provided.",
        offset_of(source, "extra: string"),
    );
}

// ───────────────────── method/accessor decorator (TS1241) ───────────────

#[test]
fn ts1278_method_decorator_too_few_args() {
    let source = r#"
function d(value: any, context: any, extra: string): any { return value; }
class C { @d method() {} }
"#;
    let diags = diagnostics_es(source);
    let primary = find(&diags, 1241).expect("expected TS1241");
    assert_arity_chain(
        primary,
        1278,
        "The runtime will invoke the decorator with 2 arguments, but the decorator expects 3.",
        6210,
        "An argument for 'extra' was not provided.",
        offset_of(source, "extra: string"),
    );
}

// ───────────────────── legacy property decorator (TS1240) ───────────────

#[test]
fn ts1278_legacy_property_decorator_too_many_args() {
    // Legacy field decorators are invoked `(target, propertyKey)`; a
    // 1-parameter decorator overflows -> TS1278 "expects 1" (the "too many"
    // shape). A too-many failure supplies every declared parameter, so there
    // is NO missing-argument pointer -- only the single TS1278 line.
    let diags = diagnostics_experimental(
        r#"
function propDec(target: any) {}

class C {
  @propDec
  static x: number;
}
"#,
    );
    let primary = find(&diags, 1240).expect("expected TS1240");
    assert_eq!(primary.related_information.len(), 1);
    assert_eq!(primary.related_information[0].code, 1278);
    assert_eq!(
        primary.related_information[0].message_text,
        "The runtime will invoke the decorator with 2 arguments, but the decorator expects 1."
    );
}

// ───────────────────── parameter decorator (TS1239) ─────────────────

#[test]
fn ts1278_parameter_decorator_too_few_args() {
    // Parameter decorators are always invoked with 3 args
    // (`target, propertyKey, parameterIndex`); a 4-required-param decorator
    // underflows -> TS1278 "expects 4" + TS6210 at the fourth parameter.
    let source = r#"
function paramDec(a: any, b: any, c: any, d: any) {}

class C {
  m(@paramDec x: number) {}
}
"#;
    let diags = diagnostics_experimental(source);
    let primary = find(&diags, 1239).expect("expected TS1239");
    assert_arity_chain(
        primary,
        1278,
        "The runtime will invoke the decorator with 3 arguments, but the decorator expects 4.",
        6210,
        "An argument for 'd' was not provided.",
        offset_of(source, "d: any"),
    );
}
