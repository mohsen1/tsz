//! `Object.defineProperty` descriptor typing must be gated by the actual
//! builtin/global `Object` value, not by an arbitrary binding named `Object`.

use crate::test_utils::{check_source_diagnostics, diagnostic_codes};

fn assert_no_code(source: &str, code: u32) {
    let diags = check_source_diagnostics(source);
    let codes = diagnostic_codes(&diags);
    assert!(
        !codes.contains(&code),
        "expected no TS{code}, got codes {codes:?}\nDiagnostics: {diags:#?}",
    );
}

#[test]
fn local_object_define_property_uses_local_descriptor_context() {
    assert_no_code(
        r#"
const Object = {
    defineProperty(
        target: {},
        key: string,
        descriptor: any
    ) {}
};

Object.defineProperty({}, "x", {
    get() { return 1; },
    set(value) {
        value.toUpperCase();
    }
});
"#,
        2339,
    );
}

#[test]
fn object_define_property_descriptor_gate_uses_global_identity() {
    let source = include_str!("../types/computation/object_literal/mod.rs");
    assert!(
        source.contains("object_define_property_base_is_global_object"),
        "Object.defineProperty descriptor detection must prove the base is the global/lib Object value"
    );
    assert!(
        !source.contains("object_ident.escaped_text != \"Object\""),
        "Object.defineProperty descriptor detection must not rely on the raw Object spelling"
    );
    assert!(
        source.contains("symbol_is_from_actual_or_cloned_lib(sym_id)"),
        "Object.defineProperty descriptor detection must admit only proven actual/cloned lib identity"
    );
}

#[test]
fn unresolved_object_define_property_without_libs_is_not_global_identity() {
    assert_no_code(
        r#"
Object.defineProperty({}, "x", {
    get() { return 1; },
    set(value) {
        value.toUpperCase();
    }
});
"#,
        2339,
    );
}

#[test]
fn renamed_local_object_define_property_is_unchanged() {
    assert_no_code(
        r#"
const LocalObject = {
    defineProperty(
        target: {},
        key: string,
        descriptor: { get(): number; set(value: string): void }
    ) {}
};

LocalObject.defineProperty({}, "x", {
    get() { return 1; },
    set(value) {
        value.toUpperCase();
    }
});
"#,
        2339,
    );
}
