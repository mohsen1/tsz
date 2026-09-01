//! Property/element access on a union of generic lib references produced by
//! narrowing (or constraint-position substitution).
//!
//! Structural rule: when a value whose type is a union of generic lib
//! `Application` references — e.g. `Int32Array | Uint8Array` — reaches a
//! property or element access, the access must see each member's resolved
//! interface shape (its members and index signatures). Such unions arise from
//! narrowing a union operand by a user-defined type guard or from
//! constraint-position substitution of a generic parameter; their members are
//! interned as raw `Application(Lazy(DefId), …)` types, which the solver's
//! environment-free evaluator leaves opaque. Resolving the application members
//! through the type environment before access makes `arr[1]` resolve to the
//! numeric-index element type (no TS7053) and `arr.length` resolve to the shared
//! property (no TS2339), matching `tsc`. A `number` member (no index signature)
//! still makes the access fail, exactly as in `tsc`.

use std::sync::Arc;
use tsz_binder::lib_loader::LibFile;
use tsz_common::options::checker::CheckerOptions;

fn opts() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    }
}

fn libs() -> Vec<Arc<LibFile>> {
    crate::test_utils::load_lib_files(&["es5.d.ts"])
}

fn codes(source: &str) -> Vec<u32> {
    crate::test_utils::check_source_with_libs(source, "test.ts", opts(), &libs())
        .iter()
        .map(|diag| diag.code)
        .collect()
}

fn count(source: &str, code: u32) -> usize {
    codes(source).iter().filter(|&&c| c == code).count()
}

// ---------------------------------------------------------------------------
// Reported repro (#10500): a generic parameter whose constraint is
// `number | TypedArray` is substituted to its base constraint at `arr[1]`
// (constraint-position substitution, #10494). The `isTypedArray` guard narrows
// the substituted union to `Int32Array | Uint8Array`; indexing that union must
// resolve through the typed arrays' numeric index signature, not report TS7053.
// ---------------------------------------------------------------------------

#[test]
fn generic_constraint_typed_array_index_after_guard_has_no_ts7053() {
    let source = r#"
type TypedArray = Int32Array | Uint8Array;
function isTypedArray(a: unknown): a is TypedArray { return true as any; }
function flatten<T extends number | TypedArray>(arr: T): void {
    if (isTypedArray(arr)) {
        const x: number = arr[1];
    }
}
"#;
    assert_eq!(count(source, 7053), 0, "got {:?}", codes(source));
    assert_eq!(count(source, 2322), 0, "got {:?}", codes(source));
}

#[test]
fn generic_constraint_typed_array_index_after_guard_renamed_params() {
    // Same rule with P/X instead of T — proves the fix is not name-keyed.
    let source = r#"
type TypedArray = Int32Array | Uint8Array;
function isTypedArray(a: unknown): a is TypedArray { return true as any; }
function flatten<P extends number | TypedArray>(o: P): void {
    if (isTypedArray(o)) {
        const x: number = o[0];
    }
}
"#;
    assert_eq!(count(source, 7053), 0, "got {:?}", codes(source));
}

// ---------------------------------------------------------------------------
// Non-generic union operand narrowed by a user-defined type guard: the same
// resolution must apply whether the union came from a generic constraint or a
// directly-declared variable.
// ---------------------------------------------------------------------------

#[test]
fn declared_union_index_after_guard_has_no_ts7053() {
    let source = r#"
function isI32(a: unknown): a is Int32Array | Uint8Array { return true as any; }
declare const z: number | Int32Array | Uint8Array;
if (isI32(z)) {
    const x: number = z[1];
}
"#;
    assert_eq!(count(source, 7053), 0, "got {:?}", codes(source));
}

#[test]
fn declared_union_property_access_after_guard_has_no_ts2339() {
    // Property access on the narrowed union must also see the shared interface
    // member: `length` exists on both Int32Array and Uint8Array.
    let source = r#"
type TypedArray = Int32Array | Uint8Array;
function isTypedArray(a: unknown): a is TypedArray { return true as any; }
declare const z: number | Int32Array | Uint8Array;
if (isTypedArray(z)) {
    const n: number = z.length;
}
"#;
    assert_eq!(count(source, 2339), 0, "got {:?}", codes(source));
}

// ---------------------------------------------------------------------------
// Negative / fallback cases.
// ---------------------------------------------------------------------------

#[test]
fn number_member_without_guard_still_reports_ts7053() {
    // A `number` member has no index signature, so indexing the un-narrowed
    // union must still report TS7053, exactly as tsc does.
    let source = r#"
function index<T extends number | Int32Array>(r: T): void {
    r[0];
}
"#;
    assert_eq!(count(source, 7053), 1, "got {:?}", codes(source));
}

#[test]
fn else_branch_keeps_number_member() {
    // The guard's else branch keeps the `number` member; indexing it stays an
    // error and `number` remains assignable to `number`.
    let source = r#"
function isI32(a: unknown): a is Int32Array { return true as any; }
declare const w: number | Int32Array;
if (isI32(w)) {
} else {
    const b: number = w;
}
"#;
    assert_eq!(count(source, 2322), 0, "got {:?}", codes(source));
}
