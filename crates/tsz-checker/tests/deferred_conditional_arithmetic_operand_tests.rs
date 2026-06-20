//! Regression tests for #14154: a deferred conditional type whose branches are
//! all numeric (and, more generally, any instantiable operand whose base
//! constraint is numeric) must be accepted as an arithmetic operand — no false
//! TS2362/TS2363.
//!
//! Structural rule: `tsc`'s `checkArithmeticOperandType` validates an operand by
//! assignability to `number | bigint`, which for an instantiable type (a deferred
//! conditional, a bare type parameter, ...) is decided against that type's base
//! constraint / apparent type. tsz now resolves the operand toward its apparent
//! form (`BinaryOpEvaluator::is_arithmetic_operand`) before classifying it, so a
//! conditional with all-numeric branches and a numeric-constrained type parameter
//! pass, while a conditional with a non-numeric branch and an unconstrained
//! parameter still fail. Anti-hardcoding: the rule is structural, every binder is
//! renamed across the matrix and the behavior holds.

use tsz_checker::test_utils::check_source_codes;

fn count_arith_operand_errors(source: &str) -> usize {
    check_source_codes(source)
        .iter()
        .filter(|&&c| c == 2362 || c == 2363)
        .count()
}

/// The minimal io-ts/remeda witness: `sum(data)` returns a deferred conditional
/// `Sum<T> = T extends readonly [] ? 0 : number`; dividing it by a number must
/// not warn (`0 | number = number` is a valid arithmetic operand).
#[test]
fn deferred_conditional_all_numeric_branches_is_valid_operand() {
    let source = r#"
type IterableContainer<T> = readonly T[] | readonly [];
type Sum<T extends IterableContainer<number>> = T extends readonly [] ? 0 : number;
declare function sum<T extends IterableContainer<number>>(data: T): Sum<T>;
export function meanImpl<T extends IterableContainer<number>>(data: T): T[number] | undefined {
  if (data.length === 0) return undefined;
  return sum(data) / data.length;
}
"#;
    assert_eq!(
        count_arith_operand_errors(source),
        0,
        "deferred conditional with all-numeric branches must be a valid arithmetic operand"
    );
}

/// Anti-hardcoding: rename every binder (alias, parameter, function); the rule
/// is structural, not name-driven.
#[test]
fn deferred_conditional_numeric_operand_is_binder_name_independent() {
    let source = r#"
type Bag<U> = readonly U[] | readonly [];
type Total<S extends Bag<number>> = S extends readonly [] ? 0 : number;
declare function reduce<S extends Bag<number>>(xs: S): Total<S>;
export function avg<S extends Bag<number>>(xs: S): number {
  return reduce(xs) * 2;
}
"#;
    assert_eq!(
        count_arith_operand_errors(source),
        0,
        "renamed-binder deferred conditional must still be a valid arithmetic operand"
    );
}

/// Adjacent: a bare type parameter with a numeric constraint is a valid operand.
#[test]
fn numeric_constrained_type_parameter_is_valid_operand() {
    let source = r"
export function dbl<T extends number>(x: T): number { return x * 2; }
export function dbl2<T extends bigint>(x: T): bigint { return x * 2n; }
";
    assert_eq!(
        count_arith_operand_errors(source),
        0,
        "a type parameter constrained to number/bigint must be a valid arithmetic operand"
    );
}

/// Adjacent: a deferred conditional with mixed numeric branches
/// (`... ? number : bigint`) is still a valid *operand* (no TS2362); any
/// number/bigint mixing is reported separately (TS2365), not as TS2362.
#[test]
fn mixed_numeric_conditional_branches_is_valid_operand() {
    let source = r"
type Mix<T> = T extends string ? number : bigint;
declare function pick<T>(t: T): Mix<T>;
export const use1 = <T>(t: T): bigint => pick<string>('s') as never;
export const use2 = <T>(t: T) => pick(t) - (0 as never);
";
    assert_eq!(
        count_arith_operand_errors(source),
        0,
        "mixed-numeric conditional branches must not raise TS2362/TS2363 (operand validity)"
    );
}

/// Negative control: a conditional with a non-numeric branch must STILL warn.
#[test]
fn conditional_with_non_numeric_branch_still_warns() {
    let source = r"
type Maybe<T> = T extends string ? number : string;
declare function pick<T>(t: T): Maybe<T>;
export const bad = <T>(t: T) => pick(t) * 2;
";
    assert!(
        count_arith_operand_errors(source) >= 1,
        "a conditional with a non-numeric branch must still raise TS2362"
    );
}

/// Negative control: an unconstrained type parameter (base constraint `unknown`)
/// must STILL warn — `unknown` is not assignable to `number | bigint`.
#[test]
fn unconstrained_type_parameter_still_warns() {
    let source = r"
export function bad<T>(x: T): number { return x * 2; }
";
    assert!(
        count_arith_operand_errors(source) >= 1,
        "an unconstrained type parameter must still raise TS2362"
    );
}

/// Negative control: a type parameter constrained to `number | string` must
/// STILL warn (the string constituent is not numeric).
#[test]
fn union_constraint_with_string_still_warns() {
    let source = r"
export function bad<T extends number | string>(x: T) { return x - 1; }
";
    assert!(
        count_arith_operand_errors(source) >= 1,
        "a type parameter constrained to number | string must still raise TS2362"
    );
}
