//! Regression tests for TS2344 when a non-callable type is used as a type
//! argument whose constraint is a callable union (`(props: P) => any | new (props: P) => any`).
//!
//! Structural rule: a concrete non-callable object type (or an intersection
//! where no member is callable) does not satisfy a callable union constraint,
//! so tsz must emit TS2344 without hitting the instantiation limit (TS2589).

use crate::test_utils::check_source_codes;

const CALLABLE_UNION_PRELUDE: &str = r#"
type CallableUnion<P> = ((props: P) => any) | (new (props: P) => any);
type AnyWrapped = { displayName?: string; theme?: any };
type Requires<X extends CallableUnion<any>> = { x: X };
"#;

fn with_prelude(body: &str) -> String {
    format!("{CALLABLE_UNION_PRELUDE}\n{body}")
}

macro_rules! assert_code {
    ($codes:ident, $code:literal, $msg:literal) => {
        assert!($codes.contains(&$code), concat!($msg, " Got: {:?}"), $codes)
    };
}

macro_rules! assert_no_code {
    ($codes:ident, $code:literal, $msg:literal) => {
        assert!(
            !$codes.contains(&$code),
            concat!($msg, " Got: {:?}"),
            $codes
        )
    };
}

// ---------------------------------------------------------------------------
// 1. Non-callable alias does NOT satisfy a callable union constraint
// ---------------------------------------------------------------------------

#[test]
fn non_callable_alias_does_not_satisfy_callable_union_constraint_ts2344() {
    let codes = check_source_codes(&with_prelude("type Result = Requires<AnyWrapped>;"));
    assert_code!(
        codes,
        2344,
        "non-callable object type used as callable-union type arg must emit TS2344."
    );
    assert_no_code!(
        codes,
        2589,
        "TS2589 must NOT fire for a simple non-callable type against callable constraint."
    );
}

#[test]
fn non_callable_alias_renamed_does_not_satisfy_callable_union_constraint_ts2344() {
    let codes = check_source_codes(&with_prelude(
        r#"
type WrappedComp = { displayName?: string; theme?: any };
type Result = Requires<WrappedComp>;
"#,
    ));
    assert_code!(
        codes,
        2344,
        "renamed non-callable alias against callable-union constraint must emit TS2344."
    );
    assert_no_code!(codes, 2589, "TS2589 must not fire.");
}

// ---------------------------------------------------------------------------
// 2. Function type satisfies callable union constraint
// ---------------------------------------------------------------------------

#[test]
fn function_type_satisfies_callable_union_constraint_no_ts2344() {
    let codes = check_source_codes(&with_prelude(
        r#"
type FnComp = (props: any) => any;
type Result = Requires<FnComp>;
"#,
    ));
    assert_no_code!(
        codes,
        2344,
        "function type satisfying callable-union constraint must NOT emit TS2344."
    );
}

#[test]
fn function_type_renamed_satisfies_callable_union_constraint_no_ts2344() {
    let codes = check_source_codes(&with_prelude(
        r#"
type Handler = (props: { id: number }) => string;
type Result = Requires<Handler>;
"#,
    ));
    assert_no_code!(
        codes,
        2344,
        "renamed function type satisfying callable-union constraint must NOT emit TS2344."
    );
}

// ---------------------------------------------------------------------------
// 3. Class type satisfies callable union constraint
// ---------------------------------------------------------------------------

#[test]
fn class_type_satisfies_callable_union_constraint_no_ts2344() {
    let codes = check_source_codes(&with_prelude(
        r#"
declare class ClassComp { constructor(props: any); }
type Result = Requires<typeof ClassComp>;
"#,
    ));
    assert_no_code!(
        codes,
        2344,
        "class type with matching constructor should satisfy callable-union constraint."
    );
}

#[test]
fn class_type_renamed_satisfies_callable_union_constraint_no_ts2344() {
    let codes = check_source_codes(&with_prelude(
        r#"
declare class Widget { constructor(props: any); }
type Result = Requires<typeof Widget>;
"#,
    ));
    assert_no_code!(
        codes,
        2344,
        "renamed class type should satisfy callable-union constraint."
    );
}

// ---------------------------------------------------------------------------
// 4. Intersection of non-callable types
// ---------------------------------------------------------------------------

#[test]
fn intersection_of_two_non_callable_objects_ts2344() {
    let codes = check_source_codes(&with_prelude(
        r#"
type Meta = { version: number };
type Styled = AnyWrapped & Meta;
type Result = Requires<Styled>;
"#,
    ));
    assert_code!(
        codes,
        2344,
        "intersection of non-callable object types against callable constraint must emit TS2344."
    );
    assert_no_code!(
        codes,
        2589,
        "TS2589 must not fire for a concrete non-callable intersection."
    );
}

#[test]
fn intersection_of_two_non_callable_objects_renamed_ts2344() {
    let codes = check_source_codes(&with_prelude(
        r#"
type Attrs = { id: string };
type Extras = { label?: string };
type Combined = Attrs & Extras;
type Result = Requires<Combined>;
"#,
    ));
    assert_code!(
        codes,
        2344,
        "renamed intersection of non-callable objects against callable constraint must emit TS2344."
    );
}

// ---------------------------------------------------------------------------
// 5. Concrete non-callable intersection at call sites
// ---------------------------------------------------------------------------

#[test]
fn intersection_non_callable_with_non_callable_concrete_arg_ts2344() {
    let codes = check_source_codes(&with_prelude(
        r#"
type WithClosed<C> = AnyWrapped & C;
type Result = Requires<WithClosed<{ label: string }>>;
"#,
    ));
    assert_code!(
        codes,
        2344,
        "concrete non-callable intersection instantiated at call site must emit TS2344."
    );
    assert_no_code!(codes, 2589, "TS2589 must not fire.");
}

// ---------------------------------------------------------------------------
// 6. No instantiation limit for complex callable-constraint checks (pins
//    styledComponentsInstantiationLimitNotReached conformance baseline)
// ---------------------------------------------------------------------------

#[test]
fn no_instantiation_limit_for_complex_callable_constraint_check() {
    let codes = check_source_codes(&with_prelude(
        r#"
type Base = { as?: any; ref?: any };
type Styled1 = Base & AnyWrapped;
type Styled2 = Styled1 & { className?: string };
type R1 = Requires<Styled1>;
type R2 = Requires<Styled2>;
"#,
    ));
    assert_code!(
        codes,
        2344,
        "non-callable styled-like intersection against callable constraint must emit TS2344."
    );
    assert_no_code!(
        codes,
        2589,
        "TS2589 (instantiation limit) must not fire — eager TS2344 should short-circuit."
    );
}

#[test]
fn no_instantiation_limit_renamed_callable_constraint_check() {
    let codes = check_source_codes(&with_prelude(
        r#"
type Metadata = { version: number; author?: string };
type Enriched = Metadata & { debug?: boolean };
type Wrapped = Enriched & { id: string };
type R1 = Requires<Enriched>;
type R2 = Requires<Wrapped>;
"#,
    ));
    assert_code!(
        codes,
        2344,
        "renamed non-callable chain against callable constraint must emit TS2344."
    );
    assert_no_code!(
        codes,
        2589,
        "TS2589 must not fire for renamed non-callable chain."
    );
}
