//! Regression coverage: `protected` member assignability is *hierarchical*,
//! not strictly nominal like `private`.
//!
//! Tracks the `binder-11-20` "augmentation drops private/protected mirror
//! visibility" family (mohsen1/tsz#11631).
//!
//! Structural rule under test:
//!
//! > When a target property is `protected`, a source object is assignable as
//! > long as the source property is declared in the target's
//! > protected-declaring class *or in a class derived from it*. The source is
//! > allowed to widen the member from `protected` to `public` in the derived
//! > class. (`private` is unaffected: it still requires the exact same
//! > declaration — declaration identity.)
//!
//! This mirrors tsc's `isPropertyInClassDerivedFrom` / `isValidOverrideOf`.
//! Behavior was verified against `tsc` 6.0.2 for every case below.
//!
//! Per the anti-hardcoding directive (§25) the cases vary the user-chosen
//! member and class names (`p`/`kind`/`items`, `Base`/`Shape`/`Repo`) and
//! include a generic variant, so a fix keyed on a specific spelling would fail
//! at least one case.

use tsz_checker::test_utils::{check_source_strict, diagnostic_codes, diagnostics_without_codes};

/// Semantic error codes, dropping TS2318 ("cannot find global type") which is
/// expected noise in the no-stdlib unit harness.
fn errors(source: &str) -> Vec<u32> {
    diagnostic_codes(&diagnostics_without_codes(
        &check_source_strict(source),
        &[2318],
    ))
}

/// A subclass that widens an inherited `protected` member to `public` is still
/// assignable to its base. tsc accepts the assignment and only reports the
/// later out-of-class `protected` access (TS2445).
#[test]
fn widen_protected_to_public_then_assign_to_base() {
    let codes = errors(
        r#"
class Base { protected p = 1; }
class Sub extends Base { public p = 2; }

const s = new Sub();
s.p;                  // ok: public on Sub
const b: Base = s;    // ok: Sub is assignable to Base (widening is legal)
b.p;                  // TS2445: still protected through the Base view
"#,
    );
    assert_eq!(
        codes,
        vec![2445],
        "widening protected->public must not block Sub -> Base assignment; got {codes:?}"
    );
}

/// The same rule across a multi-level chain and with different member/class
/// spellings, including a class that only inherits the protected member.
#[test]
fn widen_protected_deep_chain_independent_of_names() {
    let codes = errors(
        r#"
class Animal { protected kind = "a"; }
class Pet extends Animal {}
class Dog extends Pet { public kind = "d"; }

const a: Animal = new Dog();   // ok
const p: Pet = new Dog();      // ok
"#,
    );
    assert!(
        codes.is_empty(),
        "deep-chain widening must be assignable for every base; got {codes:?}"
    );
}

/// A derived class that keeps the member `protected` (a plain override) is also
/// assignable to its base — declaration identity is *not* required for
/// protected members.
#[test]
fn protected_override_stays_protected_assigns_to_base() {
    let codes = errors(
        r#"
class Shape { protected kind = "s"; }
class Circle extends Shape { protected kind = "c"; }
const sh: Shape = new Circle();
"#,
    );
    assert!(
        codes.is_empty(),
        "protected override in a derived class must remain assignable to base; got {codes:?}"
    );
}

/// Generic + renamed-identifier variant: widening a generic protected member to
/// public in a concrete subclass stays assignable to the generic base.
#[test]
fn widen_protected_generic_base() {
    let codes = errors(
        r#"
abstract class Repo<E> { protected items: E[] = []; }
class UserRepo extends Repo<string> { public items: string[] = []; }
const r: Repo<string> = new UserRepo();
"#,
    );
    assert!(
        codes.is_empty(),
        "generic protected widening must stay assignable; got {codes:?}"
    );
}

/// Index-signature variant: a class that carries a string index signature is
/// compared through the with-index object path. Widening a protected member to
/// public there must also stay assignable to the base (covers the third
/// nominal-decision call site).
#[test]
fn widen_protected_with_index_signature() {
    let codes = errors(
        r#"
class Base { protected p = 1; [k: string]: unknown; }
class Sub extends Base { public p = 2; [k: string]: unknown; }
const b: Base = new Sub();
"#,
    );
    assert!(
        codes.is_empty(),
        "protected widening must stay assignable through the index-signature path; got {codes:?}"
    );
}

/// Negative guard: an *unrelated* class whose public member happens to share the
/// name of a target's protected member is NOT assignable. The derived-class
/// relationship is required.
#[test]
fn unrelated_class_with_public_member_is_rejected() {
    let codes = errors(
        r#"
class Guarded { protected p = 1; }
class Loose { p = 2; }            // unrelated, not derived from Guarded
const g: Guarded = new Loose();   // TS2322: not a class derived from Guarded
"#,
    );
    assert_eq!(
        codes,
        vec![2322],
        "an unrelated class must not satisfy a protected member; got {codes:?}"
    );
}

/// Negative guard: two *unrelated* classes that each declare a `protected`
/// member of the same name are not interchangeable.
#[test]
fn unrelated_protected_declarations_are_rejected() {
    let codes = errors(
        r#"
class Left { protected tag = 1; }
class Right { protected tag = 2; }
const l: Left = new Right(); // TS2322: different protected-declaring classes
"#,
    );
    assert_eq!(
        codes,
        vec![2322],
        "unrelated protected declarations must not be assignable; got {codes:?}"
    );
}

/// Guard: `private` remains *strictly* nominal. Two classes with an identical
/// shape but separate `private` declarations are not assignable, proving the
/// fix only relaxed `protected`, not `private`.
#[test]
fn private_members_remain_strictly_nominal() {
    let codes = errors(
        r#"
class Token { private brand: void = undefined; id = 1; }
class Ticket { private brand: void = undefined; id = 1; }
const t: Token = new Ticket(); // TS2322: separate private declarations
"#,
    );
    assert_eq!(
        codes,
        vec![2322],
        "separate private declarations must stay non-assignable; got {codes:?}"
    );
}

/// Guard: narrowing visibility on override (public -> protected) is still a
/// class-extension error (TS2415), unchanged by this fix.
#[test]
fn narrowing_public_to_protected_still_errors() {
    let codes = errors(
        r#"
class Base { p = 1; }
class Sub extends Base { protected p = 2; }
"#,
    );
    assert_eq!(
        codes,
        vec![2415],
        "narrowing public->protected on override must still report TS2415; got {codes:?}"
    );
}
