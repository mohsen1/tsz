//! Unit test for the `is_non_suppressing_parse_error` entry added for
//! #16360's cross-file suppression bug. Split into its own file rather than
//! growing `tests.rs` past the 2000-line limit (already over before this
//! change), mirroring `rest_parameter_grammar_tests.rs`.
//!
//! TS18037 ('await' expression cannot be used inside a class static block)
//! is a check-time grammar error in tsc (`checkAwaitExpression`) on an
//! otherwise well-formed AST — tsz emits it from the parser instead
//! (`parse_await_expression`), so before this entry it set
//! `has_syntax_parse_errors` and deleted every other TS1308/TS1375/TS1378
//! ('await' outside an async function, or outside a module at the top
//! level) diagnostic elsewhere in the same file.

use super::*;

#[test]
fn class_static_block_await_error_does_not_suppress_grammar_diagnostics() {
    assert!(
        is_non_suppressing_parse_error(18037),
        "TS18037 (await inside class static block) should be non-suppressing"
    );
}
