//! An object/array-literal assignment to a property/element reference does not
//! flow-narrow the receiver to the literal's fresh (modifier-less) structure.
//!
//! Structural rule: when `r.a = <object literal>` narrows `r.a`, the flow type
//! must reduce `r.a`'s *declared* type (preserving its `readonly` modifiers)
//! rather than adopt the literal's mutable shape. A nested `readonly` member
//! therefore survives the assignment, so a later write through the same
//! reference (`r.a.b = …`) is still rejected (TS2540) — exactly as `tsc` does,
//! whether the outer property is `readonly` or mutable, and even inside a
//! constructor. A declared union is reduced to the assigned member (carrying its
//! own declared modifiers), so discriminant narrowing is unaffected.
//!
//! Owner layer: checker control-flow narrowing on assignment reference nodes
//! (`flow/control_flow/assignment.rs::assigned_type_respecting_access_read_surface`).
//!
//! All expectations below were cross-checked against `tsc` 6.0.2 with
//! `--strict --target es2022 --lib es2022`. Witness: #14787.

use crate::context::CheckerOptions;
use crate::test_utils::{check_with_options, diagnostic_count};

fn strict() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    }
}

fn ts2540(src: &str) -> usize {
    diagnostic_count(&check_with_options(src, strict()), 2540)
}

// --- positives: the literal assignment must not erase nested readonly ---

#[test]
fn nested_readonly_interface_property_write_after_outer_write() {
    // `r.a = { b: 2 }` is TS2540 on `a`; the receiver keeps `{ readonly b }`,
    // so `r.a.b = 3` is TS2540 on `b`. Two diagnostics, matching tsc.
    let src = "interface R { readonly a: { readonly b: number } }\n\
        declare const r: R;\n\
        r.a = { b: 2 };\n\
        r.a.b = 3;\n";
    assert_eq!(
        ts2540(src),
        2,
        "expected TS2540 on both the outer and nested readonly writes"
    );
}

#[test]
fn nested_readonly_write_renamed_binders() {
    // Anti-hardcoding: identical structure with different identifiers must
    // produce the same two diagnostics — the rule is structural, not name-based.
    let src = "interface Outer { readonly inner: { readonly leaf: number } }\n\
        declare const value: Outer;\n\
        value.inner = { leaf: 2 };\n\
        value.inner.leaf = 3;\n";
    assert_eq!(ts2540(src), 2);
}

#[test]
fn nested_readonly_via_mapped_type() {
    // A homomorphic `readonly` mapped application must be classified through
    // environment evaluation the same way as an explicit `readonly` member, at
    // both the outer and the nested level.
    let src = "type RO<T> = { readonly [K in keyof T]: T[K] };\n\
        type Inner = RO<{ b: number }>;\n\
        type T = RO<{ a: Inner }>;\n\
        declare const r: T;\n\
        r.a = { b: 2 };\n\
        r.a.b = 3;\n";
    assert_eq!(ts2540(src), 2);
}

#[test]
fn nested_readonly_element_access_write() {
    // Element-access spelling of the same write (`r["a"] = …; r["a"].b = …`).
    let src = "interface R { readonly a: { readonly b: number } }\n\
        declare const r: R;\n\
        r[\"a\"] = { b: 2 };\n\
        r[\"a\"].b = 3;\n";
    assert_eq!(ts2540(src), 2);
}

#[test]
fn readonly_class_instance_property_write() {
    // A readonly property on a class instance keeps its nested readonly shape
    // outside the constructor.
    let src = "class C { readonly a: { readonly b: number } = { b: 0 }; }\n\
        declare const c: C;\n\
        c.a = { b: 2 };\n\
        c.a.b = 3;\n";
    assert_eq!(ts2540(src), 2);
}

#[test]
fn mutable_outer_readonly_nested_still_rejects_nested_write() {
    // `a` is mutable (only `b` is readonly): the write to `a` is legal (no
    // TS2540 on `a`), but the receiver must NOT narrow to the mutable literal —
    // the nested `readonly b` survives, so `m.a.b = 3` is TS2540 on `b`. One
    // diagnostic, matching tsc.
    let src = "interface M { a: { readonly b: number } }\n\
        declare const m: M;\n\
        m.a = { b: 2 };\n\
        m.a.b = 3;\n";
    assert_eq!(
        ts2540(src),
        1,
        "the nested readonly member must survive a legal mutable-outer write"
    );
}

#[test]
fn readonly_nested_survives_constructor_initialization() {
    // The outer readonly init `this.a = { b: 1 }` is legal in the constructor
    // (no TS2540 on `a`), but the nested `readonly b` still survives, so
    // `this.a.b = 2` is TS2540 on `b` — matching tsc.
    let src = "class C {\n\
        readonly a: { readonly b: number };\n\
        constructor() {\n\
        this.a = { b: 1 };\n\
        this.a.b = 2;\n\
        }\n\
        }\n";
    assert_eq!(ts2540(src), 1);
}

#[test]
fn direct_nested_readonly_write_without_outer_write() {
    // Read-first / no prior write: a bare `r.a.b = 3` is a single TS2540 on `b`.
    // Guards against the fix changing the baseline single-diagnostic case.
    let src = "interface R { readonly a: { readonly b: number } }\n\
        declare const r: R;\n\
        r.a.b = 3;\n";
    assert_eq!(ts2540(src), 1);
}

// --- negatives: a fully mutable shape must still narrow / stay writable ---

#[test]
fn mutable_nested_member_stays_writable() {
    // Nothing is readonly: the receiver narrows as before and the nested write
    // is permitted. No TS2540, in or out of a constructor.
    let src = "interface M { a: { b: number } }\n\
        declare const m: M;\n\
        m.a = { b: 2 };\n\
        m.a.b = 3;\n\
        class C {\n\
        readonly a: { b: number };\n\
        constructor() { this.a = { b: 1 }; this.a.b = 2; }\n\
        }\n";
    assert_eq!(
        ts2540(src),
        0,
        "a fully mutable nested shape must remain writable after assignment"
    );
}
