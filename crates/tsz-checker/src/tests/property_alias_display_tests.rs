//! Regression tests for type-alias preservation in TS2339 receiver displays.
//!
//! Background: when a receiver has a declared annotation `bar: Bar` where
//! `type Bar = Omit<Foo, "c">`, tsc preserves the alias name `Bar` in the
//! TS2339 message. tsz used to expand to `Omit<Foo, "c">` because its
//! `display_alias` only tracks one level back (to the Application) and the
//! annotation-bridge previously required `<` in the annotation text.
//!
//! Fix: drop the `contains('<')` requirement so simple-alias annotations are
//! also bridged. See `crates/tsz-checker/src/error_reporter/properties.rs` —
//! `property_receiver_display_for_node`.
//!
//! These tests use a hand-rolled stub instead of lib's `Omit<>` so they run
//! without lib contexts loaded.

use crate::test_utils::check_source_diagnostics;

/// Mirrors `compiler/omitTypeTestErrors01.ts` in shape: a non-generic alias
/// whose RHS is a generic Application. Receiver type is the alias.
#[test]
fn ts2339_preserves_simple_alias_for_generic_application_rhs() {
    let diags = check_source_diagnostics(
        r#"
type Pick2<T, K extends keyof T> = { [P in K]: T[P] };
interface Foo { a: string; b: number; c: boolean; }
type Bar = Pick2<Foo, "a" | "b">;
declare const bar: Bar;
const x = bar.c;
"#,
    );

    let ts2339: Vec<_> = diags.iter().filter(|d| d.code == 2339).collect();
    assert_eq!(
        ts2339.len(),
        1,
        "Expected exactly one TS2339. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );

    let msg = &ts2339[0].message_text;
    assert!(
        msg.contains("'Bar'"),
        "TS2339 should preserve alias 'Bar', got: {msg:?}"
    );
    assert!(
        !msg.contains("Pick2<"),
        "TS2339 must not expand to 'Pick2<...>' when receiver was annotated as 'Bar'. Got: {msg:?}"
    );
}

/// Companion: keep generic-instantiation behavior intact (the path was already
/// covered when annotation contained `<`).
#[test]
fn ts2339_preserves_generic_alias_application() {
    let diags = check_source_diagnostics(
        r#"
type Pick2<T, K extends keyof T> = { [P in K]: T[P] };
interface Foo { a: string; b: number; c: boolean; }
declare const bar: Pick2<Foo, "a" | "b">;
const x = bar.c;
"#,
    );

    let ts2339: Vec<_> = diags.iter().filter(|d| d.code == 2339).collect();
    assert_eq!(ts2339.len(), 1);
    let msg = &ts2339[0].message_text;
    assert!(
        msg.contains("Pick2<"),
        "TS2339 should keep the generic Pick2 application. Got: {msg:?}"
    );
}

/// Negative lock: when the receiver has NO declared annotation, the formatter
/// falls back to `format_type` — must not crash or emit an alias name.
#[test]
fn ts2339_no_annotation_falls_back_cleanly() {
    let diags = check_source_diagnostics(
        r#"
const bar = { a: "x" };
const y = (bar as { a: string }).b;
"#,
    );

    let ts2339: Vec<_> = diags.iter().filter(|d| d.code == 2339).collect();
    assert_eq!(ts2339.len(), 1);
    let msg = &ts2339[0].message_text;
    assert!(
        msg.contains("'b'"),
        "TS2339 should mention property 'b'. Got: {msg:?}"
    );
}
