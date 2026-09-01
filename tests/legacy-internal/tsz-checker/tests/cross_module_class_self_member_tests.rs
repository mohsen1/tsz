use crate::diagnostics::diagnostic_codes;
use crate::test_utils::check_multi_file;

fn strict_options() -> tsz_common::CheckerOptions {
    tsz_common::CheckerOptions {
        module: tsz_common::common::ModuleKind::CommonJS,
        strict: true,
        ..Default::default()
    }
}

#[test]
fn cross_module_self_return_override_uses_base_instance_type() {
    let diagnostics = check_multi_file(
        &[
            (
                "base.ts",
                r#"
export abstract class Base {
    abstract self(): Base;
}
"#,
            ),
            (
                "derived.ts",
                r#"
import { Base } from "./base";

export class Derived extends Base {
    self(): Derived {
        return this;
    }
}
"#,
            ),
        ],
        "derived.ts",
        strict_options(),
    );

    assert!(
        !diagnostics
            .iter()
            .any(|diag| diag.code == diagnostic_codes::PROPERTY_IN_TYPE_IS_NOT_ASSIGNABLE_TO_THE_SAME_PROPERTY_IN_BASE_TYPE),
        "expected no TS2416 for covariant self return across modules, got {diagnostics:#?}"
    );
}

#[test]
fn cross_module_self_field_override_uses_base_instance_type() {
    let diagnostics = check_multi_file(
        &[
            (
                "base.ts",
                r#"
export class Base {
    next!: Base;
}
"#,
            ),
            (
                "derived.ts",
                r#"
import { Base } from "./base";

export class Derived extends Base {
    next!: Derived;
}
"#,
            ),
        ],
        "derived.ts",
        strict_options(),
    );

    assert!(
        !diagnostics
            .iter()
            .any(|diag| diag.code == diagnostic_codes::PROPERTY_IN_TYPE_IS_NOT_ASSIGNABLE_TO_THE_SAME_PROPERTY_IN_BASE_TYPE),
        "expected no TS2416 for covariant self field across modules, got {diagnostics:#?}"
    );
}

#[test]
fn cross_module_generic_self_field_override_uses_base_instance_type() {
    let diagnostics = check_multi_file(
        &[
            (
                "base.ts",
                r#"
export class Box<T> {
    next!: Box<T>;
}
"#,
            ),
            (
                "derived.ts",
                r#"
import { Box } from "./base";

export class FancyBox<Item> extends Box<Item> {
    next!: FancyBox<Item>;
}
"#,
            ),
        ],
        "derived.ts",
        strict_options(),
    );

    assert!(
        !diagnostics
            .iter()
            .any(|diag| diag.code == diagnostic_codes::PROPERTY_IN_TYPE_IS_NOT_ASSIGNABLE_TO_THE_SAME_PROPERTY_IN_BASE_TYPE),
        "expected no TS2416 for generic covariant self field across modules, got {diagnostics:#?}"
    );
}

#[test]
fn cross_module_genuine_field_override_mismatch_still_reports_ts2416() {
    let diagnostics = check_multi_file(
        &[
            (
                "base.ts",
                r#"
export class Base {
    value!: string;
}
"#,
            ),
            (
                "derived.ts",
                r#"
import { Base } from "./base";

export class Derived extends Base {
    value!: number;
}
"#,
            ),
        ],
        "derived.ts",
        strict_options(),
    );

    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == diagnostic_codes::PROPERTY_IN_TYPE_IS_NOT_ASSIGNABLE_TO_THE_SAME_PROPERTY_IN_BASE_TYPE),
        "expected TS2416 for genuine cross-module field mismatch, got {diagnostics:#?}"
    );
}

#[test]
fn cross_module_typeof_constructor_field_override_still_reports_ts2416() {
    let diagnostics = check_multi_file(
        &[
            (
                "base.ts",
                r#"
export class Base {
    ctor!: typeof Base;
}
"#,
            ),
            (
                "derived.ts",
                r#"
import { Base } from "./base";

export class Derived extends Base {
    ctor!: Derived;
}
"#,
            ),
        ],
        "derived.ts",
        strict_options(),
    );

    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == diagnostic_codes::PROPERTY_IN_TYPE_IS_NOT_ASSIGNABLE_TO_THE_SAME_PROPERTY_IN_BASE_TYPE),
        "expected TS2416 when overriding constructor-typed field with instance type, got {diagnostics:#?}"
    );
}
