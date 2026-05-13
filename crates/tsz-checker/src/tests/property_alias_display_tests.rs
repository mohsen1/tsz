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

#[test]
fn ts2339_long_merge_receiver_keeps_initial_object_args_before_truncation() {
    let diags = check_source_diagnostics(
        r#"
type Omit<T, K extends keyof any> = Pick<T, Exclude<keyof T, K>>;
type Pick<T, K extends keyof T> = { [P in K]: T[P] };
type Exclude<T, U> = T extends U ? never : T;
type merge<base, props> = Omit<base, keyof props & keyof base> & props;
type Type<t> = {
  shape: t;
  merge: <r>(r: r) => Type<merge<t, r>>;
};

declare const o1: Type<{ p1: 1 }>;
const o2 = o1.merge({ p2: 2 });
const o3 = o2.merge({ p3: 3 });
const o4 = o3.merge({ p4: 4 });
const o5 = o4.merge({ p5: 5 });
const o6 = o5.merge({ p6: 6 });
const o7 = o6.merge({ p7: 7 });
const o8 = o7.merge({ p8: 8 });
const o9 = o8.merge({ p9: 9 });
const o10 = o9.merge({ p10: 10 });
const o11 = o10.merge({ p11: 11 });
const o12 = o11.merge({ p12: 12 });
const o13 = o12.merge({ p13: 13 });
const o14 = o13.merge({ p14: 14 });
const o15 = o14.merge({ p15: 15 });
const o16 = o15.merge({ p16: 16 });

o16.shape.p17;
"#,
    );

    let ts2339: Vec<_> = diags.iter().filter(|d| d.code == 2339).collect();
    assert_eq!(ts2339.len(), 1, "Expected one TS2339, got: {diags:?}");
    let msg = &ts2339[0].message_text;
    assert!(
        msg.contains("{ p1: 1; }, { p2: number; }"),
        "long merge receiver should preserve initial concrete object args before truncation, got: {msg:?}"
    );
    assert!(
        !msg.contains("merge<{ ...; }, { ...; }"),
        "long merge receiver must not pre-elide the innermost object args, got: {msg:?}"
    );
    assert!(
        !msg.contains("{ p6: number; }"),
        "long merge receiver should elide later object args before final truncation, got: {msg:?}"
    );
}

#[test]
fn ts2339_merge_function_receiver_widens_fresh_object_literals() {
    let diags = check_source_diagnostics(
        r#"
type Omit<T, K extends keyof any> = Pick<T, Exclude<keyof T, K>>;
type Pick<T, K extends keyof T> = { [P in K]: T[P] };
type Exclude<T, U> = T extends U ? never : T;
type merge<base, props> = keyof base & keyof props extends never
  ? base & props
  : Omit<base, keyof props & keyof base> & props;

declare const merge: <l, r>(l: l, r: r) => merge<l, r>;

const o1 = merge({ p1: 1 }, { p2: 2 });
const o2 = merge(o1, { p2: 2, p3: 3 });

o1.p51;
o2.p4;
"#,
    );

    let ts2339: Vec<_> = diags.iter().filter(|d| d.code == 2339).collect();
    assert_eq!(ts2339.len(), 2, "Expected two TS2339, got: {diags:?}");

    let messages = ts2339
        .iter()
        .map(|diag| diag.message_text.as_str())
        .collect::<Vec<_>>();
    assert!(
        messages
            .iter()
            .any(|msg| msg.contains("merge<{ p1: number; }, { p2: number; }>")),
        "o1 receiver should display widened object-literal property types through merge, got: {messages:#?}"
    );
    assert!(
        messages.iter().any(|msg| msg.contains(
            "merge<merge<{ p1: number; }, { p2: number; }>, { p2: number; p3: number; }>"
        )),
        "o2 receiver should display widened object-literal property types through merge, got: {messages:#?}"
    );
    assert!(
        !messages
            .iter()
            .any(|msg| msg.contains("{ p1: 1; }") || msg.contains("{ p2: 2; }")),
        "TS2339 property receivers should not preserve fresh object literal values, got: {messages:#?}"
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
