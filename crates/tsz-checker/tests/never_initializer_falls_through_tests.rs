//! Regression tests for #3662.
//!
//! TypeScript treats only **expression-statement-level** never calls as
//! terminators of control flow. A variable declaration whose initializer is
//! a never-returning call still leaves the function falling off the end and
//! must surface TS2355 (function whose declared type is neither undefined,
//! void, nor any must return a value).

use crate::test_utils::check_source_strict_codes as check_strict;

const TS2355: u32 = 2355;

#[test]
fn const_initializer_with_never_call_emits_ts2355() {
    let source = r#"
function fail(): never { throw new Error(); }

function f(): number {
  const value = fail();
}
"#;
    let codes = check_strict(source);
    assert!(
        codes.contains(&TS2355),
        "expected TS2355 for `const value = fail()` falling off `f(): number`; got {codes:?}"
    );
}

#[test]
fn let_initializer_with_never_call_emits_ts2355() {
    let source = r#"
function fail(): never { throw new Error(); }

function f(): number {
  let value = fail();
}
"#;
    let codes = check_strict(source);
    assert!(
        codes.contains(&TS2355),
        "expected TS2355 for `let value = fail()` falling off `f(): number`; got {codes:?}"
    );
}

#[test]
fn bare_never_call_statement_suppresses_ts2355() {
    // Bare expression-statement `fail();` *does* terminate control flow per
    // tsc; the function end is unreachable so TS2355 must NOT be emitted.
    let source = r#"
function fail(): never { throw new Error(); }

function f(): number {
  fail();
}
"#;
    let codes = check_strict(source);
    assert!(
        !codes.contains(&TS2355),
        "TS2355 must not fire when a bare never-call terminates the function; got {codes:?}"
    );
}

#[test]
fn mixed_branch_initializer_emits_ts2355() {
    let source = r#"
function fail(): never { throw new Error(); }

function f(): number {
  const value = Math.random() ? fail() : 1;
}
"#;
    let codes = check_strict(source);
    assert!(
        codes.contains(&TS2355),
        "expected TS2355 when only one branch of the initializer is `never`; got {codes:?}"
    );
}

#[test]
fn never_initializer_does_not_silence_subsequent_return_check() {
    // The structural rule must not depend on the initializer variable name —
    // any `var x = neverCall()` should still leave the function falling off
    // its end.
    let source = r#"
function fail(): never { throw new Error(); }

function withDifferentName(): string {
  const result = fail();
}
"#;
    let codes = check_strict(source);
    assert!(
        codes.contains(&TS2355),
        "structural rule must not depend on initializer variable name; got {codes:?}"
    );
}
