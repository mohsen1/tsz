//! Tests for TS2344 on non-public constructor visibility.

#[path = "ts2344_class_constructor_constraint_common.rs"]
mod common;

use common::{compile_and_get_diagnostics, diagnostics_for_code};

/// `InstanceType<typeof C>` must reject private constructors because the
/// constraint requires a public constructor signature.
#[test]
fn instance_type_of_private_constructor_emits_ts2344() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
type InstanceType<T extends abstract new (...args: any) => any> =
    T extends abstract new (...args: any) => infer R ? R : any;

class WithPrivateCtor {
    private constructor() {}
}

type Bad = InstanceType<typeof WithPrivateCtor>;
"#,
    );
    let ts2344_errors = diagnostics_for_code(&diagnostics, 2344);
    assert_eq!(
        ts2344_errors.len(),
        1,
        "Should emit one TS2344 for InstanceType<typeof WithPrivateCtor> with a private constructor.\nGot: {ts2344_errors:#?}\nAll: {diagnostics:#?}"
    );
}

/// `InstanceType<typeof C>` must reject protected constructors for the same
/// public-constructor constraint.
#[test]
fn instance_type_of_protected_constructor_emits_ts2344() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
type InstanceType<T extends abstract new (...args: any) => any> =
    T extends abstract new (...args: any) => infer R ? R : any;

class WithProtectedCtor {
    protected constructor() {}
}

type Bad = InstanceType<typeof WithProtectedCtor>;
"#,
    );
    let ts2344_errors = diagnostics_for_code(&diagnostics, 2344);
    assert_eq!(
        ts2344_errors.len(),
        1,
        "Should emit one TS2344 for InstanceType<typeof WithProtectedCtor> with a protected constructor.\nGot: {ts2344_errors:#?}\nAll: {diagnostics:#?}"
    );
}
