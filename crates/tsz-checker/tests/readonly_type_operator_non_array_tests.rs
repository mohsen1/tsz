//! `readonly` as a *type operator* is transparent on any operand that is not a
//! syntactic array or tuple literal type.
//!
//! tsc's `getTypeFromTypeOperatorNode` for the `readonly` keyword returns
//! `getTypeFromTypeNode(node.type)` unchanged; the readonly-ness of an array or
//! tuple is baked into that array/tuple type only when the operator's operand is
//! *syntactically* an `ArrayType`/`TupleType` (`T[]` / `[T, U]`). On every other
//! operand `readonly` is a no-op — a separate grammar check reports TS1354, and
//! the annotation resolves to the operand type itself.
//!
//! tsz previously wrapped every operand in a `ReadonlyType` marker, minting a
//! distinct type that spuriously failed assignability (`let a: readonly number =
//! 1` reported a bogus TS2322) and displayed the operand as `readonly T`. These
//! rows pin the tsc-faithful behavior. Oracle: `typescript@7.0.2`
//! (`--noEmit --strict --target es2022 --lib es2022`).

use std::sync::Arc;
use tsz_binder::lib_loader::LibFile;
use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs_code_messages, load_default_lib_files};

fn strict(source: &str, libs: &[Arc<LibFile>]) -> Vec<(u32, String)> {
    check_source_with_libs_code_messages(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
        libs,
    )
}

fn codes(diags: &[(u32, String)]) -> Vec<u32> {
    let mut c: Vec<u32> = diags.iter().map(|(code, _)| *code).collect();
    c.sort_unstable();
    c.dedup();
    c
}

fn has(diags: &[(u32, String)], code: u32) -> bool {
    diags.iter().any(|(c, _)| *c == code)
}

/// `readonly <primitive>`: TS1354 only, and the operand resolves to the bare
/// primitive so an assignable initializer is accepted (no spurious TS2322).
#[test]
fn readonly_primitive_operand_is_transparent() {
    let libs = load_default_lib_files();
    let diags = strict("let a: readonly number = 1;", &libs);
    assert!(
        has(&diags, 1354),
        "expected TS1354 grammar error: {diags:?}"
    );
    assert!(
        !has(&diags, 2322),
        "readonly number must resolve to number, accepting `1`: {diags:?}"
    );
    assert_eq!(
        codes(&diags),
        vec![1354],
        "only the grammar error: {diags:?}"
    );
}

