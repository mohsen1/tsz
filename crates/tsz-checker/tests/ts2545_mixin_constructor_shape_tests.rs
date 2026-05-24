//! TS2545: "A mixin class must have a constructor with a single rest parameter
//! of type 'any[]'."
//!
//! Structural rule:
//! > When `class C extends B` and the declared type of `B` is a type parameter,
//! > the constraint must provide a construct signature with **exactly one rest
//! > parameter** whose element type is `any` (or whose parameter type is bare
//! > `any`). Every other shape — zero parameters, multiple parameters, a single
//! > non-rest parameter, an optional rest, or a rest of a non-`any` element
//! > type — fails the mixin contract and produces TS2545. Generic
//! > constructor signatures inferred through conditional mixin helpers are
//! > preserved as accepted by `tsc`.
//!
//! Reported in #9729: the zero-parameter case (`new () => object`) was silently
//! accepted. tsc emits TS2545 in every "not a single `...any[]` rest" shape;
//! tsz now matches for the non-generic single-signature case.
//!
//! Adjacent matrix (verified against `tsc 6.0.2`):
//!   1. zero params (reported bug)                       → TS2545
//!   2. zero params with renamed type parameter          → TS2545
//!   3. multiple typed params                            → TS2545
//!   4. single non-rest typed param                      → TS2545
//!   5. single non-rest `any[]` param                    → TS2545
//!   6. rest of `unknown[]`                              → TS2545
//!   7. rest of `never[]`                                → TS2545
//!   8. valid `...any[]`                                 → no TS2545 (control)
//!   9. valid bare `...any`                              → no TS2545 (control)
//!  10. valid `...readonly any[]`                        → no TS2545 (control)
//!  11. all-overloads-valid set                          → no TS2545 (control)
//!  12. constraint with no construct sigs                → no TS2545 (control)
//!  13. conditional-inferred generic mixin constructor    → no TS2545 (control)
//!
//! Out of scope (preserved pre-existing behavior): multi-sig constraints
//! (overload sets, intersection-of-ctor) with a zero-param member. tsc
//! fires TS2545 in those cases via the `signatures.length === 1` clause of
//! `isMixinConstructorType`; tsz currently does not. Generalizing to that
//! rule is tracked separately to keep #9729 focused on the reported
//! single-signature case.

use tsz_checker::test_utils::check_source_diagnostics;

fn count_ts2545(source: &str) -> usize {
    check_source_diagnostics(source)
        .iter()
        .filter(|d| d.code == 2545)
        .count()
}

fn codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .iter()
        .map(|d| d.code)
        .collect()
}

// -- BUG cases (TS2545 must fire) -------------------------------------------

#[test]
fn ts2545_zero_param_constructor_constraint() {
    // Reported repro from #9729.
    let source = r#"
function m<T extends new () => object>(B: T) {
    return class extends B {};
}
"#;
    let n = count_ts2545(source);
    assert_eq!(
        n,
        1,
        "zero-param constructor constraint must emit TS2545; got codes: {:?}",
        codes(source)
    );
}

#[test]
fn ts2545_zero_param_with_renamed_type_parameter() {
    // Structural, not name-based: renaming `T` to `MyBase` must still trigger.
    let source = r#"
function build<MyBase extends new () => object>(B: MyBase) {
    return class extends B {};
}
"#;
    assert_eq!(
        count_ts2545(source),
        1,
        "renamed type-parameter must not change the rule; got codes: {:?}",
        codes(source)
    );
}

#[test]
fn ts2545_multiple_typed_params() {
    // Two-argument constructor: not a single rest param.
    let source = r#"
function m<T extends new (a: any, b: any) => object>(B: T) {
    return class extends B {};
}
"#;
    assert_eq!(
        count_ts2545(source),
        1,
        "multi-param constructor must emit TS2545; got codes: {:?}",
        codes(source)
    );
}

#[test]
fn ts2545_single_non_rest_typed_param() {
    let source = r#"
function m<T extends new (x: number) => object>(B: T) {
    return class extends B {};
}
"#;
    assert_eq!(
        count_ts2545(source),
        1,
        "single non-rest param must emit TS2545; got codes: {:?}",
        codes(source)
    );
}

#[test]
fn ts2545_single_non_rest_any_array_param() {
    // `(a: any[])` is non-rest — the parameter must be a rest declaration.
    let source = r#"
function m<T extends new (a: any[]) => object>(B: T) {
    return class extends B {};
}
"#;
    assert_eq!(
        count_ts2545(source),
        1,
        "single non-rest `any[]` param must emit TS2545; got codes: {:?}",
        codes(source)
    );
}

#[test]
fn ts2545_rest_of_unknown_array() {
    let source = r#"
function m<T extends new (...a: unknown[]) => object>(B: T) {
    return class extends B {};
}
"#;
    assert_eq!(
        count_ts2545(source),
        1,
        "rest of `unknown[]` must emit TS2545; got codes: {:?}",
        codes(source)
    );
}

