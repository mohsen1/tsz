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

/// The operation-node witness: an OUTER generic identity-wrapper call whose
/// object-literal argument has a non-context-sensitive method (all params
/// annotated) with a nested generic call in its body. The outer call's
/// return-context substitution pins the wrapper's type parameter to the
/// contextual factory type, and that concrete contextual must survive into the
/// literal's method body so the inner wrapper sees `Readonly<XNode>` and keeps
/// the literal discriminant.
#[test]
fn outer_generic_call_with_annotated_method_preserves_literal() {
    assert_no_widening_false_positive(
        &[(
            "outer.ts",
            r#"
function deepFreeze<TObj>(entity: TObj): Readonly<TObj> { return entity; }
interface LimitNode { readonly kind: "LimitNode"; readonly max: number; }
interface LimitNodeFactory { create(max: number): Readonly<LimitNode>; }
const LimitNode: LimitNodeFactory = deepFreeze({
    create(max: number) {
        return deepFreeze({ kind: "LimitNode", max });
    },
});
"#,
        )],
        "outer generic call with annotated method",
    );
}

/// Same outer-call shape with an explicit type argument on the inner wrapper:
/// the explicit instantiation already provides the contextual pin, and the
/// outer refresh must not disturb it.
#[test]
fn outer_generic_call_with_explicit_inner_type_argument_preserves_literal() {
    assert_no_widening_false_positive(
        &[(
            "explicit.ts",
            r#"
function wrapValue<Z>(value: Z): Readonly<Z> { return value; }
interface AlterNode { readonly kind: "AlterNode"; readonly size: number; }
interface AlterNodeFactory { create(size: number): Readonly<AlterNode>; }
const AlterNode: AlterNodeFactory = wrapValue({
    create(size: number) {
        return wrapValue<AlterNode>({ kind: "AlterNode", size });
    },
});
"#,
        )],
        "outer generic call with explicit inner type argument",
    );
}

/// Same outer-call shape where the method return type is annotated: the
/// annotation supplies the inner contextual return directly and the outer
/// refresh must keep it intact.
#[test]
fn outer_generic_call_with_annotated_method_return_preserves_literal() {
    assert_no_widening_false_positive(
        &[(
            "annotated-return.ts",
            r#"
function sealItem<Q>(input: Q): Readonly<Q> { return input; }
interface UniqueNode { readonly kind: "UniqueNode"; readonly width: number; }
interface UniqueNodeFactory { build(width: number): Readonly<UniqueNode>; }
const UniqueNode: UniqueNodeFactory = sealItem({
    build(width: number): Readonly<UniqueNode> {
        return sealItem({ kind: "UniqueNode", width });
    },
});
"#,
        )],
        "outer generic call with annotated method return",
    );
}

/// Negative for the outer-call shape: a wrong literal discriminant in the
/// nested wrapper must still produce `TS2322`. The concrete contextual pin
/// must not silence real mismatches.
#[test]
fn outer_generic_call_still_reports_wrong_literal_kind() {
    let wrong = r#"
function grabAll<R>(obj: R): Readonly<R> { return obj; }
interface MergeNode { readonly kind: "MergeNode"; readonly span: number; }
interface MergeNodeFactory { create(span: number): Readonly<MergeNode>; }
const MergeNode: MergeNodeFactory = grabAll({
    create(span: number) {
        return grabAll({ kind: "Wrong", span });
    },
});
"#;
    let files = vec![("merge-node.ts", wrong)];
    let diags = check(&files, "merge-node.ts");
    assert!(
        codes(&diags).contains(&2322),
        "expected TS2322 for the wrong literal discriminant through the outer call, got: {:#?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
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
