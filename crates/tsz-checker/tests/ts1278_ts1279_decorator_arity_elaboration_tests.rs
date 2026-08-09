//! TS1278/TS1279 elaboration for decorator signature-resolution failures.
//!
//! Structural rule: when a decorator's call resolution fails specifically
//! because of an argument-count mismatch, tsc attaches a related-information
//! chain link to the primary TS1238/1239/1240/1241 ("Unable to resolve
//! signature of ... decorator when called as an expression."):
//! - TS1278 ("The runtime will invoke the decorator with {1} arguments, but
//!   the decorator expects {0}.") when the decorator's declared arity has a
//!   concrete upper bound.
//! - TS1279 ("...but the decorator expects at least {0}.") when too few
//!   arguments were supplied and the decorator's arity is genuinely
//!   open-ended (a trailing `...rest: any[]`).
//!
//! tsz previously reported only the primary message, with no related
//! information at all — the elaboration line was silently dropped in both
//! the too-few and too-many argument shapes, across all five
//! resolve-call-based decorator-signature-check sites (class, ES member,
//! method/accessor, legacy property, parameter). A non-arity failure (e.g. an
//! argument type mismatch) is deliberately left untouched: tsc attaches a
//! different elaboration there (a `TS2345`-style "not assignable" line) that
//! this change does not wire, so those diagnostics keep their pre-existing
//! shape (no related information) rather than attaching the wrong kind.
//!
//! Oracle-verified against pinned `typescript@7.0.2` for every shape below.

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

// ───────────────────────── class decorator (TS1238, legacy) ─────────────────

#[test]
fn ts1278_class_decorator_too_few_args_exact() {
    // `classDec` declares 2 required params; a legacy class decorator is
    // invoked with exactly 1 (the class constructor) -> TS1278 "expects 2".
    let diags = diagnostics_experimental(
        r#"
function classDec(target: Function, extra: string) {}

@classDec
class C {}
"#,
    );
    let primary = find(&diags, 1238).expect("expected TS1238");
    assert_eq!(primary.related_information.len(), 1);
    assert_eq!(primary.related_information[0].code, 1278);
    assert_eq!(
        primary.related_information[0].message_text,
        "The runtime will invoke the decorator with 1 arguments, but the decorator expects 2."
    );
}

#[test]
fn ts1279_class_decorator_too_few_args_at_least() {
    // A genuinely variadic decorator (`...rest: any[]`) with a required
    // `name` param that isn't supplied -> TS1279 "expects at least 2".
    let diags = diagnostics_experimental(
        r#"
function classDec(target: Function, name: string, ...rest: any[]) {}

@classDec
class C {}
"#,
    );
    let primary = find(&diags, 1238).expect("expected TS1238");
    assert_eq!(primary.related_information.len(), 1);
    assert_eq!(primary.related_information[0].code, 1279);
    assert_eq!(
        primary.related_information[0].message_text,
        "The runtime will invoke the decorator with 1 arguments, but the decorator expects at least 2."
    );
}

#[test]
fn ts1278_class_decorator_fixed_length_rest_stays_exact() {
    // A fixed-length tuple rest param (`...rest: [string, number]`) has a
    // concrete max arity, so this stays TS1278 ("expects 3"), not TS1279 --
    // the discriminator is a concrete upper bound, not the presence of `...`.
    let diags = diagnostics_experimental(
        r#"
function classDec(target: Function, ...rest: [string, number]) {}

@classDec
class C {}
"#,
    );
    let primary = find(&diags, 1238).expect("expected TS1238");
    assert_eq!(primary.related_information.len(), 1);
    assert_eq!(primary.related_information[0].code, 1278);
    assert_eq!(
        primary.related_information[0].message_text,
        "The runtime will invoke the decorator with 1 arguments, but the decorator expects 3."
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

// ───────────────────────── ES member decorator (TS1240) ─────────────────────

#[test]
fn ts1278_es_field_decorator_too_few_args() {
    // ES (TC39) field decorators are invoked with `(value, context)` -- at
    // most 2 synthetic args, capped by `es_member_decorator_argument_count`.
    // A decorator declaring 3+ required params always underflows that cap
    // (a 1- or 2-required-param decorator gets exactly the args it declared,
    // per `min(max(paramCount, 1), 2)`, and never fails on arity alone).
    let diags = diagnostics_es(
        r#"
function d(value: any, context: any, extra: string): any { return value; }
class C { @d field = 1; }
"#,
    );
    let primary = find(&diags, 1240).expect("expected TS1240");
    assert_eq!(primary.related_information.len(), 1);
    assert_eq!(primary.related_information[0].code, 1278);
    assert_eq!(
        primary.related_information[0].message_text,
        "The runtime will invoke the decorator with 2 arguments, but the decorator expects 3."
    );
}

// ───────────────────────── method/accessor decorator (TS1241) ───────────────

#[test]
fn ts1278_method_decorator_too_few_args() {
    let diags = diagnostics_es(
        r#"
function d(value: any, context: any, extra: string): any { return value; }
class C { @d method() {} }
"#,
    );
    let primary = find(&diags, 1241).expect("expected TS1241");
    assert_eq!(primary.related_information.len(), 1);
    assert_eq!(primary.related_information[0].code, 1278);
}

// ───────────────────────── legacy property decorator (TS1240) ───────────────

#[test]
fn ts1278_legacy_property_decorator_too_many_args() {
    // Legacy field decorators are invoked `(target, propertyKey)`; a
    // 1-parameter decorator overflows -> TS1278 "expects 1" (the "too many"
    // shape, not "too few" -- proves the exact-vs-at-least split isn't keyed
    // on which direction the mismatch runs).
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

// ───────────────────────── parameter decorator (TS1239) ─────────────────────

#[test]
fn ts1278_parameter_decorator_too_few_args() {
    // Parameter decorators are always invoked with 3 args
    // (`target, propertyKey, parameterIndex`); a 4-required-param decorator
    // underflows -> TS1278 "expects 4".
    let diags = diagnostics_experimental(
        r#"
function paramDec(a: any, b: any, c: any, d: any) {}

class C {
  m(@paramDec x: number) {}
}
"#,
    );
    let primary = find(&diags, 1239).expect("expected TS1239");
    assert_eq!(primary.related_information.len(), 1);
    assert_eq!(primary.related_information[0].code, 1278);
    assert_eq!(
        primary.related_information[0].message_text,
        "The runtime will invoke the decorator with 3 arguments, but the decorator expects 4."
    );
}
