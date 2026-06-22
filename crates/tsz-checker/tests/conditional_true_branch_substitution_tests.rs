//! Regression for issue #14167: inside the true branch of a conditional type,
//! the check operand must be narrowed by the `extends` type (`tsc`'s
//! `getConditionalFlowTypeOfType` / `SubstitutionType`), so a dependent
//! constraint check on a use of the operand passes.
//!
//! The previously-missing case is a *structured* (non-naked) check operand such
//! as a generic-alias instantiation `F<V>`: `tsc`'s `getImpliedConstraint`
//! compares the actual type variable of the whole check type, not only bare type
//! parameters, so `F<V> extends string ? Capitalize<F<V>> : F<V>` narrows
//! `F<V>` to `& string` in the true branch and the `Capitalize` constraint
//! passes.
//!
//! Binder names and the string-mapping intrinsic are varied across cases so the
//! guard follows structure, not identifier text. Negative controls confirm the
//! narrowing is by the *actual* `extends` type and does not blanket-suppress.

use tsz_checker::CheckerOptions;
use tsz_checker::test_utils::check_source_with_libs_code_messages;

fn codes(source: &str) -> Vec<u32> {
    let libs = tsz_checker::test_utils::load_default_lib_files();
    let opts = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    };
    check_source_with_libs_code_messages(source, "test.ts", opts, &libs)
        .into_iter()
        .map(|(c, _)| c)
        .collect()
}

/// The mined witness: a deferred-conditional alias `Snake<V>` used as the check
/// operand; in the true branch `Capitalize<Snake<V>>`'s `extends string`
/// constraint must pass because `Snake<V>` is narrowed to `string`.
#[test]
fn structured_check_operand_narrowed_in_true_branch_no_ts2344() {
    let c = codes(
        r#"
type Snake<Type> = Type extends string ? `snake_${Type}` : Type;
type _Pascal<V> = Snake<V> extends string
    ? Capitalize<Snake<V>>
    : Snake<V>;
export {};
"#,
    );
    assert!(
        !c.contains(&2344),
        "no TS2344 expected — Snake<V> is narrowed to string in the true branch. Got: {c:?}"
    );
}

/// Renamed binders + a different string-mapping intrinsic (`Uppercase`) must
/// behave identically — the rule is structural.
#[test]
fn structured_check_operand_renamed_binders_uppercase_no_ts2344() {
    let c = codes(
        r#"
type Wrap<Q> = Q extends string ? `wrap_${Q}` : Q;
type _Out<W> = Wrap<W> extends string
    ? Uppercase<Wrap<W>>
    : Wrap<W>;
export {};
"#,
    );
    assert!(
        !c.contains(&2344),
        "no TS2344 expected with renamed binders / Uppercase. Got: {c:?}"
    );
}

/// Negative control: when the operand is constrained by `number` (not
/// `string`), a `Capitalize<...>` use in the true branch must STILL report
/// TS2344 — the narrowing is by the actual `extends` type, not a blanket pass.
#[test]
fn structured_check_operand_wrong_constraint_still_ts2344() {
    let c = codes(
        r#"
type Snake<Type> = Type extends string ? `snake_${Type}` : Type;
type _Bad<V> = Snake<V> extends number
    ? Capitalize<Snake<V>>
    : Snake<V>;
export {};
"#,
    );
    assert!(
        c.contains(&2344),
        "TS2344 expected — Snake<V> narrowed to `& number` does not satisfy `string`. Got: {c:?}"
    );
}

/// The classic naked-type-parameter operand must keep working: `T extends
/// string ? Capitalize<T> : T` is clean.
#[test]
fn naked_type_parameter_operand_still_clean() {
    let c = codes(
        r#"
type F<T> = T extends string ? Capitalize<T> : T;
export {};
"#,
    );
    assert!(
        !c.contains(&2344),
        "no TS2344 expected for a naked type-parameter check operand. Got: {c:?}"
    );
}

/// A bare reference to the (unnarrowed) alias outside any conditional true
/// branch must remain rejected, so the narrowing is scoped to the true branch.
#[test]
fn operand_outside_true_branch_still_ts2344() {
    let c = codes(
        r#"
type Snake<Type> = Type extends string ? `snake_${Type}` : Type;
type _Direct<V> = Capitalize<Snake<V>>;
export {};
"#,
    );
    assert!(
        c.contains(&2344),
        "TS2344 expected — outside a conditional true branch Snake<V> is not narrowed. Got: {c:?}"
    );
}
