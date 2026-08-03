//! Regression tests for TS1168 — a computed method name in a *concrete*
//! (non-ambient) class must be a literal or `unique symbol`-typed expression
//! when the method has no body.
//!
//! `check_computed_property_requires_literal` already implements the shared
//! literal/entity-name/unique-symbol gate for TS1166 (class properties),
//! TS1169 (interfaces) and TS1170 (type literals); TS1168 is the same rule's
//! arm for a bodyless class *method* declaration outside an ambient context
//! (`declare class`/`.d.ts`, which is TS1165 instead). Oracle-verified
//! against pinned `typescript@7.0.2`.

use crate::test_utils::check_source_diagnostics;

fn diag_codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

// --- Positive: bodyless method, non-ambient, non-literal computed name ---

#[test]
fn ts1168_overload_signature_before_implementation() {
    let codes = diag_codes(
        r#"
declare const x: string;
class C {
    [`a${x}`](): void;
    [`a${x}`](y: number): void;
    [`a${x}`](y?: number): void { }
}
"#,
    );
    assert_eq!(
        codes.iter().filter(|&&c| c == 1168).count(),
        2,
        "Expected TS1168 on both bodyless overload signatures, not the implementation. Got: {codes:?}"
    );
}

#[test]
fn ts1168_standalone_abstract_method() {
    let codes = diag_codes(
        r#"
declare const x: string;
abstract class C {
    abstract [`a${x}`](): void;
}
"#,
    );
    assert!(
        codes.contains(&1168),
        "Expected TS1168 for a standalone bodyless abstract method. Got: {codes:?}"
    );
}

#[test]
fn ts1168_static_overload_signatures() {
    let codes = diag_codes(
        r#"
declare const x: string;
class C {
    static [`a${x}`](): void;
    static [`a${x}`](y: number): void;
    static [`a${x}`](y?: number): void { }
}
"#,
    );
    assert_eq!(
        codes.iter().filter(|&&c| c == 1168).count(),
        2,
        "Expected TS1168 on both static bodyless overload signatures. Got: {codes:?}"
    );
}

#[test]
fn ts1168_lone_overload_signature_with_no_implementation() {
    // A single bodyless method with nothing following it also carries TS1168
    // — the gate is "no body", not "part of a multi-signature overload set".
    let codes = diag_codes(
        r#"
declare const x: string;
class C {
    [`a${x}`](): void;
}
"#,
    );
    assert!(
        codes.contains(&1168),
        "Expected TS1168 for a lone bodyless method with a bad computed name. Got: {codes:?}"
    );
}

#[test]
fn ts1168_renamed_binder() {
    // Renamed-binder control: the diagnostic is keyed on shape, not on any
    // hardcoded identifier text.
    let codes = diag_codes(
        r#"
declare const zzTemplateOperand: string;
class Widget {
    [`prefix${zzTemplateOperand}`](): void;
    [`prefix${zzTemplateOperand}`](y: number): void;
    [`prefix${zzTemplateOperand}`](y?: number): void { }
}
"#,
    );
    assert_eq!(
        codes.iter().filter(|&&c| c == 1168).count(),
        2,
        "Expected TS1168 on both renamed-binder overload signatures. Got: {codes:?}"
    );
}

// --- Negative: implementation with a body never takes it ---

#[test]
fn ts1168_implementation_with_body_is_clean() {
    let codes = diag_codes(
        r#"
declare const x: string;
class C {
    [`a${x}`](): void { }
}
"#,
    );
    assert!(
        !codes.contains(&1168),
        "A method with a body must never take TS1168, even with a bad computed name. Got: {codes:?}"
    );
}

// --- Negative: entity-name / literal / unique-symbol computed names ---

#[test]
fn ts1168_string_literal_name_is_clean() {
    let codes = diag_codes(
        r#"
class C {
    ["abc"](): void;
    ["abc"](y: number): void;
    ["abc"](y?: number): void { }
}
"#,
    );
    assert!(
        !codes.contains(&1168),
        "A string-literal computed name must never take TS1168. Got: {codes:?}"
    );
}

#[test]
fn ts1168_unique_symbol_name_is_clean() {
    let codes = diag_codes(
        r#"
declare const sym: unique symbol;
class C {
    [sym](): void;
    [sym](y: number): void;
    [sym](y?: number): void { }
}
"#,
    );
    assert!(
        !codes.contains(&1168),
        "A unique-symbol-typed computed name must never take TS1168. Got: {codes:?}"
    );
}

#[test]
fn ts1168_entity_name_expression_is_clean() {
    let codes = diag_codes(
        r#"
declare const a: { b: string };
class C {
    [a.b](): void;
    [a.b](y: number): void;
    [a.b](y?: number): void { }
}
"#,
    );
    assert!(
        !codes.contains(&1168),
        "An entity-name-expression computed name must never take TS1168. Got: {codes:?}"
    );
}

// --- Negative: ambient context takes TS1165, not TS1168 ---

#[test]
fn ts1168_does_not_fire_in_declare_class() {
    // `declare class` is the ambient sibling rule (TS1165, not yet wired up
    // as of this test's authoring — tracked separately). This arm must not
    // fire here regardless.
    let codes = diag_codes(
        r#"
declare const x: string;
declare class C {
    [`a${x}`](): void;
}
"#,
    );
    assert!(
        !codes.contains(&1168),
        "TS1168 must not fire inside a `declare class` — that is the ambient sibling rule. Got: {codes:?}"
    );
}

// --- Negative: accessors are a different grammar arm (TS7032/TS7033), not TS1168 ---

#[test]
fn ts1168_does_not_fire_on_abstract_accessor() {
    let codes = diag_codes(
        r#"
declare const x: string;
abstract class C {
    abstract get [`a${x}`](): number;
}
"#,
    );
    assert!(
        !codes.contains(&1168),
        "An abstract accessor with a bad computed name must not take TS1168 — accessors are TS7033's domain. Got: {codes:?}"
    );
}

// --- Negative: interface/type-literal members are a different (already-correct) arm ---

#[test]
fn ts1168_does_not_fire_on_interface_method() {
    let codes = diag_codes(
        r#"
declare const x: string;
interface I {
    [`a${x}`](): void;
}
"#,
    );
    assert!(
        !codes.contains(&1168),
        "An interface method signature takes TS1169, never TS1168. Got: {codes:?}"
    );
    assert!(
        codes.contains(&1169),
        "Expected the interface's own TS1169 to still fire. Got: {codes:?}"
    );
}