#[test]
fn ts2545_rest_of_never_array() {
    let source = r#"
function m<T extends new (...a: never[]) => object>(B: T) {
    return class extends B {};
}
"#;
    assert_eq!(
        count_ts2545(source),
        1,
        "rest of `never[]` must emit TS2545; got codes: {:?}",
        codes(source)
    );
}

// -- Known divergence (not asserted): multi-sig constraints --
// tsc's `isMixinConstructorType` requires `signatures.length === 1`, so an
// overload set or intersection containing a zero-param construct signature
// also fires TS2545 in tsc. tsz preserves its pre-existing multi-sig
// behavior here (skips zero-param sigs in the multi-sig branch); generalizing
// to the count==1 rule is tracked separately to keep #9729 focused on the
// reported single-signature case.

// -- CONTROL cases (TS2545 must NOT fire) -----------------------------------

#[test]
fn no_ts2545_for_single_rest_any_array() {
    let source = r#"
function m<T extends new (...a: any[]) => object>(B: T) {
    return class extends B {};
}
"#;
    assert_eq!(
        count_ts2545(source),
        0,
        "single rest `any[]` is the canonical valid shape; got codes: {:?}",
        codes(source)
    );
}

#[test]
fn no_ts2545_for_bare_any_rest_param() {
    // `(...a: any)` (rest of bare `any`) is accepted by tsc.
    let source = r#"
function m<T extends new (...a: any) => object>(B: T) {
    return class extends B {};
}
"#;
    assert_eq!(
        count_ts2545(source),
        0,
        "bare `any` rest param must not emit TS2545; got codes: {:?}",
        codes(source)
    );
}

#[test]
fn no_ts2545_for_readonly_any_rest_array() {
    let source = r#"
function m<T extends new (...a: readonly any[]) => object>(B: T) {
    return class extends B {};
}
"#;
    assert_eq!(
        count_ts2545(source),
        0,
        "`readonly any[]` rest must not emit TS2545; got codes: {:?}",
        codes(source)
    );
}

#[test]
fn no_ts2545_when_all_overloads_valid() {
    let source = r#"
type Ctor = { new (...a: any[]): object; new (...a: any[]): { extra: 1 } };
function m<T extends Ctor>(B: T) {
    return class extends B {};
}
"#;
    assert_eq!(
        count_ts2545(source),
        0,
        "all-overloads-valid set must not emit TS2545; got codes: {:?}",
        codes(source)
    );
}

#[test]
fn no_ts2545_for_conditional_inferred_generic_mixin_constructor() {
    // Regression witness from `doubleMixinConditionalTypeBaseClassWorks.ts`.
    // The inferred constructor shape is generic mixin output, not the plain
    // `new () => object` constraint reported in #9729.
    let source = r#"
type Constructor = new (...args: any[]) => {};

const Mixin1 = <C extends Constructor>(Base: C) => class extends Base {
    private _fooPrivate: {};
};

type FooConstructor = typeof Mixin1 extends (a: Constructor) => infer Cls ? Cls : never;
const Mixin2 = <C extends FooConstructor>(Base: C) => class extends Base {};

class C extends Mixin2(Mixin1(Object)) {}
"#;
    assert_eq!(
        count_ts2545(source),
        0,
        "conditional-inferred generic mixin constructor must not emit TS2545; got codes: {:?}",
        codes(source)
    );
}

#[test]
fn no_ts2545_when_base_instance_type_is_not_object_like() {
    // tsc gates the TS2545 emission on `getBaseTypes(type)` being non-empty,
    // which is false when the base constructor's return type is `never`,
    // `void`, `null`, `undefined`, etc. In those cases tsc emits TS2509
    // instead and skips the mixin-shape check. Mirror that here so the two
    // diagnostics don't both fire on the same heritage clause.
    for return_ty in ["never", "void", "undefined"] {
        let source = format!(
            r#"
function m<T extends new () => {return_ty}>(B: T) {{
    return class extends B {{}};
}}
"#
        );
        assert_eq!(
            count_ts2545(&source),
            0,
            "TS2545 must be skipped when base ctor returns `{return_ty}`; got codes: {:?}",
            codes(&source)
        );
    }
}

#[test]
fn no_ts2545_when_constraint_has_no_construct_signatures() {
    // No construct sigs at all → rule does not apply; tsc reports TS2507 for
    // the missing constructor type, not TS2545.
    let source = r#"
function m<T extends { foo: number }>(B: T) {
    return class extends (B as any) {};
}
"#;
    assert_eq!(
        count_ts2545(source),
        0,
        "constraint without construct sigs must not emit TS2545; got codes: {:?}",
        codes(source)
    );
}
