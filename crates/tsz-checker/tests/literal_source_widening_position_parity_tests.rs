//! A declared *unit-literal* source (`0n`, `42`, `"abc"`, `true`) widens to its
//! base type in a diagnostic message when the assignment/return target cannot
//! hold a literal, matching `tsc` and the call-argument source path.
//!
//! tsc's diagnostic widening is target-driven: a unit literal is shown as its
//! primitive base (`bigint`, `number`, `string`, `boolean`) when the target is a
//! non-literal-sensitive type, and kept precise only against a literal-sensitive
//! target (`0`, `"x"`). tsz already mirrored this for call arguments but
//! preserved the declared literal in return-statement and variable-assignment
//! source positions, so `function f(): boolean { return x }` (where
//! `x: 0n`) printed `Type '0n' ...` instead of `Type 'bigint' ...`.
//!
//! The fix is confined to scalar unit literals (`literal_value` present): tuple,
//! object, and `as const` literal surfaces have no scalar literal value and keep
//! their existing preserve-the-literal display. Binder names are varied so the
//! behaviour is proven structural rather than keyed on a fixture identifier.

use tsz_checker::context::CheckerOptions;
use tsz_common::diagnostics::Diagnostic;

fn check_strict(source: &str) -> Vec<Diagnostic> {
    let options = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..Default::default()
    };
    tsz_checker::test_utils::check_source(source, "test.ts", options)
}

fn message_for(diags: &[Diagnostic], code: u32) -> String {
    let matches: Vec<&Diagnostic> = diags.iter().filter(|d| d.code == code).collect();
    assert_eq!(
        matches.len(),
        1,
        "expected exactly one TS{code}, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
    matches[0].message_text.clone()
}

/// Return-statement source: a `bigint` literal widens to `bigint`.
#[test]
fn bigint_literal_return_source_widens_against_boolean() {
    let msg = message_for(
        &check_strict(
            "declare const ticks: 0n;\n\
             function asFlag(): boolean { return ticks; }\n",
        ),
        2322,
    );
    assert!(
        msg.contains("Type 'bigint' is not assignable to type 'boolean'"),
        "bigint literal return source should widen to 'bigint'; got: {msg}"
    );
}

/// Variable-assignment source: a numeric literal widens to `number`.
#[test]
fn number_literal_assignment_source_widens_against_boolean() {
    let msg = message_for(
        &check_strict(
            "declare const tally: 42;\n\
             const gate: boolean = tally;\n",
        ),
        2322,
    );
    assert!(
        msg.contains("Type 'number' is not assignable to type 'boolean'"),
        "number literal assignment source should widen to 'number'; got: {msg}"
    );
}

/// Return-statement source: a string literal widens to `string`.
#[test]
fn string_literal_return_source_widens_against_boolean() {
    let msg = message_for(
        &check_strict(
            "declare const label: \"abc\";\n\
             function asFlag(): boolean { return label; }\n",
        ),
        2322,
    );
    assert!(
        msg.contains("Type 'string' is not assignable to type 'boolean'"),
        "string literal return source should widen to 'string'; got: {msg}"
    );
}

/// Call-argument source already widened; guard against regressing it.
#[test]
fn numeric_literal_call_argument_source_still_widens() {
    let msg = message_for(
        &check_strict(
            "declare function expect(flag: boolean): void;\n\
             declare const seven: 7;\n\
             expect(seven);\n",
        ),
        2345,
    );
    assert!(
        msg.contains("Argument of type 'number' is not assignable to parameter of type 'boolean'"),
        "numeric literal call-argument source should widen to 'number'; got: {msg}"
    );
}

/// Negative control: a literal-sensitive target keeps the source literal precise.
#[test]
fn literal_sensitive_target_keeps_source_literal() {
    let msg = message_for(
        &check_strict(
            "declare const single: 1;\n\
             const zeroOnly: 0 = single;\n",
        ),
        2322,
    );
    assert!(
        msg.contains("Type '1' is not assignable to type '0'"),
        "a literal-sensitive target must keep the source literal; got: {msg}"
    );
}

/// Negative control: a tuple-literal source has no scalar literal value and keeps
/// its precise literal surface even against a non-literal-sensitive target.
#[test]
fn tuple_literal_source_keeps_literal_surface() {
    let msg = message_for(
        &check_strict(
            "declare const triad: [1, 2, 3];\n\
             const gate: boolean = triad;\n",
        ),
        2322,
    );
    assert!(
        msg.contains("Type '[1, 2, 3]' is not assignable to type 'boolean'"),
        "a tuple-literal source must keep its literal surface; got: {msg}"
    );
}
