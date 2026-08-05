//! Regression tests for #16478: a function-like source relates to a non-weak
//! object target through its **apparent type** — the call signatures plus the
//! full global `Function` interface surface (`length`, `name`, `bind`, `call`,
//! `apply`, `arguments`, `caller`, `prototype`, ...) — not a two-name stub.
//!
//! Structural rule: when the source is function-like, `tsc` compares against
//! `getApparentType`, which merges the `Function` interface. A target requiring
//! any of those members is therefore satisfied (with the member's *declared*
//! type, so `{ length: number }` is accepted but `{ length: string }` is not).
//!
//! The apparent surface must NOT leak into the weak-type rule (TS2559): `tsc`'s
//! `hasCommonProperties` scans the source's *own* properties (empty for a bare
//! function), so an all-optional target sharing only a `Function` member name
//! (e.g. `{ length?: number }`) is still rejected. Owner:
//! `crates/tsz-solver/src/relations/subtype/function_apparent.rs`
//! (`function_apparent_full_object_shape`), dispatched from `core_dispatch.rs`.
//!
//! The fix resolves the `Function` interface by identity through the boxed-type
//! registry, not by matching any identifier, so these tests vary binder names
//! (aliases, named function declarations) to confirm it is structural.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs, load_default_lib_files};
use tsz_common::common::ScriptTarget;

fn codes(source: &str) -> Vec<u32> {
    let libs = load_default_lib_files();
    check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            target: ScriptTarget::ES2022,
            strict: true,
            ..CheckerOptions::default()
        },
        &libs,
    )
    .into_iter()
    .map(|d| d.code)
    .collect()
}

// ---- Positive rows: required `Function` members are satisfied. ------------

#[test]
fn required_length_member_is_satisfied() {
    assert!(
        codes("var q: { length: number } = () => {};").is_empty(),
        "a function satisfies a required `length: number` via its apparent type"
    );
}

#[test]
fn required_name_member_is_satisfied() {
    assert!(
        codes("var q: { name: string } = () => {};").is_empty(),
        "a function satisfies a required `name: string` via its apparent type"
    );
}

#[test]
fn required_bind_method_is_satisfied() {
    assert!(
        codes("var q: { bind(...a: any[]): any } = () => {};").is_empty(),
        "a function satisfies a required `bind(...)` method via its apparent type"
    );
}

#[test]
fn required_call_and_apply_methods_are_satisfied() {
    assert!(
        codes("var q: { call(...a: any[]): any; apply(...a: any[]): any } = () => {};").is_empty(),
        "a function satisfies required `call`/`apply` via its apparent type"
    );
}

// ---- The member's *declared* type is enforced. ---------------------------

#[test]
fn required_length_with_wrong_type_still_errors() {
    // `Function.length` is `number`, not `string`.
    assert!(
        codes("var q: { length: string } = () => {};").contains(&2322),
        "a required `length: string` must fail: apparent `length` is `number`"
    );
}

#[test]
fn required_missing_member_still_errors() {
    // `zzz` is on neither the function nor the `Function` interface.
    let got = codes("var q: { zzz: string } = () => {};");
    assert!(
        !got.is_empty(),
        "a required member absent from the apparent type must still fail; got {got:?}"
    );
}

// ---- Weak-type rule (TS2559) is preserved: own-property scan, not the
//      apparent surface. -------------------------------------------------

#[test]
fn weak_target_sharing_only_a_function_member_name_is_rejected() {
    // `{ length?: number }` is a weak type; a bare function has no *own*
    // property in common with it, so tsc reports TS2559 even though the
    // apparent type carries `length`.
    assert!(
        codes("var q: { length?: number } = () => {};").contains(&2559),
        "an all-optional target sharing only a `Function` member name is weak-rejected"
    );
}

#[test]
fn weak_target_with_unrelated_optional_is_rejected() {
    assert!(
        codes("var q: { zzz?: string } = () => {};").contains(&2559),
        "an all-optional target with no common own property is weak-rejected"
    );
}

// ---- Intersection member (weak rule suppressed) still succeeds. ----------

#[test]
fn optional_intersection_member_is_accepted() {
    // `{ brand?: number }` reached as an intersection member: the weak rule is
    // suppressed and the optional member is satisfiable, so this is clean.
    assert!(
        codes("var f: (() => void) & { brand?: number } = Object.assign(() => {}, { brand: 1 });")
            .is_empty(),
        "an optional intersection member of a function type is accepted"
    );
}

// ---- Constructor sources get the same apparent surface. ------------------

#[test]
fn constructor_satisfies_required_function_member() {
    assert!(
        codes("class C {}\nvar q: { name: string } = C;").is_empty(),
        "a constructor satisfies a required `name: string` via its apparent type"
    );
}

#[test]
fn constructor_missing_required_member_still_errors() {
    let got = codes("class C {}\nvar q: { zzz: string } = C;");
    assert!(
        !got.is_empty(),
        "a constructor missing a required member must still fail; got {got:?}"
    );
}

// ---- Name-agnostic: the fix is not keyed on any identifier. --------------

#[test]
fn apparent_surface_is_name_agnostic() {
    // Named function declaration + named type alias with arbitrary identifiers:
    // resolution goes through the `Function` interface identity, not any name.
    let clean = r#"
type WidgetShape = { length: number; name: string };
function buildWidget(): void {}
var w: WidgetShape = buildWidget;
"#;
    assert!(
        codes(clean).is_empty(),
        "apparent-type satisfaction must not depend on binder names"
    );

    // Same shape, different names, plus a genuinely missing member: still fails.
    let bad = r#"
type GadgetShape = { length: number; sprocket: string };
function makeGadget(): void {}
var g: GadgetShape = makeGadget;
"#;
    assert!(
        !codes(bad).is_empty(),
        "a genuinely missing required member must still fail regardless of names"
    );
}
