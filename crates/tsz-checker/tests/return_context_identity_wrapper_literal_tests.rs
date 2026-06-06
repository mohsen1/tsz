//! Return-context inference for identity-wrapper factories must preserve literal
//! discriminants and must keep reporting genuine mismatches.
//!
//! Structural rule: when a generic identity wrapper `freeze<T>(obj: T):
//! Readonly<T>` is called with a fresh object literal whose discriminant has a
//! string-literal initializer, and the call sits in a contextual-return
//! position (`create(): Readonly<XNode>`), `tsc` infers `T` with the literal
//! preserved (`kind: 'XNode'`) because the contextual return type pins the
//! unconstrained `T`. The return-context substitution that recovers this
//! pinning must decompose the contextual wrapper structurally (via the
//! application/display-alias provenance) rather than by inspecting rendered type
//! text, and must not erase real discriminant mismatches.
//!
//! Witness family: the `operation-node` factories behind the Kysely project row
//! (issue #10663).

use tsz_checker::context::{CheckerOptions, ScriptTarget};
use tsz_checker::test_utils::{check_multi_file_with_libs, load_default_lib_files};
use tsz_common::diagnostics::Diagnostic;

fn ts_options() -> CheckerOptions {
    CheckerOptions {
        target: ScriptTarget::ES2015,
        ..CheckerOptions::default()
    }
}

fn check(files: &[(&str, &str)], entry: &str) -> Vec<Diagnostic> {
    let libs = load_default_lib_files();
    check_multi_file_with_libs(files, entry, ts_options(), &libs)
}

fn codes(diags: &[Diagnostic]) -> Vec<u32> {
    diags.iter().map(|d| d.code).collect()
}

fn assert_no_widening_false_positive(files: &[(&str, &str)], context: &str) {
    let diags = check(files, files[0].0);
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "{context}: expected no TS2322 literal-widening false positive, got: {:#?}",
        ts2322
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

/// Inner identity wrapper whose contextual return type is supplied directly by
/// the surrounding annotation. The literal `kind: "WithNode"` must survive.
#[test]
fn inner_wrapper_with_direct_contextual_return_preserves_literal() {
    assert_no_widening_false_positive(
        &[(
            "direct.ts",
            r#"
function freeze<T>(obj: T): Readonly<T> { return obj; }
interface WithNode { readonly kind: "WithNode"; readonly count: number; }
const create: (count: number) => Readonly<WithNode> =
    (count) => freeze({ kind: "WithNode", count });
"#,
        )],
        "direct contextual return",
    );
}

/// Object literal contextually typed by the factory interface directly (no outer
/// generic call). The unannotated method body's nested wrapper must be pinned.
#[test]
fn factory_object_literal_preserves_literal_through_method_wrapper() {
    assert_no_widening_false_positive(
        &[(
            "factory.ts",
            r#"
function freeze<T>(obj: T): Readonly<T> { return obj; }
interface WithNode { readonly kind: "WithNode"; readonly count: number; }
interface WithNodeFactory { create(count: number): Readonly<WithNode>; }
const WithNode: WithNodeFactory = {
    create(count: number) {
        return freeze({ kind: "WithNode", count });
    },
};
"#,
        )],
        "factory object literal",
    );
}

/// Context-sensitive method (unannotated parameter) routed through the deferred
/// contextual pass must also preserve the literal discriminant.
#[test]
fn context_sensitive_factory_method_preserves_literal() {
    assert_no_widening_false_positive(
        &[(
            "ctxsens.ts",
            r#"
function freeze<T>(obj: T): Readonly<T> { return obj; }
interface WithNode { readonly kind: "WithNode"; readonly count: number; }
interface WithNodeFactory { create(count: number): Readonly<WithNode>; }
const WithNode: WithNodeFactory = freeze({
    create(count) {
        return freeze({ kind: "WithNode", count });
    },
});
"#,
        )],
        "context-sensitive method",
    );
}

/// Negative: a factory whose `create` returns the *wrong* literal discriminant
/// must still report `TS2322`. The literal pinning must not erase real
/// mismatches by widening both sides.
#[test]
fn identity_wrapper_factory_still_reports_wrong_literal_kind() {
    let wrong = r#"
function freeze<T>(obj: T): Readonly<T> { return obj; }
interface SelectNode { readonly kind: "SelectNode"; readonly value: number; }
interface SelectNodeFactory { create(value: number): Readonly<SelectNode>; }
const SelectNode: SelectNodeFactory = {
    create(value: number): Readonly<SelectNode> {
        return freeze({ kind: "NotSelectNode", value });
    },
};
"#;
    let files = vec![("select-node.ts", wrong)];
    let diags = check(&files, "select-node.ts");
    assert!(
        codes(&diags).contains(&2322),
        "expected TS2322 for the wrong literal discriminant, got: {:#?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}
