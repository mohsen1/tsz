//! Regression tests for `TS6133` against the operand of a `typeof` type query.
//!
//! Structural rule (pinned against `typescript@7.0.2`, the conformance pin):
//! a `TYPE_QUERY` is syntactically a type node, but its entity name resolves in
//! the **value** namespace — tsc's `checkTypeQuery` routes it through
//! `checkExpressionOrQualifiedName`, so `typeof x` is a genuine read of `x`.
//! A binding whose only reference is the operand of a `typeof` is therefore
//! *used*, and neither `--noUnusedLocals` nor `--noUnusedParameters` reports it.
//!
//! tsz decides this in the checker's unused-identifier pass
//! (`types/type_checking/unused.rs`). `is_parameter_only_type_referenced`
//! re-scans same-named identifiers and discounts the ones sitting in a type
//! context, so that a parameter shadowed by a same-named *type*
//! (`interface Zed {}` + `function o(Zed: number) { let v: Zed; }`) is still
//! reported — tsc reports that one. The classifier behind it,
//! `node_is_in_type_context`, answered `true` for the operand of a `typeof`
//! because `TYPE_QUERY` answers `is_type_node()`, which made every
//! `typeof`-only reference invisible and produced a false `TS6133`.
//!
//! Corpus witness: `compiler/unusedParameterUsedInTypeOf.ts`, a pure
//! false-positive row (extra `TS6133`, nothing missing) in
//! `scripts/conformance/conformance-detail.json`.
//!
//! Every row below was measured against the pin with
//! `--strict --target es2022 --lib es2022 --noUnusedLocals --noUnusedParameters`.
//! Binder names are varied across rows: the rule is structural, so no row may
//! depend on a particular identifier spelling.

use crate::context::ScriptTarget;
use crate::test_utils::check_source;
use crate::{CheckerOptions, diagnostics::Diagnostic};

fn check_unused(source: &str) -> Vec<Diagnostic> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            no_unused_locals: true,
            no_unused_parameters: true,
            target: ScriptTarget::ES2022,
            ..CheckerOptions::default()
        },
    )
}

