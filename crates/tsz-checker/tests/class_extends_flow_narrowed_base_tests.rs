//! A class expression whose `extends` base is a flow-narrowed value must use the
//! narrowed type, not the wider declared type, for the TS2507 constructor check.
//!
//! Regression for #14260 (effect): `(klass?: Ctor) => klass ? class extends klass {} : null`
//! narrows `klass` from `Ctor | undefined` to `Ctor` at the `extends` position, so the
//! base is constructable and tsc accepts it. tsz used the declared `Ctor | undefined`
//! and emitted a spurious TS2507. The fix is additive: a TS2507 is suppressed only when
//! the flow-narrowed base type is genuinely a constructor type.

use tsz_checker::test_utils::check_source_code_messages;

fn ts2507_count(source: &str) -> usize {
    check_source_code_messages(source)
        .into_iter()
        .filter(|(code, _)| *code == 2507)
        .count()
}

#[test]
fn ternary_narrowed_optional_ctor_base_no_ts2507() {
    let src = r#"
type Ctor<T = {}> = new (...args: Array<any>) => T;
export const make = (klass?: Ctor) => (klass ? class extends klass {} : null);
"#;
    assert_eq!(
        ts2507_count(src),
        0,
        "narrowed `Ctor` base in a ternary must not emit TS2507"
    );
}

#[test]
fn early_return_guard_narrowed_ctor_base_no_ts2507() {
    let src = r#"
type Ctor<T = {}> = new (...args: Array<any>) => T;
export function make(klass?: Ctor) {
    if (!klass) return null;
    return class extends klass {};
}
"#;
    assert_eq!(
        ts2507_count(src),
        0,
        "narrowed `Ctor` base after an early-return guard must not emit TS2507"
    );
}

// Binder-name variation: the fix is structural (flow narrowing of the base
// expression), not keyed on any identifier. A renamed parameter behaves the same.
#[test]
fn ternary_narrowed_optional_ctor_base_renamed_binder_no_ts2507() {
    let src = r#"
type MakeNew<T = {}> = new (...args: Array<any>) => T;
export const build = (factory?: MakeNew) =>
    factory ? class extends factory {} : null;
"#;
    assert_eq!(
        ts2507_count(src),
        0,
        "renamed binder narrowed base must not emit TS2507"
    );
}

// Negative control: a non-constructor base still emits TS2507.
#[test]
fn extends_non_constructor_value_still_emits_ts2507() {
    let src = r#"
const n: number = 5;
class C extends n {}
"#;
    assert!(
        ts2507_count(src) >= 1,
        "extending a number must still emit TS2507"
    );
}

// Negative control: an un-narrowed `Ctor | undefined` base still emits TS2507,
// because narrowing does not remove the nullish arm at the `extends` position.
#[test]
fn extends_unnarrowed_optional_ctor_still_emits_ts2507() {
    let src = r#"
type Ctor<T = {}> = new (...args: Array<any>) => T;
declare const maybe: Ctor | undefined;
class C extends maybe {}
"#;
    assert!(
        ts2507_count(src) >= 1,
        "extending an un-narrowed `Ctor | undefined` must still emit TS2507"
    );
}
