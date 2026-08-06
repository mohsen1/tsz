//! Regression tests for the TS2407 message's operand type display (issue
//! surfaced while investigating `TypeScript/tests/cases/compiler/forIn2.ts`).
//!
//! `tsc`'s `checkForInStatement` reports
//! `typeToString(getNonNullableTypeIfNeeded(checkExpression(node.expression)))`
//! — the RHS's own checked type, not a type widened for property-access
//! purposes. tsz's `check_for_in_expression_type` widens its `expr_type`
//! parameter internally (via `resolve_type_for_property_access`, needed for
//! the object-type validity check) and was reusing that widened value for the
//! message too, so `for (var i in 1)` reported `'number'` where `tsc` reports
//! `'1'`.
//!
//! Two corrections, both oracle-verified against `tsc` 7.0.2
//! (`--noEmit --pretty false --target es2015`, plus `--singleThreaded
//! --stableTypeOrdering true` — the flags `scripts/conformance/oracle.sh` adds
//! for TypeScript 7+, see #16413):
//!
//! - A fresh literal operand (`1`, `"str"`, `true`, `10n`) keeps its literal
//!   spelling, recovered from the operand node via
//!   `literal_type_from_initializer` — the same helper
//!   `emit_ts2488_not_iterable` already uses for the analogous for-of message.
//! - A bare `null`/`undefined` operand displays as `'never'`
//!   (`getNonNullableTypeIfNeeded` collapses it before `typeToString`), in
//!   both strict and non-strict mode. Tested here under strict mode only,
//!   where TS2407 unambiguously fires for both operands; whether TS2407 fires
//!   at all for a non-strict bare `null`/`undefined` operand is a pre-existing
//!   question this file does not touch (see
//!   `for_in_self_reference_and_nullable_operand_tests.rs`) — this file only
//!   covers the message *text* on the error path, which is independent of
//!   whether that path is taken.
//!
//! An operand that is a *declared identifier* (not a literal expression) is
//! unaffected either way: its already-widened or already-literal declared
//! type displays exactly as before, so `let x = 5` still reports `'number'`
//! and `declare const n: 1` still reports `'1'`. Binder names are varied so
//! nothing here can be satisfied by a user-chosen identifier.

use crate::test_utils::check_source_strict_messages;

const TS2407: u32 = 2407;

fn message_for(source: &str) -> String {
    let messages = check_source_strict_messages(source);
    let matches: Vec<&String> = messages
        .iter()
        .filter(|(code, _)| *code == TS2407)
        .map(|(_, text)| text)
        .collect();
    assert_eq!(
        matches.len(),
        1,
        "expected exactly one TS2407 in {source:?}, got: {messages:?}"
    );
    matches[0].clone()
}

#[test]
fn fresh_numeric_literal_operand_keeps_its_literal_type() {
    assert_eq!(
        message_for("for (var i in 42) {}"),
        "The right-hand side of a 'for...in' statement must be of type 'any', an object type or a type parameter, but here has type '42'."
    );
}

#[test]
fn fresh_string_literal_operand_keeps_its_literal_type() {
    assert_eq!(
        message_for("for (var reader in \"payload\") {}"),
        "The right-hand side of a 'for...in' statement must be of type 'any', an object type or a type parameter, but here has type '\"payload\"'."
    );
}

#[test]
fn fresh_boolean_literal_operand_keeps_its_literal_type() {
    assert_eq!(
        message_for("for (var flag in true) {}"),
        "The right-hand side of a 'for...in' statement must be of type 'any', an object type or a type parameter, but here has type 'true'."
    );
    assert_eq!(
        message_for("for (var flag in false) {}"),
        "The right-hand side of a 'for...in' statement must be of type 'any', an object type or a type parameter, but here has type 'false'."
    );
}

#[test]
fn fresh_bigint_literal_operand_keeps_its_literal_type() {
    assert_eq!(
        message_for("for (var big in 10n) {}"),
        "The right-hand side of a 'for...in' statement must be of type 'any', an object type or a type parameter, but here has type '10n'."
    );
}

#[test]
fn bare_null_operand_displays_as_never() {
    assert_eq!(
        message_for("for (var key in null) {}"),
        "The right-hand side of a 'for...in' statement must be of type 'any', an object type or a type parameter, but here has type 'never'."
    );
}

#[test]
fn bare_undefined_operand_displays_as_never() {
    assert_eq!(
        message_for("for (var key in undefined) {}"),
        "The right-hand side of a 'for...in' statement must be of type 'any', an object type or a type parameter, but here has type 'never'."
    );
}

#[test]
fn declared_null_typed_identifier_operand_also_displays_as_never() {
    // Same `never` collapse applies at the type level, not just the syntax
    // level: a `null`-typed identifier (not a literal node) must display
    // identically to the bare `null` literal above.
    assert_eq!(
        message_for("declare const holder: null; for (var key in holder) {}"),
        "The right-hand side of a 'for...in' statement must be of type 'any', an object type or a type parameter, but here has type 'never'."
    );
}

#[test]
fn declared_literal_typed_identifier_operand_keeps_its_declared_literal_type() {
    // Not a literal *expression* (it is an identifier), but its declared type
    // is already the literal `1` — the message must still show `'1'`, exactly
    // as it did before this fix (this path never went through the widened
    // `resolve_type_for_property_access` value for display).
    assert_eq!(
        message_for("declare const pinned: 1; for (var key in pinned) {}"),
        "The right-hand side of a 'for...in' statement must be of type 'any', an object type or a type parameter, but here has type '1'."
    );
}

#[test]
fn widened_let_binding_operand_still_displays_its_widened_type() {
    // `let count = 5` infers the widened `number`, and the for-in operand is
    // an identifier (not a literal node), so `literal_type_from_initializer`
    // declines and the message correctly falls back to the resolved type —
    // matching `tsc`, which also widens here (ordinary `let` inference, not a
    // for-in-specific rule).
    assert_eq!(
        message_for("let count = 5; for (var key in count) {}"),
        "The right-hand side of a 'for...in' statement must be of type 'any', an object type or a type parameter, but here has type 'number'."
    );
}
