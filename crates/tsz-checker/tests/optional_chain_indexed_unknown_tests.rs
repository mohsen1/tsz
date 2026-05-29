//! Parity regression matrix for the `unknown`-object element-access decision
//! (`TS2571` / `TS18046`) on plain and optional-chained indexed access.
//!
//! Tracks issue #11360 ("false positive TS2571 on optional chaining with
//! indexed access"). `tsc` forbids indexing a value of type `unknown`
//! regardless of whether the access is optional (`x[k]` and `x?.[k]` behave
//! identically), choosing the named form `TS18046` when the base expression
//! has a printable name and the object form `TS2571` otherwise. These tests
//! lock that behaviour — and, crucially, prove that the diagnostic does *not*
//! fire once the base has been narrowed away from `unknown` or is a perfectly
//! well-typed indexable value reached through an optional chain.
//!
//! Per the anti-hardcoding gate, every binder-introduced name (type
//! parameters, parameters) is varied across cases so a fix keyed on a specific
//! spelling would fail here.

use tsz_checker::test_utils::{check_source_strict, diagnostic_count, diagnostics_with_code};

const TS2571_OBJECT_IS_UNKNOWN: u32 = 2571;
const TS18046_IS_OF_TYPE_UNKNOWN: u32 = 18046;

fn count(source: &str, code: u32) -> usize {
    diagnostic_count(&check_source_strict(source), code)
}

/// The two unknown-object diagnostics are mutually exclusive per access site;
/// this asserts the *total* unknown-object diagnostics on a snippet.
fn unknown_object_diags(source: &str) -> usize {
    let diags = check_source_strict(source);
    diagnostic_count(&diags, TS2571_OBJECT_IS_UNKNOWN)
        + diagnostic_count(&diags, TS18046_IS_OF_TYPE_UNKNOWN)
}

// ---------------------------------------------------------------------------
// Positive cases: indexing `unknown` is an error, optional or not.
// ---------------------------------------------------------------------------

/// Complex (un-named) base of type `unknown` indexed -> object form `TS2571`.
#[test]
fn complex_base_unknown_indexed_reports_ts2571() {
    let source = r#"
function read(value: unknown) {
    return (value as unknown)["k"];
}
"#;
    assert_eq!(
        count(source, TS2571_OBJECT_IS_UNKNOWN),
        1,
        "indexing a complex `unknown` base must report TS2571 (object form)"
    );
    assert_eq!(count(source, TS18046_IS_OF_TYPE_UNKNOWN), 0);
}

/// A named property-access base of type `unknown` -> named form `TS18046`,
/// identical for plain and optional element access.
#[test]
fn named_base_unknown_indexed_reports_ts18046_plain_and_optional() {
    let plain = r#"
function read(holder: { payload: unknown }) {
    return holder.payload["k"];
}
"#;
    let optional = r#"
function read(holder: { payload: unknown }) {
    return holder.payload?.["k"];
}
"#;
    for source in [plain, optional] {
        let diags = check_source_strict(source);
        let named = diagnostics_with_code(&diags, TS18046_IS_OF_TYPE_UNKNOWN);
        assert_eq!(
            named.len(),
            1,
            "named `unknown` base must report exactly one TS18046, got: {named:?}"
        );
        assert!(
            named[0].message_text.contains("'holder.payload'"),
            "TS18046 must name the base expression, got: {:?}",
            named[0].message_text
        );
        assert_eq!(diagnostic_count(&diags, TS2571_OBJECT_IS_UNKNOWN), 0);
    }
}

/// An element-access base whose element type is `unknown` is itself un-named,
/// so a further index reports the object form `TS2571` (plain and optional).
#[test]
fn element_base_unknown_indexed_reports_ts2571_plain_and_optional() {
    let plain = r#"
function read(items: unknown[]) {
    return items[0]["k"];
}
"#;
    let optional = r#"
function read(items: unknown[]) {
    return items[0]?.["k"];
}
"#;
    for source in [plain, optional] {
        assert_eq!(
            count(source, TS2571_OBJECT_IS_UNKNOWN),
            1,
            "indexing an un-named `unknown` element base must report TS2571"
        );
        assert_eq!(count(source, TS18046_IS_OF_TYPE_UNKNOWN), 0);
    }
}

/// The optional-chain short-circuit does not rescue `unknown`: a possibly
/// `undefined` *and* `unknown` union still collapses to `unknown` after `?.`.
#[test]
fn optional_chain_does_not_rescue_unknown_union() {
    let source = r#"
function read(maybe: unknown | undefined) {
    return (maybe as unknown)?.["k"];
}
"#;
    assert_eq!(
        unknown_object_diags(source),
        1,
        "`?.` removes nullishness but not `unknown`; one unknown-object diagnostic expected"
    );
}

