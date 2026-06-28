//! Witness matrix for the `delete o.a` control-flow narrowing reset.
//!
//! Structural rule:
//!
//! > When a `delete` operates on a property-access reference (`delete o.a`),
//! > `tsc`'s binder records a flow mutation for that reference
//! > (`bindDeleteExpressionFlow` -> `bindAssignmentTargetFlow`), so a later read
//! > of `o.a` widens back to its declared read type (re-including `undefined`
//! > for an optional property). `tsz` mirrors this: the binder
//! > (`PREFIX_UNARY_EXPRESSION` delete handler) creates an `ASSIGNMENT` flow node
//! > for the deleted property reference, and the checker's `get_assigned_type`
//! > returns `undefined` for the delete operand (matching tsc's
//! > `getAssignedType` -> `undefinedType`), which the killing-definition path
//! > reduces against the declared union.
//!
//! Scope matches tsc exactly: only a *property-access* operand is a flow
//! mutation. `delete o[k]` (element access) and an unrelated `delete o.b` leave
//! the narrowing of `o.a` intact.
//!
//! Every case uses distinct binder / parameter / property names so the behavior
//! follows the structural shape, not any identifier spelling (CLAUDE.md
//! anti-hardcoding gate).

use tsz_checker::test_utils::check_source_codes;

const TS2322_NOT_ASSIGNABLE: u32 = 2322;

fn assert_reports_2322(source: &str) {
    let diags = check_source_codes(source);
    assert!(
        diags.contains(&TS2322_NOT_ASSIGNABLE),
        "delete of the narrowed property must reset narrowing and report TS2322; got: {diags:?}",
    );
}

fn assert_no_2322(source: &str) {
    let diags = check_source_codes(source);
    assert!(
        !diags.contains(&TS2322_NOT_ASSIGNABLE),
        "narrowing must be preserved (unexpected TS2322); got: {diags:?}",
    );
}

// =========================================================================
// `delete o.a` RESETS the narrowing of `o.a`.
// =========================================================================

/// The minimal repro from the issue: after `delete o.a`, the read `o.a` widens
/// back to `number | undefined`, so `const after: number = o.a` is TS2322.
/// `tsc` reports this; `tsz` previously silently accepted it (FN/soundness gap).
#[test]
fn delete_property_resets_optional_narrowing() {
    assert_reports_2322(
        r#"
function f(o: { a?: number }) {
    if (o.a !== undefined) {
        const before: number = o.a;
        delete o.a;
        const after: number = o.a;
    }
}
"#,
    );
}

/// Same structural shape with different binder/property names — the fix must be
/// driven by the structural delete-of-property pattern, not any identifier.
#[test]
fn delete_property_resets_narrowing_distinct_names() {
    assert_reports_2322(
        r#"
function processRecord(record: { payload?: string }) {
    if (record.payload !== undefined) {
        delete record.payload;
        const reread: string = record.payload;
    }
}
"#,
    );
}

/// A nested property `delete o.inner.value` resets `o.inner.value` only.
#[test]
fn delete_nested_property_resets_that_leaf() {
    assert_reports_2322(
        r#"
function g(o: { inner: { value?: number } }) {
    if (o.inner.value !== undefined) {
        delete o.inner.value;
        const after: number = o.inner.value;
    }
}
"#,
    );
}

// =========================================================================
// Narrowing is PRESERVED when there is no relevant delete.
// =========================================================================

/// Control: without the `delete`, the narrowing persists and the read is fine
/// in both compilers.
#[test]
fn no_delete_keeps_narrowing() {
    assert_no_2322(
        r#"
function f(o: { a?: number }) {
    if (o.a !== undefined) {
        const after: number = o.a;
    }
}
"#,
    );
}

/// Control: deleting an UNRELATED property (`o.other`) must keep `o.value`
/// narrowed.
#[test]
fn delete_unrelated_property_keeps_narrowing() {
    assert_no_2322(
        r#"
function f(o: { value?: number; other?: number }) {
    if (o.value !== undefined) {
        delete o.other;
        const after: number = o.value;
    }
}
"#,
    );
}

/// A nested `delete o.inner.other` must keep a sibling leaf `o.inner.value`
/// narrowed.
#[test]
fn delete_nested_sibling_keeps_narrowing() {
    assert_no_2322(
        r#"
function g(o: { inner: { value?: number; other?: number } }) {
    if (o.inner.value !== undefined) {
        delete o.inner.other;
        const after: number = o.inner.value;
    }
}
"#,
    );
}

/// Parity with tsc: an *element-access* delete (`delete o["a"]`) is NOT a flow
/// mutation in tsc's binder (`bindDeleteExpressionFlow` only handles
/// `PropertyAccessExpression`), so `o.a` stays narrowed and the read is clean.
#[test]
fn delete_element_access_keeps_narrowing() {
    assert_no_2322(
        r#"
function f(o: { a?: number }) {
    if (o.a !== undefined) {
        delete o["a"];
        const after: number = o.a;
    }
}
"#,
    );
}
