//! Generic method override type-parameter constraint compatibility (TS2416).
//!
//! Structural rule: when a derived class overrides a base class's *generic*
//! method, the derived method's type-parameter constraints may be no stricter
//! than the base's. If a derived type parameter is strictly narrower than the
//! corresponding base type parameter, the base method could be called with
//! arguments the derived method's constraint rejects, so the override is
//! unsound and tsc emits TS2416. tsz used to erase both signatures' type
//! parameters to their constraints and then compare under method bivariance,
//! which silently accepted these unsound overrides.
//!
//! These tests vary the type-parameter names (`T`/`U`, `K`/`Q`, ...) so a fix
//! that hardcoded a particular spelling would not satisfy them.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

fn ts2416_count(source: &str) -> usize {
    let diags = check_source(source, "test.ts", CheckerOptions::default());
    diags.iter().filter(|d| d.code == 2416).count()
}

/// Derived adds a stronger constraint (`object`) to an unconstrained base type
/// parameter. tsc rejects (the base accepts any `T`, the derived only objects).
#[test]
fn derived_adds_stronger_constraint_emits_ts2416() {
    let source = r#"
class Base { foo<T>(x: T): T { return x; } }
class Derived extends Base { foo<U extends object>(x: U): U { return x; } }
"#;
    assert_eq!(
        ts2416_count(source),
        1,
        "expected TS2416 when derived narrows an unconstrained base type parameter"
    );
}

/// Same rule, different type-parameter names and a narrower literal-union
/// constraint. A name-hardcoded fix would miss this.
#[test]
fn derived_narrows_string_constraint_emits_ts2416() {
    let source = r#"
class Base { pick<K extends string>(x: K): K { return x; } }
class Derived extends Base { pick<Q extends "a" | "b">(x: Q): Q { return x; } }
"#;
    assert_eq!(ts2416_count(source), 1);
}

/// Stronger constraint that only shows up in the *return* position is still
/// rejected, because the base caller observes the wider return type.
#[test]
fn derived_stronger_constraint_in_return_emits_ts2416() {
    let source = r#"
class Base { make<U extends string>(): U { return null as any; } }
class Derived extends Base { make<T extends "a">(): T { return null as any; } }
"#;
    assert_eq!(ts2416_count(source), 1);
}

/// Multiple type parameters: only the second is stricter. The unconstrained
/// first parameter must stay bound to the base marker (so it is NOT erased to
/// `unknown`, which would also break the return position), while the stricter
/// second parameter is rejected.
#[test]
fn one_of_two_type_params_stricter_emits_ts2416() {
    let source = r#"
class Base { foo<X, Y>(a: X, b: Y): void {} }
class Derived extends Base { foo<A, B extends object>(a: A, b: B): void {} }
"#;
    assert_eq!(ts2416_count(source), 1);
}

// ---------------------------------------------------------------------------
// Negative / fallback cases: these must NOT regress into false positives.
// ---------------------------------------------------------------------------

/// Renaming type parameters with identical constraints is a pure alpha-rename
/// and must stay accepted.
#[test]
fn renamed_type_params_same_constraint_no_ts2416() {
    let source = r#"
class Base { foo<X, Y>(a: X, b: Y): X { return a; } }
class Derived extends Base { foo<A, B>(a: A, b: B): A { return a; } }
"#;
    assert_eq!(ts2416_count(source), 0);
}

/// Derived *loosens* the constraint (drops it / widens it). The derived method
/// accepts a superset of the base's inputs, so the override is sound.
#[test]
fn derived_loosens_constraint_no_ts2416() {
    let source = r#"
class Base { foo<T extends string>(x: T): T { return x; } }
class Derived extends Base { foo<U>(x: U): U { return x; } }
"#;
    assert_eq!(ts2416_count(source), 0);
}

/// An unused stricter type parameter alongside a used unconstrained one stays
/// accepted, because the unconstrained parameter remains bound to the base
/// marker and the stricter (unused) one does not affect any value position.
#[test]
fn unused_stricter_type_param_no_ts2416() {
    let source = r#"
class Base { foo<X, Y>(a: X): X { return a; } }
class Derived extends Base { foo<A, B extends object>(a: A): A { return a; } }
"#;
    assert_eq!(ts2416_count(source), 0);
}

/// Constraint difference reconciled by parameter usage: the derived constraint
/// looks narrower in isolation, but the way the parameters consume the type
/// parameter makes the erased shapes equivalent, so tsc accepts it. This guards
/// against an over-eager rejection.
#[test]
fn constraint_difference_reconciled_by_param_usage_no_ts2416() {
    let source = r#"
interface Source { value<T extends { p: string }>(x: T[]): void; }
interface Target extends Source { value<U extends { p: string }[]>(x: U): void; }
"#;
    // tsc accepts this pair (the erased parameter shapes coincide).
    assert_eq!(ts2416_count(source), 0);
}