/// The genuine mismatch under a transparent `readonly` still reports TS2322 —
/// and the target renders as the bare operand (`number`), not `readonly number`.
#[test]
fn readonly_primitive_still_reports_a_real_mismatch_with_bare_display() {
    let libs = load_default_lib_files();
    let diags = strict(r#"let a: readonly number = "x";"#, &libs);
    assert!(has(&diags, 1354), "expected TS1354: {diags:?}");
    let ts2322 = diags
        .iter()
        .find(|(c, _)| *c == 2322)
        .map(|(_, m)| m.clone())
        .unwrap_or_default();
    assert!(
        ts2322.contains("type 'number'"),
        "target must render as bare `number`, not `readonly number`: {ts2322:?}"
    );
    assert!(
        !ts2322.contains("readonly number"),
        "target must not render the readonly wrapper: {ts2322:?}"
    );
}

/// A union operand is not a syntactic array/tuple, so `readonly` is a no-op.
#[test]
fn readonly_union_operand_is_transparent() {
    let libs = load_default_lib_files();
    let diags = strict("let a: readonly (number | string) = 1;", &libs);
    assert_eq!(codes(&diags), vec![1354], "only TS1354: {diags:?}");
}

/// A *parenthesized* array is a `ParenthesizedType`, not an `ArrayType`, so tsc
/// reports TS1354 and the type stays a mutable `number[]` — `.push` is allowed.
#[test]
fn readonly_parenthesized_array_operand_stays_mutable() {
    let libs = load_default_lib_files();
    let diags = strict("let x: readonly (number[]) = [1]; x.push(2);", &libs);
    assert_eq!(
        codes(&diags),
        vec![1354],
        "parenthesized array under readonly must stay mutable (no TS2339 on push): {diags:?}"
    );
}

/// A type reference that merely *aliases* an array is also not a syntactic
/// array, so `readonly Arr` is a no-op and `Arr` stays mutable.
#[test]
fn readonly_array_alias_operand_stays_mutable() {
    let libs = load_default_lib_files();
    let diags = strict(
        "type Arr = number[]; let x: readonly Arr = [1]; x.push(2);",
        &libs,
    );
    assert_eq!(
        codes(&diags),
        vec![1354],
        "aliased array under readonly must stay mutable: {diags:?}"
    );
}

/// Renamed binders must not change the answer — the rule keys on the syntactic
/// operand kind, never on any identifier.
#[test]
fn readonly_transparency_is_independent_of_binder_names() {
    let libs = load_default_lib_files();
    let diags = strict("let zzWidget: readonly bigint = 1n;", &libs);
    assert!(has(&diags, 1354), "expected TS1354: {diags:?}");
    assert!(!has(&diags, 2322), "no spurious mismatch: {diags:?}");
    assert_eq!(codes(&diags), vec![1354], "only TS1354: {diags:?}");
}

/// Control: a genuine `readonly T[]` (syntactic array operand) keeps its
/// readonly semantics — mutating methods are rejected (TS2339) and no TS1354.
#[test]
fn genuine_readonly_array_keeps_readonly_semantics() {
    let libs = load_default_lib_files();
    let diags = strict("let x: readonly number[] = [1]; x.push(2);", &libs);
    assert!(
        !has(&diags, 1354),
        "a syntactic array operand is legal (no TS1354): {diags:?}"
    );
    assert!(
        has(&diags, 2339),
        "readonly array must reject `.push` (TS2339): {diags:?}"
    );
}

/// Control: a genuine `readonly [T]` tuple keeps readonly semantics — element
/// assignment is rejected (TS2540) and no TS1354.
#[test]
fn genuine_readonly_tuple_keeps_readonly_semantics() {
    let libs = load_default_lib_files();
    let diags = strict("let x: readonly [number] = [1]; x[0] = 2;", &libs);
    assert!(
        !has(&diags, 1354),
        "a syntactic tuple operand is legal (no TS1354): {diags:?}"
    );
    assert!(
        has(&diags, 2540),
        "readonly tuple must reject element assignment (TS2540): {diags:?}"
    );
}

// --- Lowering-path contexts ---------------------------------------------------
//
// A `readonly <non-array>` operand reached through generic instantiation, a
// conditional-type branch, a mapped-type value, or a type-parameter default is
// resolved by `tsz_lowering::lower_type_operator`, a separate mint site from the
// two checker `get_type_from_type_operator` entry points. Before the fix these
// wrapped unconditionally too, so each context reproduced the spurious TS2322.
// The transparency rule is shared across all three sites via
// `syntax_kind_ext::is_array_or_tuple_type`, so every context now agrees with
// tsc (TS1354 only, no spurious mismatch).

/// `readonly <non-array>` passed as a generic type argument.
#[test]
fn readonly_transparent_through_generic_argument() {
    let libs = load_default_lib_files();
    let diags = strict("type Id<T> = T; let x: Id<readonly number> = 1;", &libs);
    assert_eq!(codes(&diags), vec![1354], "only TS1354: {diags:?}");
}

/// `readonly <non-array>` in a conditional-type branch.
#[test]
fn readonly_transparent_through_conditional_branch() {
    let libs = load_default_lib_files();
    let diags = strict(
        "type C = true extends true ? readonly number : never; let z: C = 1;",
        &libs,
    );
    assert_eq!(codes(&diags), vec![1354], "only TS1354: {diags:?}");
}

/// `readonly <non-array>` as a mapped-type value.
#[test]
fn readonly_transparent_through_mapped_value() {
    let libs = load_default_lib_files();
    let diags = strict(
        r#"type M = { [K in "a"]: readonly number }; let w: M = { a: 1 };"#,
        &libs,
    );
    assert_eq!(codes(&diags), vec![1354], "only TS1354: {diags:?}");
}

/// `readonly <non-array>` as a type-parameter default.
#[test]
fn readonly_transparent_through_type_parameter_default() {
    let libs = load_default_lib_files();
    let diags = strict("type D<T = readonly number> = T; let x: D = 1;", &libs);
    assert_eq!(codes(&diags), vec![1354], "only TS1354: {diags:?}");
}

/// Control: a genuine `readonly T[]` reached through the lowering path (a type
/// alias body) keeps readonly semantics.
#[test]
fn genuine_readonly_array_alias_keeps_readonly_semantics() {
    let libs = load_default_lib_files();
    let diags = strict(
        "type A = readonly number[]; let x: A = [1]; x.push(2);",
        &libs,
    );
    assert!(
        !has(&diags, 1354),
        "a syntactic array operand is legal (no TS1354): {diags:?}"
    );
    assert!(
        has(&diags, 2339),
        "readonly array alias must reject `.push` (TS2339): {diags:?}"
    );
}
