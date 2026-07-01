//! Regression tests for ES5 CommonJS-exported class private-field storage
//! placement when the class also declares a static field initializer.
//!
//! `tsc` externalizes a CommonJS-exported class's private-field `WeakMap`
//! storage — `var _C_x;` at module scope, `_C_x = new WeakMap();` after the
//! class IIFE — ONLY when nothing forces the storage to stay inside the IIFE.
//! A static property with a runtime initializer (`static s = 3`) emits a
//! `C.s = ...` assignment inside the IIFE and keeps the private `WeakMap`
//! declaration/instantiation inside the IIFE alongside it. In that case the
//! module-scope lift must be suppressed, or `var _C_x;` is emitted twice —
//! once (spuriously) at module scope and once inside the IIFE.
//!
//! Source: `crates/tsz-emitter/src/emitter/declarations/class/emit_declaration.rs`
//! (`es5_class_externally_hoisted_decls` / `class_has_es5_static_field_initializer`).

use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_emitter::output::printer::PrintOptions;

#[path = "test_support.rs"]
mod test_support;

use test_support::parse_and_lower_print as parse_lower_print;

fn es5_cjs(source: &str) -> String {
    parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES5,
            module: ModuleKind::CommonJS,
            ..Default::default()
        },
    )
}

/// With a static field initializer present, the private-field `WeakMap` var
/// must appear exactly once, inside the IIFE (never lifted to module scope).
#[test]
fn static_field_initializer_keeps_private_storage_inside_iife() {
    let output = es5_cjs("export class C {\n  #y = 2;\n  static s = 3;\n}\n");

    assert_eq!(
        output.matches("var _C_y;").count(),
        1,
        "private-field WeakMap var must be declared exactly once (not duplicated \
         at module scope) when a static field initializer keeps it inside the \
         IIFE.\noutput:\n{output}"
    );

    // The single declaration must be the indented, inside-IIFE one, and the
    // WeakMap instantiation stays inside too.
    let var_pos = output.find("var _C_y;").expect("private WeakMap var");
    let init_pos = output
        .find("_C_y = new WeakMap();")
        .expect("private WeakMap instantiation");
    let return_pos = output.find("return C;").expect("class IIFE return");
    assert!(
        var_pos < init_pos && init_pos < return_pos,
        "declaration and instantiation must both stay inside the IIFE, before \
         `return C;`.\noutput:\n{output}"
    );
}

/// Multiple private fields with a static initializer: none may leak to module
/// scope.
#[test]
fn multiple_private_fields_with_static_field_stay_inside() {
    let output = es5_cjs("export class C {\n  #y = 2;\n  #z = 3;\n  static s = 1;\n}\n");
    // Both private WeakMaps are declared together on a single inside-IIFE line;
    // before the fix that same `var _C_y, _C_z;` line was also emitted at
    // module scope (a duplicate), so the count would be 2.
    assert_eq!(
        output.matches("var _C_y, _C_z;").count(),
        1,
        "both private WeakMap vars must be declared exactly once (not duplicated \
         at module scope).\noutput:\n{output}"
    );
}

/// Public instance fields alongside the private field and static initializer
/// must not change the outcome (this is the originally-reported shape).
#[test]
fn public_and_private_fields_with_static_field() {
    let output = es5_cjs("export class C {\n  x = 1;\n  #y = 2;\n  static s = 3;\n}\n");
    assert_eq!(output.matches("var _C_y;").count(), 1, "output:\n{output}");
}

/// Control: WITHOUT a static field initializer, a CommonJS-exported private
/// class still externalizes its storage — `var _C_y;` at module scope and no
/// inside-IIFE declaration. Guards against over-suppression.
#[test]
fn no_static_field_still_externalizes_private_storage() {
    let output = es5_cjs("export class C {\n  #y = 2;\n}\n");

    assert_eq!(output.matches("var _C_y;").count(), 1, "output:\n{output}");

    // Externalized: the WeakMap instantiation is emitted after the class IIFE
    // returns (module scope), not before `return C;`.
    let init_pos = output
        .find("_C_y = new WeakMap();")
        .expect("private WeakMap instantiation");
    let return_pos = output.find("return C;").expect("class IIFE return");
    assert!(
        return_pos < init_pos,
        "without a static field initializer the WeakMap instantiation must be \
         lifted to module scope (after the IIFE).\noutput:\n{output}"
    );
}

/// Control: a static *method* (no runtime field initializer) does not keep the
/// storage inside — it still externalizes.
#[test]
fn static_method_still_externalizes_private_storage() {
    let output = es5_cjs("export class C {\n  #y = 2;\n  static m() {}\n}\n");
    assert_eq!(output.matches("var _C_y;").count(), 1, "output:\n{output}");
    let init_pos = output
        .find("_C_y = new WeakMap();")
        .expect("private WeakMap instantiation");
    let return_pos = output.find("return C;").expect("class IIFE return");
    assert!(
        return_pos < init_pos,
        "a static method carries no runtime field initializer, so the storage \
         must still externalize.\noutput:\n{output}"
    );
}

/// Control: a declare-only static field (no initializer) still externalizes.
#[test]
fn declare_only_static_field_still_externalizes() {
    let output = es5_cjs("export class C {\n  #y = 2;\n  static s: number;\n}\n");
    let init_pos = output
        .find("_C_y = new WeakMap();")
        .expect("private WeakMap instantiation");
    let return_pos = output.find("return C;").expect("class IIFE return");
    assert!(
        return_pos < init_pos,
        "a declare-only static field has no runtime initializer, so the storage \
         must still externalize.\noutput:\n{output}"
    );
}
