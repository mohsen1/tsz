//! A class that `implements` an interface carrying a **call** or **construct**
//! signature must report TS2420: a class instance can never be callable or
//! constructable, so the signature can never be satisfied.
//!
//! `tsc` reports TS2420 here for concrete *and* abstract classes alike —
//! `abstract` may defer an ordinary member (a subclass can implement it), but
//! no class instance can ever provide a call/construct signature, so the gap is
//! unclosable. Previously `tsz` only ran the whole-type assignability check for
//! `implements` when the interface had an *index* signature (or the class
//! extended the same base), so a call/construct signature — equally invisible to
//! the member-by-member walk — slipped through as a false negative.
//!
//! The decision is structural — keyed on the presence of a call/construct
//! signature in the implemented interface, not on any identifier — so the cases
//! below vary the interface, class, and member names.

use crate::test_utils::check_source_codes;

fn assert_has_2420(src: &str) {
    let codes = check_source_codes(src);
    assert!(
        codes.contains(&2420),
        "expected TS2420, got none. Got: {codes:?}\nSource:\n{src}"
    );
}

fn assert_no_2420(src: &str) {
    let codes = check_source_codes(src);
    assert!(
        !codes.contains(&2420),
        "unexpected TS2420 (false positive). Got: {codes:?}\nSource:\n{src}"
    );
}

// ---------------------------------------------------------------------------
// Must REPORT TS2420 — the interface carries a call or construct signature that
// no class instance can provide. These were false negatives before the fix.
// ---------------------------------------------------------------------------

#[test]
fn reports_2420_concrete_class_implements_call_signature_interface() {
    assert_has_2420(
        "interface Callable { (input: number): void; }
         class Widget implements Callable {}",
    );
}

#[test]
fn reports_2420_abstract_class_implements_call_signature_interface() {
    assert_has_2420(
        "interface Callable { (input: number): void; }
         abstract class Widget implements Callable {}",
    );
}

#[test]
fn reports_2420_concrete_class_implements_construct_signature_interface() {
    assert_has_2420(
        "interface Factory { new (): object; }
         class Registry implements Factory {}",
    );
}

#[test]
fn reports_2420_abstract_class_implements_construct_signature_interface() {
    assert_has_2420(
        "interface Factory { new (): object; }
         abstract class Registry implements Factory {}",
    );
}

// The call signature is unsatisfiable even when every *named* member of the
// interface is present on the class.
#[test]
fn reports_2420_call_signature_interface_with_all_named_members_present() {
    assert_has_2420(
        "interface Invoker { (arg: string): number; describe(): string; }
         class Service implements Invoker { describe() { return \"\"; } }",
    );
}

// An inherited call signature (from an extended interface) is equally
// unsatisfiable — the member-by-member walk sees no local call signature.
#[test]
fn reports_2420_inherited_call_signature() {
    assert_has_2420(
        "interface Base { (n: number): void; }
         interface Derived extends Base {}
         class Impl implements Derived {}",
    );
}

// Abstract class, call signature *and* a missing ordinary member: `tsc` still
// reports a single TS2420 driven by the unsatisfiable signature, even though
// the abstract class is otherwise allowed to defer the ordinary member.
#[test]
fn reports_2420_abstract_call_signature_with_missing_member() {
    assert_has_2420(
        "interface Handler { (event: string): void; handle(): void; }
         abstract class Node implements Handler {}",
    );
}

// Name variation to prove the rule is structural, not keyed on any identifier.
#[test]
fn reports_2420_call_signature_with_varied_names() {
    assert_has_2420(
        "interface Zeta { (q: boolean): boolean; }
         class Alpha implements Zeta {}",
    );
}

// ---------------------------------------------------------------------------
// Must NOT report TS2420 — no call/construct signature is involved, so the
// ordinary-member rules (including the abstract deferral exemption) still hold.
// These guard against the new whole-type trigger over-firing.
// ---------------------------------------------------------------------------

// Plain interface, class provides the member: clean.
#[test]
fn no_2420_class_satisfies_plain_interface() {
    assert_no_2420(
        "interface Spec { run(): void; }
         class Job implements Spec { run() {} }",
    );
}

// Abstract class deferring an ordinary member (no signature): still exempt.
#[test]
fn no_2420_abstract_class_defers_ordinary_member() {
    assert_no_2420(
        "interface Spec { run(): void; }
         abstract class Job implements Spec {}",
    );
}

// An interface whose only members are ordinary methods the class implements —
// the class *does* have a method named like a call, but no call signature.
#[test]
fn no_2420_named_method_is_not_a_call_signature() {
    assert_no_2420(
        "interface Spec { call(): void; }
         class Job implements Spec { call() {} }",
    );
}
