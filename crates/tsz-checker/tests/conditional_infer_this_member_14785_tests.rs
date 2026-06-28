//! `infer` extraction from a polymorphic-`this`-returning member.
//!
//! Regression coverage for issue #14785: `T extends { m(): infer S } ? S : F`
//! instantiated with a class/interface whose `m()` returns the polymorphic
//! `this` type. tsc binds `this` to the matched receiver
//! (`getTypeWithThisArgument`) and infers `S = receiver`, taking the true
//! branch. tsz collected no candidate for `S` (the member return stayed an
//! unsubstituted `ThisType`), so the conditional fell to the false branch and
//! a spurious `TS2322` was raised when the (correct) value was assigned.
//!
//! Structural rule: when an `infer` pattern matches a source member whose
//! declared type references the polymorphic `this`, the member type must be
//! read through its receiver (the matched source object) before candidate
//! collection. Owner: solver conditional-`infer` evaluation
//! (`evaluate_rules/infer_pattern_object_match.rs`).

/// Minimal lib surface so `class`/conditional/`infer` resolve without pulling
/// the real standard library into the test.
fn check(source: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_source_code_messages(&format!(
        r#"
interface Array<T> {{}}
interface Boolean {{}}
interface CallableFunction {{}}
interface Function {{}}
interface IArguments {{}}
interface NewableFunction {{}}
interface Number {{}}
interface Object {{}}
interface RegExp {{}}
interface String {{}}

{source}
"#
    ))
}

fn assignability_codes(diagnostics: &[(u32, String)]) -> Vec<&(u32, String)> {
    diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322 || *code == 2345)
        .collect()
}

/// Canonical witness (class receiver): the conditional must take the true
/// branch and infer `S = Node`, so assigning a `Node` to the result is sound.
#[test]
fn infer_this_returning_method_class_takes_true_branch() {
    let diagnostics = check(
        r#"
class Node { self(): this { return this; } }
type GetSelf<T> = T extends { self(): infer S } ? S : never;
type GN = GetSelf<Node>;
const g: GN = new Node();
"#,
    );
    let errs = assignability_codes(&diagnostics);
    assert!(
        errs.is_empty(),
        "infer S against a `this`-returning method must infer S = receiver (true branch); assigning the receiver must be clean.\nGot: {errs:#?}\nAll: {diagnostics:#?}"
    );
}

/// Same shape, varied binder/alias/property names (anti-hardcoding): the fix
/// must be structural, not keyed on any identifier.
#[test]
fn infer_this_returning_method_renamed_binders_takes_true_branch() {
    let diagnostics = check(
        r#"
class Gadget { fluent(): this { return this; } }
type Pull<Recv> = Recv extends { fluent(): infer Out } ? Out : never;
type PG = Pull<Gadget>;
const value: PG = new Gadget();
"#,
    );
    let errs = assignability_codes(&diagnostics);
    assert!(
        errs.is_empty(),
        "renamed-binder variant must also take the true branch.\nGot: {errs:#?}\nAll: {diagnostics:#?}"
    );
}

/// False-branch witness with a non-`never` false branch: if the true branch is
/// taken (correct), `S = Widget`, so assigning the result to the false-branch
/// literal `"FAIL"` must error `TS2322`. A *silent* result would prove the bug
/// (false branch, `S = "FAIL"`). The presence of TS2322 confirms parity.
#[test]
fn infer_this_returning_method_interface_does_not_take_false_branch() {
    let diagnostics = check(
        r#"
interface Widget { build(): this; }
type Extract1<T> = T extends { build(): infer S } ? S : "FAIL";
type R1 = Extract1<Widget>;
declare const w: R1;
const bad: "FAIL" = w;
"#,
    );
    let errs = assignability_codes(&diagnostics);
    assert!(
        !errs.is_empty(),
        "true branch makes R1 = Widget, which is not assignable to the false-branch literal \"FAIL\" -> expected TS2322. Silence proves the false-branch bug.\nGot: {errs:#?}\nAll: {diagnostics:#?}"
    );
}

/// Control: a concrete (non-`this`) return was already sound and must stay so —
/// `S = Payload`, true branch, clean assignment.
#[test]
fn infer_concrete_returning_method_control_stays_sound() {
    let diagnostics = check(
        r#"
class Payload {}
class Source { take(): Payload { return new Payload(); } }
type Taken<T> = T extends { take(): infer S } ? S : never;
type TS = Taken<Source>;
const p: TS = new Payload();
"#,
    );
    let errs = assignability_codes(&diagnostics);
    assert!(
        errs.is_empty(),
        "concrete-return control must remain sound.\nGot: {errs:#?}\nAll: {diagnostics:#?}"
    );
}
