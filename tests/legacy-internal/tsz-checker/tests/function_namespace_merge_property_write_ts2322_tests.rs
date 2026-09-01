//! A function merged with a namespace (`function F() {} namespace F { export
//! var p = 1; }`) already declares `p` with a concrete type through the
//! merge. A later dot-write `F.p = value` is therefore an assignment to an
//! EXISTING declared member, not a fresh expando-property declaration — tsc
//! checks it against `p`'s declared type and reports `TS2322` on a mismatch,
//! the same as it would for a plain namespace-only merge or an ordinary
//! object-literal property.
//!
//! `get_type_of_assignment_target` (`types/computation/helpers.rs`) treated
//! every property write on a non-class function symbol as an expando
//! declaration outside checked JS, returning `any` for the assignment target
//! type unconditionally and silencing the check — regardless of whether the
//! property already had a concrete type via a namespace merge. The fix scopes
//! the expando `any` fallback to properties NOT already present in the
//! merged symbol's own `exports` table (corpus witnesses:
//! `conformance/salsa/typeFromPropertyAssignment31/32/33.ts`).

use crate::diagnostics::Diagnostic;
use crate::test_utils::check_source_diagnostics;

fn ts2322_count(diags: &[Diagnostic]) -> usize {
    diags.iter().filter(|d| d.code == 2322).count()
}

#[test]
fn single_file_merge_incompatible_write_reports_ts2322() {
    let diags = check_source_diagnostics(
        r#"
function ExpandoMerge(n: number) {
    return n;
}
namespace ExpandoMerge {
    export var p8 = 6;
}
ExpandoMerge.p8 = false;
"#,
    );
    assert_eq!(
        ts2322_count(&diags),
        1,
        "write to a namespace-merged property with an incompatible type must report TS2322; got: {diags:?}"
    );
}

#[test]
fn single_file_merge_renamed_binder_incompatible_write_reports_ts2322() {
    // Same shape, different names throughout — pins the check to the
    // structural merge, not to any particular identifier.
    let diags = check_source_diagnostics(
        r#"
function Widget(size: number) {
    return size;
}
namespace Widget {
    export var color = "red";
}
Widget.color = 42;
"#,
    );
    assert_eq!(
        ts2322_count(&diags),
        1,
        "renamed-binder merge write must still report TS2322; got: {diags:?}"
    );
}

#[test]
fn single_file_merge_compatible_write_is_clean() {
    let diags = check_source_diagnostics(
        r#"
function ExpandoMerge(n: number) {
    return n;
}
namespace ExpandoMerge {
    export var p8 = 6;
}
ExpandoMerge.p8 = 7;
"#,
    );
    assert_eq!(
        ts2322_count(&diags),
        0,
        "a compatible write to a merged property must stay clean; got: {diags:?}"
    );
}

#[test]
fn single_file_merge_fresh_expando_property_is_clean() {
    // `freshProp` is not exported by the namespace, so it is still a
    // legitimate fresh expando declaration, unaffected by the fix.
    let diags = check_source_diagnostics(
        r#"
function ExpandoMerge(n: number) {
    return n;
}
namespace ExpandoMerge {
    export var p8 = 6;
}
ExpandoMerge.freshProp = "hello";
"#,
    );
    assert_eq!(
        ts2322_count(&diags),
        0,
        "a fresh (non-merged) expando property write must stay clean; got: {diags:?}"
    );
}

#[test]
fn plain_function_without_namespace_merge_keeps_expando_leniency() {
    // No namespace merge at all: every property write is still a fresh
    // expando declaration, exactly as before this fix.
    let diags = check_source_diagnostics(
        r#"
function G(n: number) {
    return n;
}
G.foo = "bar";
"#,
    );
    assert_eq!(
        ts2322_count(&diags),
        0,
        "a plain (non-merged) function expando write must stay clean; got: {diags:?}"
    );
}

// Cross-file coverage (function-declared-first and namespace-declared-first
// orderings) is NOT exercised here: `check_multi_file_with_global_index`
// does not populate `get_cross_file_symbol`'s merged exports table the way
// the production driver does for a function+namespace merge (the same
// fidelity gap documented at the top of
// `js_cross_file_expando_declaration_tests.rs`), so a harness test would pin
// harness behavior, not compiler behavior. The real driver is verified
// directly instead: `conformance/salsa/typeFromPropertyAssignment32.ts`
// (function file first) and `typeFromPropertyAssignment33.ts` (namespace
// file first) are the identical two-file repro in both orderings, confirmed
// passing against the pinned oracle via `tsz-conformance --filter
// typeFromPropertyAssignment3`.