// ---------------------------------------------------------------------------
// Property access (dot) shares the same unknown-object decision gate as element
// access, so the named/object diagnostic choice must match.
// ---------------------------------------------------------------------------

/// Named property-access base of type `unknown`, accessed by name (plain and
/// optional) -> named form `TS18046`.
#[test]
fn named_base_unknown_property_access_reports_ts18046() {
    let plain = r#"
function read(holder: { payload: unknown }) {
    return holder.payload.field;
}
"#;
    let optional = r#"
function read(holder: { payload: unknown }) {
    return holder.payload?.field;
}
"#;
    for source in [plain, optional] {
        assert_eq!(
            count(source, TS18046_IS_OF_TYPE_UNKNOWN),
            1,
            "named `unknown` base property access must report TS18046"
        );
        assert_eq!(count(source, TS2571_OBJECT_IS_UNKNOWN), 0);
    }
}

/// Complex (un-named) base of type `unknown`, property-accessed -> object form
/// `TS2571`, mirroring the element-access object-form case.
#[test]
fn complex_base_unknown_property_access_reports_ts2571() {
    let source = r#"
function read(value: unknown) {
    return (value as unknown).field;
}
"#;
    assert_eq!(count(source, TS2571_OBJECT_IS_UNKNOWN), 1);
    assert_eq!(count(source, TS18046_IS_OF_TYPE_UNKNOWN), 0);
}

// ---------------------------------------------------------------------------
// Negative cases: the diagnostic must NOT fire for well-typed indexable bases
// reached through an optional chain, nor after narrowing away from `unknown`.
// These are the cases a "false positive TS2571" regression would break.
// ---------------------------------------------------------------------------

/// Optional element access on an index-signature property: no unknown-object
/// diagnostic. (Type-parameter / property names deliberately non-idiomatic.)
#[test]
fn optional_index_signature_access_is_accepted() {
    let source = r#"
function read(container: { bag?: { [slot: string]: number } }) {
    return container.bag?.["slot"];
}
"#;
    assert_eq!(
        unknown_object_diags(source),
        0,
        "optional access into an index signature is well-typed, not `unknown`"
    );
}

/// Optional element access directly on a possibly-`undefined` index signature.
#[test]
fn optional_access_on_possibly_undefined_index_signature_is_accepted() {
    let source = r#"
function read(lookup?: { [entry: string]: number }) {
    return lookup?.["entry"];
}
"#;
    assert_eq!(unknown_object_diags(source), 0);
}

/// Generic `T | undefined` indexed by `keyof T` through an optional chain:
/// no unknown-object diagnostic. Renamed type parameter (`Shape`/`Field`) to
/// defeat any name-keyed fix.
#[test]
fn generic_keyof_optional_access_is_accepted() {
    let source = r#"
function pick<Shape, Field extends keyof Shape>(
    source: Shape | undefined,
    field: Field,
) {
    return source?.[field];
}
"#;
    assert_eq!(unknown_object_diags(source), 0);
}

/// Nested optional chain of well-typed members ending in indexed access.
#[test]
fn nested_optional_chain_indexed_access_is_accepted() {
    let source = r#"
function read(root: { branch?: { leaves?: number[] } }) {
    return root.branch?.leaves?.[0];
}
"#;
    assert_eq!(unknown_object_diags(source), 0);
}

/// `unknown` narrowed to an indexable object via `typeof` + `in` before access
/// must not report the unknown-object diagnostic (`tsc` accepts the index).
#[test]
fn in_narrowed_unknown_indexed_access_is_accepted() {
    let plain = r#"
function read(candidate: unknown) {
    if (candidate && typeof candidate === "object" && "field" in candidate) {
        return candidate["field"];
    }
}
"#;
    let optional = r#"
function read(candidate: unknown) {
    if (candidate && typeof candidate === "object" && "field" in candidate) {
        return candidate?.["field"];
    }
}
"#;
    for source in [plain, optional] {
        assert_eq!(
            unknown_object_diags(source),
            0,
            "`in`-narrowed `unknown` is indexable; no unknown-object diagnostic expected"
        );
    }
}

/// Negative/fallback guard: a possibly-`undefined` (but not `unknown`) base
/// indexed *without* an optional chain still reports the possibly-undefined
/// family (TS18048), never the unknown-object family. Proves the unknown path
/// is not over-applied.
#[test]
fn possibly_undefined_non_optional_index_is_not_unknown_object() {
    let source = r#"
function read(table: { [column: string]: number } | undefined) {
    return table["column"];
}
"#;
    assert_eq!(
        unknown_object_diags(source),
        0,
        "possibly-undefined (not unknown) base must not report TS2571/TS18046"
    );
    assert_eq!(
        count(source, 18048),
        1,
        "possibly-undefined base indexed without `?.` must report TS18048"
    );
}
