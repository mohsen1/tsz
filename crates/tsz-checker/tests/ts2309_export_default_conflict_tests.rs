//! TS2309 ("An export assignment cannot be used in a module with other
//! exported elements") for `export =` mixed with a **default** export, and its
//! module-kind independence.
//!
//! Structural rule (verified against `typescript@7.0.2`): TS2309 fires when a
//! module has exactly one `export =` and at least one other **value-meaning**
//! export — named, re-exported, or a value `export default` — regardless of the
//! module target (commonjs / esnext / node / `preserve`). Type-only exports
//! (interfaces, type aliases, `export default interface`) never count.
//!
//! Two gaps this suite pins:
//!   * a value `export default` (`export default 1` / `function` / `class`) was
//!     not counted as an "other exported element", so `export = X` alongside it
//!     silently missed TS2309; and
//!   * TS2309 was suppressed under `module: preserve`, which tsc does not do.
//!
//! Binder names are varied across rows so the check keys off structure, not a
//! fixed identifier.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;
use tsz_common::common::ModuleKind;

fn codes(source: &str, module: ModuleKind) -> Vec<u32> {
    let options = CheckerOptions {
        module,
        ..CheckerOptions::default()
    };
    check_source(source, "test.ts", options)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

// ---------------------------------------------------------------------------
// Value `export default` counts as an "other exported element" (TS2309).
// ---------------------------------------------------------------------------

#[test]
fn export_equals_with_default_expression_emits_ts2309_commonjs() {
    let src = "declare const foo: number;\nexport = foo;\nexport default 2;\n";
    let got = codes(src, ModuleKind::CommonJS);
    assert!(
        got.contains(&2309),
        "export= + `export default <expr>` must emit TS2309, got: {got:?}"
    );
}

#[test]
fn export_equals_with_default_function_emits_ts2309_commonjs() {
    let src = "declare const bar: number;\nexport = bar;\nexport default function fn() {}\n";
    let got = codes(src, ModuleKind::CommonJS);
    assert!(
        got.contains(&2309),
        "export= + `export default function` must emit TS2309, got: {got:?}"
    );
}

#[test]
fn export_equals_with_default_class_emits_ts2309_commonjs() {
    let src = "declare const baz: number;\nexport = baz;\nexport default class Widget {}\n";
    let got = codes(src, ModuleKind::CommonJS);
    assert!(
        got.contains(&2309),
        "export= + `export default class` must emit TS2309, got: {got:?}"
    );
}

#[test]
fn export_equals_with_default_object_literal_emits_ts2309_commonjs() {
    let src = "declare const qux: number;\nexport = qux;\nexport default { a: 1 };\n";
    let got = codes(src, ModuleKind::CommonJS);
    assert!(
        got.contains(&2309),
        "export= + `export default <object literal>` must emit TS2309, got: {got:?}"
    );
}

#[test]
fn export_equals_with_default_expression_emits_ts2309_esnext_alongside_ts1203() {
    // Under an ESM target the format diagnostic TS1203 also fires; TS2309 must
    // fire in addition, not instead.
    let src = "declare const thing: number;\nexport = thing;\nexport default 7;\n";
    let got = codes(src, ModuleKind::ESNext);
    assert!(
        got.contains(&1203),
        "TS1203 should fire for export= under ESNext, got: {got:?}"
    );
    assert!(
        got.contains(&2309),
        "TS2309 should fire alongside TS1203 for export= + default, got: {got:?}"
    );
}

#[test]
fn export_equals_with_default_inside_ambient_module_emits_ts2309() {
    let src =
        "declare module \"ext\" {\n  const val: number;\n  export = val;\n  export default 2;\n}\n";
    let got = codes(src, ModuleKind::CommonJS);
    assert!(
        got.contains(&2309),
        "export= + `export default` inside an ambient module must emit TS2309, got: {got:?}"
    );
}

// ---------------------------------------------------------------------------
// Module-kind independence: `preserve` does not suppress TS2309.
// ---------------------------------------------------------------------------

#[test]
fn export_equals_with_default_emits_ts2309_under_preserve() {
    let src = "declare const foo: number;\nexport = foo;\nexport default 2;\n";
    let got = codes(src, ModuleKind::Preserve);
    assert!(
        got.contains(&2309),
        "`preserve` must not suppress TS2309 for export= + default, got: {got:?}"
    );
}

#[test]
fn export_equals_with_named_export_emits_ts2309_under_preserve() {
    let src = "declare const foo: number;\nexport = foo;\nexport const bar = 1;\n";
    let got = codes(src, ModuleKind::Preserve);
    assert!(
        got.contains(&2309),
        "`preserve` must not suppress TS2309 for export= + named export, got: {got:?}"
    );
}

#[test]
fn export_equals_with_reexport_emits_ts2309_under_preserve() {
    let src = "declare const foo: number;\nexport = foo;\nexport { foo as baz };\n";
    let got = codes(src, ModuleKind::Preserve);
    assert!(
        got.contains(&2309),
        "`preserve` must not suppress TS2309 for export= + re-export, got: {got:?}"
    );
}

// ---------------------------------------------------------------------------
// Negative controls: type-only defaults and a lone export= do not fire.
// ---------------------------------------------------------------------------

#[test]
fn export_equals_with_default_interface_does_not_emit_ts2309() {
    let src = "declare const foo: number;\nexport = foo;\nexport default interface Shape {}\n";
    let got = codes(src, ModuleKind::CommonJS);
    assert!(
        !got.contains(&2309),
        "a type-only `export default interface` must NOT count for TS2309, got: {got:?}"
    );
}

#[test]
fn lone_export_equals_does_not_emit_ts2309_under_preserve() {
    let src = "declare const foo: number;\nexport = foo;\n";
    let got = codes(src, ModuleKind::Preserve);
    assert!(
        !got.contains(&2309),
        "a lone export= (no other exports) must NOT emit TS2309, got: {got:?}"
    );
}

#[test]
fn lone_export_equals_does_not_emit_ts2309_commonjs() {
    let src = "declare const foo: number;\nexport = foo;\n";
    let got = codes(src, ModuleKind::CommonJS);
    assert!(
        !got.contains(&2309),
        "a lone export= (no other exports) must NOT emit TS2309, got: {got:?}"
    );
}

#[test]
fn export_equals_with_default_type_reference_does_not_over_report_ts2309() {
    // `export default <Ident>` where the identifier names a type must not draw a
    // spurious TS2309 (the syntax-only predicate leaves references uncounted).
    let src =
        "declare const foo: number;\ninterface Only {}\nexport = foo;\nexport default Only;\n";
    let got = codes(src, ModuleKind::CommonJS);
    assert!(
        !got.contains(&2309),
        "`export default <type ref>` must NOT emit TS2309, got: {got:?}"
    );
}
