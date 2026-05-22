//! TS2545: "A mixin class must have a constructor with a single rest parameter
//! of type 'any[]'."
//!
//! Structural rule:
//! > When `class C extends B` and the declared type of `B` is a type parameter,
//! > the constraint must provide a construct signature with **exactly one rest
//! > parameter** whose element type is `any` (or whose parameter type is bare
//! > `any`). Every other shape — zero parameters, multiple parameters, a single
//! > non-rest parameter, an optional rest, or a rest of a non-`any` element
//! > type — fails the mixin contract and produces TS2545.
//!
//! Reported in #9729: the zero-parameter case (`new () => object`) was silently
//! accepted. tsc emits TS2545 in every "not a single `...any[]` rest" shape;
//! tsz now matches.
//!
//! Adjacent matrix (verified against `tsc 6.0.2`):
//!   1. zero params (reported bug)                       → TS2545
//!   2. zero params with renamed type parameter          → TS2545
//!   3. multiple typed params                            → TS2545
//!   4. single non-rest typed param                      → TS2545
//!   5. single non-rest `any[]` param                    → TS2545
//!   6. rest of `unknown[]`                              → TS2545
//!   7. rest of `never[]`                                → TS2545
//!   8. overload set containing a zero-param sig         → TS2545
//!   9. intersection containing a zero-param ctor        → TS2545
//!  10. valid `...any[]`                                 → no TS2545 (control)
//!  11. valid bare `...any`                              → no TS2545 (control)
//!  12. valid `...readonly any[]`                        → no TS2545 (control)
//!  13. all-overloads-valid set                          → no TS2545 (control)
//!  14. constraint with no construct sigs                → no TS2545 (control)

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

#[test]
fn ts2545_overload_set_with_one_zero_param_sig() {
    // tsc treats the constraint as invalid if *any* member overload fails.
    let source = r#"
type Ctor = { new (): object; new (...a: any[]): object };
function m<T extends Ctor>(B: T) {
    return class extends B {};
}
"#;
    assert_eq!(
        count_ts2545(source),
        1,
        "overload set with a bad sig must emit TS2545; got codes: {:?}",
        codes(source)
    );
}

#[test]
fn ts2545_intersection_with_zero_param_ctor() {
    let source = r#"
type Inter = (new () => object) & (new (...a: any[]) => object);
function m<T extends Inter>(B: T) {
    return class extends B {};
}
"#;
    assert_eq!(
        count_ts2545(source),
        1,
        "intersection containing a zero-param ctor must emit TS2545; got codes: {:?}",
        codes(source)
    );
}

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
