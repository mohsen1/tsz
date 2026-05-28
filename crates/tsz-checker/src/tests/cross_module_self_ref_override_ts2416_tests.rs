//! Regression coverage for issue #10634: a cross-module derived class that
//! overrides a base member with a self-referential (covariant) return/field
//! type must not produce a false `TS2416`, while a genuinely incompatible
//! cross-module override must still error.

use crate::context::CheckerOptions;
use crate::diagnostics::{Diagnostic, diagnostic_codes};
use crate::test_utils::check_multi_file;
use tsz_common::common::ModuleKind;

fn check(files: &[(&str, &str)], entry: &str) -> Vec<Diagnostic> {
    check_multi_file(
        files,
        entry,
        CheckerOptions {
            module: ModuleKind::CommonJS,
            strict: true,
            ..CheckerOptions::default()
        },
    )
}

fn ts2416(diags: &[Diagnostic]) -> Vec<String> {
    diags
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::PROPERTY_IN_TYPE_IS_NOT_ASSIGNABLE_TO_THE_SAME_PROPERTY_IN_BASE_TYPE
        })
        .map(|d| d.message_text.to_string())
        .collect()
}

#[test]
fn cross_module_self_referential_method_no_false_ts2416() {
    let diags = check(
        &[
            (
                "./base.ts",
                r#"export abstract class Base { abstract self(): Base; }"#,
            ),
            (
                "./derived.ts",
                r#"import { Base } from "./base";
export class Derived extends Base { self(): Derived { return this; } }"#,
            ),
        ],
        "./derived.ts",
    );
    let errs = ts2416(&diags);
    assert!(errs.is_empty(), "expected no TS2416, got: {errs:?}");
}

#[test]
fn cross_module_self_referential_field_no_false_ts2416() {
    let diags = check(
        &[
            ("./base.ts", r#"export class Base { next!: Base; }"#),
            (
                "./derived.ts",
                r#"import { Base } from "./base";
export class Derived extends Base { next!: Derived; }"#,
            ),
        ],
        "./derived.ts",
    );
    let errs = ts2416(&diags);
    assert!(errs.is_empty(), "expected no TS2416, got: {errs:?}");
}

#[test]
fn cross_module_incompatible_override_still_errors_ts2416() {
    let diags = check(
        &[
            (
                "./base.ts",
                r#"export class Base { val(): string { return ""; } }"#,
            ),
            (
                "./derived.ts",
                r#"import { Base } from "./base";
export class Derived extends Base { val(): number { return 0; } }"#,
            ),
        ],
        "./derived.ts",
    );
    let errs = ts2416(&diags);
    assert!(
        !errs.is_empty(),
        "expected TS2416 for number-over-string override"
    );
}

#[test]
fn cross_module_self_referential_method_renamed_type_param_no_false_ts2416() {
    // The fix must follow the structural self-reference, not a particular
    // type-parameter spelling. A generic base/derived with differently named
    // type parameters that both reference the class itself must stay clean.
    let diags = check(
        &[
            (
                "./base.ts",
                r#"export abstract class Base<A> { abstract self(): Base<A>; }"#,
            ),
            (
                "./derived.ts",
                r#"import { Base } from "./base";
export class Derived<Q> extends Base<Q> { self(): Derived<Q> { return this; } }"#,
            ),
        ],
        "./derived.ts",
    );
    let errs = ts2416(&diags);
    assert!(errs.is_empty(), "expected no TS2416, got: {errs:?}");
}

#[test]
fn cross_module_identity_override_no_false_ts2416() {
    // Overriding a self-referential member with the *same* base type (no
    // narrowing) must also stay clean cross-module.
    let diags = check(
        &[
            ("./base.ts", r#"export class Base { next!: Base; }"#),
            (
                "./derived.ts",
                r#"import { Base } from "./base";
export class Derived extends Base { next!: Base; }"#,
            ),
        ],
        "./derived.ts",
    );
    let errs = ts2416(&diags);
    assert!(errs.is_empty(), "expected no TS2416, got: {errs:?}");
}

#[test]
fn cross_module_self_referential_getter_no_false_ts2416() {
    // Accessor members carry the same self-reference shape as fields/methods.
    let diags = check(
        &[
            (
                "./base.ts",
                r#"export class Base { get node(): Base { return this; } }"#,
            ),
            (
                "./derived.ts",
                r#"import { Base } from "./base";
export class Derived extends Base { get node(): Derived { return this; } }"#,
            ),
        ],
        "./derived.ts",
    );
    let errs = ts2416(&diags);
    assert!(errs.is_empty(), "expected no TS2416, got: {errs:?}");
}

#[test]
fn cross_module_incompatible_self_referential_field_still_errors_ts2416() {
    // Negative guard for the self-referential shape: overriding a self field
    // (`next: Base`) with a primitive that is not assignable to the base
    // instance must still error. Proves the fix resolves the self-reference to
    // the base instance for the relation rather than blanket-suppressing
    // self-referential overrides.
    let diags = check(
        &[
            ("./base.ts", r#"export class Base { next!: Base; }"#),
            (
                "./derived.ts",
                r#"import { Base } from "./base";
export class Derived extends Base { next!: number; }"#,
            ),
        ],
        "./derived.ts",
    );
    let errs = ts2416(&diags);
    assert!(
        !errs.is_empty(),
        "expected TS2416 for number override of a self-referential class field"
    );
}
