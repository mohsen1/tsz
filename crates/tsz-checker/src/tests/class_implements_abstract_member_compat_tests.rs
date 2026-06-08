//! Regression tests for issue #10804 — member compatibility on **abstract**
//! classes that `implements` an interface.
//!
//! An abstract class is exempt only from the *completeness* requirement: it need
//! not provide an implementation for every interface member (a concrete subclass
//! does, and `tsc` enforces that on the subclass). But a member the abstract
//! class *does* declare — concrete or `abstract` — must still be type- and
//! visibility-compatible with the interface. `tsc` reports TS2416 / TS2420 for an
//! incompatible member regardless of whether the class is abstract.
//!
//! Previously `tsz` short-circuited the entire `implements` member check for any
//! abstract class, so it silently accepted genuinely incompatible members (a
//! false negative — the symmetric gap to the `implements` false-positive closed
//! in PR #12848). These tests pin the abstract form to the same per-member
//! variance decision used for concrete classes, while keeping abstract classes
//! exempt from the missing-member / weak-type / whole-type completeness
//! diagnostics.
//!
//! The decision is structural — keyed on the signature shapes, not on any
//! identifier — so the cases below vary the method, interface, and type-parameter
//! names.

use crate::test_utils::check_source_codes;

fn assert_has_2416(src: &str) {
    let codes = check_source_codes(src);
    assert!(
        codes.contains(&2416),
        "expected TS2416, got none. Got: {codes:?}\nSource:\n{src}"
    );
}

fn assert_no_2416(src: &str) {
    let codes = check_source_codes(src);
    assert!(
        !codes.contains(&2416),
        "unexpected TS2416 (false positive). Got: {codes:?}\nSource:\n{src}"
    );
}

// ---------------------------------------------------------------------------
// Must REPORT TS2416 — declared member on an abstract class is incompatible
// with the interface. These were false negatives before the fix (the abstract
// class short-circuited the whole check).
// ---------------------------------------------------------------------------

// Plain property type mismatch on an abstract member.
#[test]
fn reports_2416_abstract_property_type_mismatch() {
    assert_has_2416(
        "interface Spec { x: number; }
         abstract class Svc implements Spec { abstract x: string; }",
    );
}

// Plain method return-type mismatch on an abstract member.
#[test]
fn reports_2416_abstract_method_return_mismatch() {
    assert_has_2416(
        "interface Spec { read(): number; }
         abstract class Svc implements Spec { abstract read(): string; }",
    );
}

// The documented witness: a method-local generic used covariantly in the
// return cannot be satisfied by a concrete return type, even abstractly.
#[test]
fn reports_2416_abstract_covariant_return_generic_drop() {
    assert_has_2416(
        "interface Spec { run<T>(): T; }
         abstract class Svc implements Spec { abstract run(): string; }",
    );
}

// Renamed method, interface, and type parameter — proves the rule is
// structural, not identifier-keyed.
#[test]
fn reports_2416_abstract_covariant_return_generic_drop_renamed() {
    assert_has_2416(
        "interface Contract { produce<U>(): U; }
         abstract class Worker implements Contract { abstract produce(): number; }",
    );
}

// Generic in param AND return: still covariant in the return, still rejected.
#[test]
fn reports_2416_abstract_in_out_generic_drop() {
    assert_has_2416(
        "interface Spec { map<T>(x: T): T; }
         abstract class Svc implements Spec { abstract map(x: string): string; }",
    );
}

// A *concrete* method on an abstract class is also checked (mixed members).
#[test]
fn reports_2416_abstract_class_concrete_member_mismatch() {
    assert_has_2416(
        "interface Spec { id(): number; other(): void; }
         abstract class Svc implements Spec {
             id(): string { return \"\"; }
             abstract other(): void;
         }",
    );
}

// The incompatible member is *inherited* from a base class of the abstract
// class — the inherited-member branch must run for abstract classes too.
#[test]
fn reports_2416_abstract_class_inherited_member_mismatch() {
    assert_has_2416(
        "interface Spec { value(): number; }
         class BaseImpl { value(): string { return \"\"; } }
         abstract class Svc extends BaseImpl implements Spec {}",
    );
}

// ---------------------------------------------------------------------------
// Must NOT report — abstract classes stay exempt from completeness, and the
// input-only generic-drop specialization stays valid (no false positives).
// ---------------------------------------------------------------------------

// Member left entirely unimplemented: legal for an abstract class.
#[test]
fn no_2416_abstract_class_member_left_unimplemented() {
    let codes = check_source_codes(
        "interface Spec { run(): number; other(): string; }
         abstract class Svc implements Spec {}",
    );
    assert!(
        !codes.contains(&2416) && !codes.contains(&2420),
        "abstract class may leave interface members unimplemented; got {codes:?}"
    );
}

// Declared abstract member that is exactly compatible: no error.
#[test]
fn no_2416_abstract_member_compatible() {
    assert_no_2416(
        "interface Spec { run(): number; }
         abstract class Svc implements Spec { abstract run(): number; }",
    );
}

// Input-only generic dropped to its constraint on an abstract member: the wider
// concrete parameter admits every instantiation, so this stays valid (the
// false-positive suppression must still apply on the abstract path).
#[test]
fn no_2416_abstract_input_only_generic_drop() {
    assert_no_2416(
        "interface Builder { with<K extends string>(k: K): Builder; }
         abstract class Impl implements Builder { abstract with(k: string): Builder; }",
    );
}

// Renamed binders for the input-only acceptance case.
#[test]
fn no_2416_abstract_input_only_generic_drop_renamed() {
    assert_no_2416(
        "interface Factory { make<P extends string>(p: P): Factory; }
         abstract class Plant implements Factory { abstract make(p: string): Factory; }",
    );
}

// A covariant-subtype return on an abstract member is still a valid override.
#[test]
fn no_2416_abstract_member_covariant_subtype_return() {
    assert_no_2416(
        "interface Animal { kind: string; }
         interface Dog extends Animal { bark(): void; }
         interface Spec { pet(): Animal; }
         abstract class Shelter implements Spec { abstract pet(): Dog; }",
    );
}

// A *compatible* member inherited from a base class satisfies the interface on
// an abstract class with no false positive (success path of the inherited
// branch, complementing the inherited-mismatch case above).
#[test]
fn no_2416_abstract_class_inherited_member_compatible() {
    assert_no_2416(
        "interface Spec { value(): number; }
         class BaseImpl { value(): number { return 42; } }
         abstract class Svc extends BaseImpl implements Spec {}",
    );
}

// An interface that inherits an inaccessible private brand from a base class is
// a whole-type completeness check; an abstract class that leaves those members
// for a concrete subclass stays exempt (no false TS2420 / TS2416).
#[test]
fn no_error_abstract_class_unimplemented_inaccessible_private_interface() {
    let codes = check_source_codes(
        "class Secret { private s: number = 0; }
         interface Spec extends Secret { run(): void; }
         abstract class Svc implements Spec {}",
    );
    assert!(
        !codes.contains(&2416) && !codes.contains(&2420),
        "abstract class may leave brand-carrying members unimplemented; got {codes:?}"
    );
}