fn unused_codes(source: &str) -> Vec<u32> {
    check_unused(source)
        .into_iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

/// The corpus witness, reduced: a parameter whose only reference is a sibling
/// parameter's `typeof`.
///
/// ```text
/// tsc: (no output)
/// ```
#[test]
fn a_parameter_read_by_a_sibling_parameter_type_query_is_used() {
    let codes = unused_codes("function f1(a: number, b: typeof a) { return b; }\nf1(1, 2);\n");

    assert!(
        !codes.contains(&6133),
        "a parameter named by a sibling parameter's `typeof` is read; tsc reports nothing. Got: {codes:?}"
    );
}

/// Same shape, different binder names — the rule must not key on `a`/`b`.
#[test]
fn the_type_query_read_does_not_depend_on_the_binder_spelling() {
    let codes = unused_codes(
        "export function zeta(kappa: string, lambda: typeof kappa) { return lambda; }\n",
    );

    assert!(
        !codes.contains(&6133),
        "renamed binders must behave identically. Got: {codes:?}"
    );
}

/// The query in a *return* type annotation rather than a parameter's.
#[test]
fn a_parameter_read_by_a_return_type_query_is_used() {
    let codes = unused_codes("export function k(a: number): typeof a { return a; }\n");

    assert!(
        !codes.contains(&6133),
        "a return-type `typeof` reads its operand. Got: {codes:?}"
    );
}

/// A query nested inside a surrounding type literal — the walk must reach the
/// `TYPE_QUERY` through the intervening type nodes.
#[test]
fn a_parameter_read_by_a_nested_type_query_is_used() {
    let codes =
        unused_codes("export function p(a: number, b: { deep: { q: typeof a } }) { return b; }\n");

    assert!(
        !codes.contains(&6133),
        "a `typeof` nested inside a type literal still reads its operand. Got: {codes:?}"
    );
}

/// A qualified operand (`typeof ns.p`): the identifier's parent is a
/// `QUALIFIED_NAME`, so the walk reaches `TYPE_QUERY` one hop later.
#[test]
fn a_parameter_read_by_a_qualified_type_query_is_used() {
    let codes =
        unused_codes("export function n(ns: { p: number }, q: typeof ns.p) { return q; }\n");

    assert!(
        !codes.contains(&6133),
        "the root of a qualified `typeof` operand is read. Got: {codes:?}"
    );
}

/// A query written inside the function body rather than in a signature.
#[test]
fn a_parameter_read_by_a_body_local_type_query_is_used() {
    let codes = unused_codes("export function q(a: number) { const t: typeof a = 1; return t; }\n");

    assert!(
        !codes.contains(&6133),
        "a body-local `typeof` annotation reads its operand. Got: {codes:?}"
    );
}

/// The `--noUnusedLocals` half of the same rule: a local whose only reference is
/// a `typeof`.
#[test]
fn a_local_read_only_by_a_type_query_is_used() {
    let codes =
        unused_codes("export function h() { const w = 1; const r: typeof w = 1; return r; }\n");

    assert!(
        !codes.contains(&6133),
        "a local named by a `typeof` is read. Got: {codes:?}"
    );
}

/// Module-scope local read only by a `typeof` in an exported type alias.
#[test]
fn a_module_local_read_by_an_exported_type_alias_query_is_used() {
    let codes = unused_codes("const uu = 1;\nexport type UU = typeof uu;\n");

    assert!(
        !codes.contains(&6133),
        "a module-scope local named by an exported alias's `typeof` is read. Got: {codes:?}"
    );
}

/// Tripwire for the one spelling this fix does **not** reach: a `typeof` written
/// inside the **type arguments** of a type reference. Measured through the built
/// CLI (`--strict --target es2022 --noUnusedLocals --noUnusedParameters`), the
/// parameter is still reported as unread for `Wrap<typeof a>`, `Array<typeof a>`,
/// `ReadonlyArray<typeof a>`, `Map<string, typeof a>` and `Wrap<(typeof a)>`,
/// while every non-type-argument spelling above is now correct. tsc reports
/// nothing for any of them.
///
/// The `--noUnusedLocals` half of the same spelling is already correct
/// (`const w = 1; export const y: Wrap<typeof w> = { v: 1 };` is clean), which
/// places the remaining defect on the parameter re-scan rather than on reference
/// tracking. Left as a failing assertion under `#[ignore]` so it flips loudly
/// when the type-argument cell is closed rather than sitting as a silent absence.
#[test]
#[ignore = "type-argument-nested typeof is a separate open cell; see the module docs"]
fn a_parameter_read_by_a_type_argument_type_query_is_used() {
    let codes = unused_codes(
        "type Wrap<T> = { v: T };\nexport function m2(a: number, b: Wrap<typeof a>) { return b; }\n",
    );

    assert!(
        !codes.contains(&6133),
        "a `typeof` in type-argument position reads its operand. Got: {codes:?}"
    );
}

/// Negative control: a genuinely unread parameter still reports `TS6133`.
///
/// ```text
/// tsc: error TS6133: 'unusedParam' is declared but its value is never read.
/// ```
#[test]
fn a_parameter_with_no_reference_at_all_still_reports_ts6133() {
    let codes = unused_codes("export function bad(unusedParam: number) { return 1; }\n");

    assert!(
        codes.contains(&6133),
        "an unread parameter must still report TS6133. Got: {codes:?}"
    );
}

/// Negative control, and the reason `is_parameter_only_type_referenced` exists:
/// a parameter shadowed by a same-named *type*. The `Zed` in `v: Zed` resolves
/// to the interface, not to the parameter, so the parameter is unread.
///
/// ```text
/// tsc: error TS6133: 'Zed' is declared but its value is never read.
/// ```
#[test]
fn a_parameter_shadowed_by_a_same_named_type_reference_still_reports_ts6133() {
    let codes = unused_codes(
        "interface Zed { z: number }\nexport function o(Zed: number) { const v: Zed = { z: 1 }; return v; }\n",
    );

    assert!(
        codes.contains(&6133),
        "a same-named *type* reference is not a read of the parameter. Got: {codes:?}"
    );
}

/// Negative control on the other side of the new branch: a parameter named only
/// by a same-named type reference nested *inside* a `typeof`-bearing signature
/// stays unread. The `typeof` here names a different binding, so the type-only
/// reference must not be laundered into a read.
#[test]
fn a_type_only_reference_beside_an_unrelated_type_query_still_reports_ts6133() {
    let codes = unused_codes(
        "interface Wye { w: number }\nexport function s(Wye: number, other: string, t: typeof other) { const v: Wye = { w: 1 }; return [v, t]; }\n",
    );

    assert!(
        codes.contains(&6133),
        "an unrelated `typeof` in the same signature must not mark `Wye` as read. Got: {codes:?}"
    );
}
