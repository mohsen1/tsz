//! Tests for `| undefined` propagation through optional chain continuations.
//!
//! Closes #9736. The structural rule:
//!
//! > When a property access, element access, or call is a non-optional
//! > continuation of an optional chain (e.g., the `.b` in `o?.a.b`, or
//! > `()` in `o?.a.b()`), the result type must include `| undefined`
//! > to account for the chain short-circuiting when `o` is nullish.
//!
//! The fix makes `is_optional_chain` recursive for property/element/call
//! access nodes and propagates `| undefined` in the three resolution
//! paths: property access wrapper, element access, and call expressions.
//!
//! Per §25 / §26 of CLAUDE.md, each test uses at least two different
//! property-name spellings to prove the rule is not hardcoded.

use tsz_checker::test_utils::check_source_strict_codes;

// =========================================================================
// Probe helpers
// =========================================================================

/// Returns true if assigning `expr` to a `number`-typed variable produces
/// TS2322, meaning the inferred type includes `undefined`.
fn contains_undefined_in_number_ctx(source: &str) -> bool {
    check_source_strict_codes(source).contains(&2322)
}

fn has_no_error(source: &str) -> bool {
    check_source_strict_codes(source).is_empty()
}

// =========================================================================
// Property access continuations — the reported repro
// =========================================================================

/// `o?.a.b` — the direct repro from #9736.
/// When `o` is `{ a: { b: number } } | null`, the result must be
/// `number | undefined`, so assigning it to `number` fires TS2322.
#[test]
fn property_continuation_adds_undefined() {
    let source = r#"
declare const o: { a: { b: number } } | null;
const x: number = o?.a.b;
"#;
    assert!(
        contains_undefined_in_number_ctx(source),
        "o?.a.b with nullable root must produce `number | undefined`, not `number`",
    );
}

/// Vary property names: `p.q` instead of `a.b` — the rule must not be
/// keyed to specific identifier spellings.
#[test]
fn property_continuation_adds_undefined_different_names() {
    let source = r#"
declare const p: { q: { r: number } } | null;
const x: number = p?.q.r;
"#;
    assert!(
        contains_undefined_in_number_ctx(source),
        "p?.q.r with nullable root must produce `number | undefined`",
    );
}

/// `o?.a.b` assigned to `number | undefined` must produce no error.
#[test]
fn property_continuation_no_error_when_target_includes_undefined() {
    let source = r#"
declare const o: { a: { b: number } } | null;
const x: number | undefined = o?.a.b;
"#;
    assert!(
        has_no_error(source),
        "o?.a.b assigned to `number | undefined` should produce no TS2322",
    );
}

// =========================================================================
// Multi-level continuations
// =========================================================================

/// `o?.a.b.c` — three levels deep; `| undefined` must reach the outermost
/// access, not just the second level.
#[test]
fn multi_level_property_continuation_adds_undefined() {
    let source = r#"
declare const o: { a: { b: { c: string } } } | null;
const x: string = o?.a.b.c;
"#;
    assert!(
        check_source_strict_codes(source).contains(&2322),
        "o?.a.b.c with nullable root must produce `string | undefined`",
    );
}

/// Different names for the same multi-level pattern.
#[test]
fn multi_level_property_continuation_different_names() {
    let source = r#"
declare const node: { left: { right: { value: string } } } | undefined;
const x: string = node?.left.right.value;
"#;
    assert!(
        check_source_strict_codes(source).contains(&2322),
        "node?.left.right.value with nullable root must produce `string | undefined`",
    );
}

// =========================================================================
// Element access continuations
// =========================================================================

/// `o?.a["key"]` — element access that continues an optional chain.
#[test]
fn element_access_continuation_adds_undefined() {
    let source = r#"
declare const o: { a: { [key: string]: number } } | null;
const x: number = o?.a["someKey"];
"#;
    assert!(
        contains_undefined_in_number_ctx(source),
        "o?.a[\"someKey\"] with nullable root must produce `number | undefined`",
    );
}

/// Different names: `m?.data["index"]`.
#[test]
fn element_access_continuation_different_names() {
    let source = r#"
declare const m: { data: { [k: string]: number } } | null;
const x: number = m?.data["index"];
"#;
    assert!(
        contains_undefined_in_number_ctx(source),
        "m?.data[\"index\"] with nullable root must produce `number | undefined`",
    );
}

// =========================================================================
// Call expression continuations
// =========================================================================

/// `o?.a.b()` — a call that follows a `?.` earlier in the chain.
#[test]
fn call_continuation_adds_undefined() {
    let source = r#"
declare const o: { a: { b: () => number } } | null;
const x: number = o?.a.b();
"#;
    assert!(
        contains_undefined_in_number_ctx(source),
        "o?.a.b() with nullable root must produce `number | undefined`",
    );
}

/// Different names to prevent hardcoding: `obj?.fn.invoke()`.
#[test]
fn call_continuation_different_names() {
    let source = r#"
declare const obj: { fn: { invoke: () => number } } | null;
const x: number = obj?.fn.invoke();
"#;
    assert!(
        contains_undefined_in_number_ctx(source),
        "obj?.fn.invoke() with nullable root must produce `number | undefined`",
    );
}

// =========================================================================
// Non-nullable root — no spurious undefined
// =========================================================================

/// When the chain root is NOT nullable, `o?.a.b` should produce just
/// `number`, not `number | undefined`. The fix must check whether the
/// base type actually contains nullish before adding `| undefined`.
#[test]
fn non_nullable_root_no_spurious_undefined() {
    let source = r#"
declare const o: { a: { b: number } };
const x: number = o?.a.b;
"#;
    assert!(
        has_no_error(source),
        "o?.a.b when root is non-nullable must produce `number`, not `number | undefined`",
    );
}

/// Different names for the same non-nullable case.
#[test]
fn non_nullable_root_different_names_no_spurious_undefined() {
    let source = r#"
declare const p: { x: { y: number } };
const v: number = p?.x.y;
"#;
    assert!(
        has_no_error(source),
        "p?.x.y when root is non-nullable must produce `number`",
    );
}

// =========================================================================
// Control cases — direct `?.` access (must not regress)
// =========================================================================

/// Direct `o?.a` (single-level optional chain) must still produce
/// `number | undefined`. This was already correct before the fix;
/// verify no regression.
#[test]
fn direct_optional_chain_still_adds_undefined() {
    let source = r#"
declare const o: { a: number } | null;
const x: number = o?.a;
"#;
    assert!(
        contains_undefined_in_number_ctx(source),
        "o?.a with nullable root must still produce `number | undefined` (regression guard)",
    );
}

/// `o?.a` assigned to `number | undefined` — no error.
#[test]
fn direct_optional_chain_no_error_when_target_includes_undefined() {
    let source = r#"
declare const o: { a: number } | null;
const x: number | undefined = o?.a;
"#;
    assert!(
        has_no_error(source),
        "o?.a assigned to `number | undefined` should not emit TS2322",
    );
}

// =========================================================================
// Nullish coalescing eliminates undefined
// =========================================================================

/// `(o?.a.b) ?? 0` eliminates the `undefined`, so assigning to `number`
/// is fine.
#[test]
fn nullish_coalesce_on_continuation_removes_undefined() {
    let source = r#"
declare const o: { a: { b: number } } | null;
const x: number = (o?.a.b) ?? 0;
"#;
    assert!(
        has_no_error(source),
        "(o?.a.b) ?? 0 should be assignable to `number` after nullish coalescing",
    );
}
