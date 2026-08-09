//! When a source callable's signature and a target overload signature are
//! both generic, `tsc`'s `compareSignaturesRelated` erases each side's own
//! type parameters to `any` (`getErasedSignature`) before running the N×M
//! `signaturesRelatedTo` comparison used for a target with two or more call
//! signatures. A single, non-overloaded target signature never takes this
//! path — it keeps its own type parameters opaque and infers/alpha-renames
//! against the source instead.
//!
//! tsz's N×M erased-signature fallback
//! (`crates/tsz-solver/src/relations/subtype/rules/functions/mod.rs`,
//! `erase_call_sig_to_any`/`erase_fn_shape_to_any`) erased only a signature's
//! own declared `type_params` list. A contextually-typed function
//! expression's parameter can be seeded directly from a target overload's own
//! type-parameter `TypeId` (shared identity) without the expression carrying
//! a `type_params` list of its own, so that shared `T` stayed opaque and
//! never got erased — producing a spurious `TS2322` when the merged
//! overloads' return types disagreed (`<T>(x:T):string` vs `<T>(x:T):number`,
//! issue #16952, a residual of #16950/PR #16957).
//!
//! Fix: `erase_call_sig_to_any`/`erase_fn_shape_to_any` now erase every FREE
//! type-parameter occurrence in a signature that is `is_same_binder` with
//! either its own declared `type_params` or the *paired* signature's declared
//! `type_params` (owner: `crates/tsz-solver/src/relations/subtype/rules/functions/mod.rs::free_type_params_to_any`).
//! A free type parameter belonging to neither side — an outer/captured
//! parameter from an enclosing generic function that both signatures merely
//! reference — stays opaque and un-erased.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source, diagnostic_count};

fn options() -> CheckerOptions {
    CheckerOptions::default()
}

/// The bug witness (#16952): two same-arity generic overloads whose RETURN
/// types disagree. The contextual identity arrow's merged parameter type is a
/// bare, opaque `T` under both overloads (#16950/#16957 already fixed the
/// `T | T` dedup); `tsc` still accepts the assignment via generic-to-generic
/// erasure.
#[test]
fn identity_arrow_against_disagreeing_return_overloads_is_clean() {
    let source = r#"
var f3: {
    <T>(x: T): string;
    <T>(x: T): number;
} = (x) => x;
"#;
    let diags = check_source(source, "disagreeing_returns.ts", options());
    assert_eq!(
        diagnostic_count(&diags, 2322),
        0,
        "a contextual identity arrow against disagreeing-return same-arity \
         generic overloads must not report TS2322 (tsc erases both sides' \
         type params to `any` for this N x M comparison); got: {diags:?}"
    );
}

/// Same shape with three disagreeing-return overloads, proving the erasure
/// is not special-cased to exactly two signatures.
#[test]
fn identity_arrow_against_three_disagreeing_return_overloads_is_clean() {
    let source = r#"
var f5: {
    <T>(x: T): string;
    <T>(x: T): number;
    <T>(x: T): boolean;
} = (x) => x;
"#;
    let diags = check_source(source, "three_disagreeing_returns.ts", options());
    assert_eq!(
        diagnostic_count(&diags, 2322),
        0,
        "three disagreeing-return overloads must still erase cleanly; got: {diags:?}"
    );
}

/// An independently-declared generic function (not a contextually-typed
/// literal) assigned to the same disagreeing-return overload set must also
/// erase cleanly — the fix must not be limited to the contextual-arrow shape.
#[test]
fn named_generic_function_against_disagreeing_return_overloads_is_clean() {
    let source = r#"
function idfn<T>(x: T): T { return x; }
var f9: {
    <T>(x: T): string;
    <T>(x: T): number;
} = idfn;
"#;
    let diags = check_source(source, "named_fn_disagreeing_returns.ts", options());
    assert_eq!(
        diagnostic_count(&diags, 2322),
        0,
        "a named generic identity function against disagreeing-return \
         overloads must not report TS2322; got: {diags:?}"
    );
}

/// Mixed overload set: one generic overload matching the source's own shape,
/// and one fully concrete overload. Erasure must apply per target-signature
/// pair, not require every overload to be generic.
#[test]
fn named_generic_function_against_mixed_generic_and_concrete_overload_is_clean() {
    let source = r#"
function idfn<T>(x: T): T { return x; }
var f13: {
    <T>(x: T): T;
    (x: number): string;
} = idfn;
"#;
    let diags = check_source(source, "mixed_overload.ts", options());
    assert_eq!(
        diagnostic_count(&diags, 2322),
        0,
        "a generic identity function against a mixed generic/concrete \
         overload set must not report TS2322; got: {diags:?}"
    );
}

/// Negative control: a single, non-overloaded generic target signature must
/// keep its prior (correct) rejection. The N x M erasure path is specific to
/// an overloaded (2+ call signature) target; a lone generic target signature
/// alpha-renames/infers against the source instead, and a bare, opaque `T`
/// genuinely is not assignable to a concrete `string`.
#[test]
fn single_non_overloaded_generic_target_still_rejects_return_mismatch() {
    let source = r#"
var f4: <T>(x: T) => string = (x) => x;
"#;
    let diags = check_source(source, "single_target.ts", options());
    assert!(
        diagnostic_count(&diags, 2322) > 0,
        "a single (non-overloaded) generic target signature must still \
         reject a bare-T-vs-string return mismatch; got: {diags:?}"
    );
}

/// Negative control: the source body's return is genuinely concrete (a
/// string literal, not the shared `T`), so it must still fail against
/// whichever overload it does not structurally satisfy -- erasure must not
/// blanket-accept every N x M comparison regardless of the source's real
/// shape.
#[test]
fn concrete_return_still_rejected_by_mismatched_overload() {
    let source = r#"
var f6: {
    <T>(x: T): string;
    <T>(x: T): number;
} = (x) => { return "hello"; };
"#;
    let diags = check_source(source, "concrete_return.ts", options());
    assert!(
        diagnostic_count(&diags, 2322) > 0,
        "a source whose return is genuinely the concrete string 'hello' \
         must still fail against the number overload; got: {diags:?}"
    );
}

/// Negative control (regression guard): a type parameter captured from an
/// *enclosing* generic function -- not declared by either compared signature
/// -- must stay opaque even though it occurs free in both the source and
/// target signatures of an overloaded comparison. Erasing it would silently
/// accept a genuinely unsound assignment (`tsc` rejects this).
#[test]
fn outer_captured_type_param_shared_across_overloads_still_rejects() {
    let source = r#"
function f<Args extends unknown[]>(
  source: {
    (...args: Args): void;
    (...renamed: Args): void;
  },
  target: (value: Args) => void
) { target = source; }
"#;
    let diags = check_source(source, "outer_captured.ts", options());
    assert!(
        diagnostic_count(&diags, 2322) > 0,
        "an outer/captured type parameter shared by identity across both \
         overloads (not declared by either signature) must not be erased; \
         got: {diags:?}"
    );
}
