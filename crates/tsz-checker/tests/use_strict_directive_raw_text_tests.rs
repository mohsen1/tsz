//! Directive-prologue recognition must follow the ECMAScript rule that a
//! Use Strict Directive may NOT contain an escape sequence: recognition is
//! performed against the verbatim source text of the string literal, not its
//! cooked value. `tsc` mirrors this in `isUseStrictPrologueDirective`.
//!
//! These tests pin the strict-mode-only diagnostic TS1100 ("Invalid use of
//! 'eval'/'arguments' in strict mode.") to the directive that actually puts a
//! file into strict mode:
//!   - `"use strict"` / `'use strict'` → strict (TS1100 fires)
//!   - a `use strict` literal written with an escape sequence → NOT strict (no TS1100)
//!   - `"use client"` / `"use server"` (React directives) → NOT strict
//!
//! `always_strict` is explicitly disabled so the directive prologue is the only
//! possible source of strict mode and the assertions isolate its effect.

use tsz_checker::test_utils::check_with_options_code_messages;
use tsz_common::CheckerOptions;

fn non_always_strict_options() -> CheckerOptions {
    CheckerOptions {
        always_strict: false,
        ..CheckerOptions::default()
    }
}

fn ts1100_eval_count(source: &str) -> usize {
    check_with_options_code_messages(source, non_always_strict_options())
        .into_iter()
        .filter(|(code, msg)| *code == 1100 && msg.contains("'eval'"))
        .count()
}

#[test]
fn plain_double_quoted_use_strict_enables_strict_mode() {
    let source = "\"use strict\";\nfunction f() { var { eval } = { eval: 1 }; }\n";
    assert!(
        ts1100_eval_count(source) > 0,
        "plain \"use strict\" must put the file in strict mode (TS1100 expected)"
    );
}

#[test]
fn single_quoted_use_strict_enables_strict_mode() {
    let source = "'use strict';\nfunction f() { var { eval } = { eval: 1 }; }\n";
    assert!(
        ts1100_eval_count(source) > 0,
        "'use strict' must put the file in strict mode (TS1100 expected)"
    );
}

#[test]
fn escaped_use_strict_does_not_enable_strict_mode() {
    // Cooked value is `use strict`, but the ` ` escape disqualifies it as a
    // Use Strict Directive, so the file stays non-strict and TS1100 must NOT fire.
    let source = "\"use\\u0020strict\";\nfunction f() { var { eval } = { eval: 1 }; }\n";
    assert_eq!(
        ts1100_eval_count(source),
        0,
        "escaped \"use\\u0020strict\" must not enable strict mode (no TS1100)"
    );
}

#[test]
fn react_directives_do_not_enable_strict_mode() {
    for directive in ["\"use client\"", "\"use server\"", "\"use cache\""] {
        let source = format!("{directive};\nfunction f() {{ var {{ eval }} = {{ eval: 1 }}; }}\n");
        assert_eq!(
            ts1100_eval_count(&source),
            0,
            "directive {directive} must not enable strict mode (no TS1100)"
        );
    }
}
