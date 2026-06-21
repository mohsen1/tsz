//! Computed property name literal-form acceptance (TS1170). (#14256)

use super::super::core::*;

/// A unary `+`/`-` applied directly to a numeric or bigint literal is a
/// numeric-literal-typed computed property name; tsc accepts it. tsz previously
/// rejected `[-1]`/`[+1]`/`[-1n]` with TS1170 because its literal-form gate only
/// recognized bare `NumericLiteral`/`StringLiteral`. Mined from ts-arithmetic.
#[test]
fn unary_numeric_literal_computed_property_no_ts1170() {
    let diagnostics = compile_and_get_diagnostics(
        r"
type M = { [-1]: 0; [+2]: 1; [-3n]: 2 };
type X = M[-1];
        ",
    );
    assert!(
        !has_error(&diagnostics, 1170),
        "unary +/- over a numeric/bigint literal is a literal computed-property name; \
         no TS1170 expected. Actual diagnostics: {diagnostics:#?}"
    );
}

/// Negative control: a unary operator that is NOT `+`/`-` (here bitwise `~`) is
/// not a numeric-literal-form name, so TS1170 must still fire — the fix is gated
/// on the operator and a literal operand, not on "any prefix-unary expression".
#[test]
fn non_plusminus_unary_computed_property_still_ts1170() {
    let diagnostics = compile_and_get_diagnostics(
        r"
type N = { [~1]: 0 };
        ",
    );
    assert!(
        has_error(&diagnostics, 1170),
        "`[~1]` is not a numeric-literal-form computed property name; TS1170 expected. \
         Actual diagnostics: {diagnostics:#?}"
    );
}
