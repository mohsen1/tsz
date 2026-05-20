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

// ---------------------------------------------------------------------------
// 7. Application constraints that evaluate to callable unions (e.g. React's
//    `ComponentType<P> = ComponentClass<P> | FunctionComponent<P>`).
//
// Structural rule: when a type parameter constraint is an Application alias
// that expands to a union where every member has call or construct signatures,
// tsz must recognise the constraint as callable and emit TS2344 for
// non-callable type arguments — regardless of whether the alias name is
// `ComponentType`, `Renderable`, or any other user-chosen identifier.
// ---------------------------------------------------------------------------

const COMPONENT_TYPE_PRELUDE: &str = r#"
interface ComponentClass<P> { new(props: P): object; }
interface FunctionComponent<P> { (props: P): any; }
type ComponentType<P> = ComponentClass<P> | FunctionComponent<P>;

interface AnyComponent { displayName?: string; theme?: any }
type RequiresComponent<C extends ComponentType<any>> = { wrapped: C };
"#;

fn with_component_prelude(body: &str) -> String {
    format!("{COMPONENT_TYPE_PRELUDE}\n{body}")
}

#[test]
fn non_callable_alias_does_not_satisfy_application_callable_union_constraint_ts2344() {
    let codes = check_source_codes(&with_component_prelude(
        "type Result = RequiresComponent<AnyComponent>;",
    ));
    assert_code!(
        codes,
        2344,
        "non-callable object type against Application-callable-union constraint must emit TS2344."
    );
    assert_no_code!(codes, 2589, "TS2589 must not fire.");
}

#[test]
fn non_callable_alias_renamed_does_not_satisfy_application_callable_union_constraint_ts2344() {
    // Uses a different alias name to prove the fix is not hardcoded to 'AnyComponent'.
    let codes = check_source_codes(&with_component_prelude(
        r#"
type PlainObj = { id: string; label?: string };
type Result = RequiresComponent<PlainObj>;
"#,
    ));
    assert_code!(
        codes,
        2344,
        "renamed non-callable object against Application-callable-union constraint must emit TS2344."
    );
    assert_no_code!(codes, 2589, "TS2589 must not fire.");
}

#[test]
fn callable_type_satisfies_application_callable_union_constraint_no_ts2344() {
    let codes = check_source_codes(&with_component_prelude(
        "type Result = RequiresComponent<FunctionComponent<any>>;",
    ));
    assert_no_code!(
        codes,
        2344,
        "FunctionComponent<any> satisfies ComponentType<any> — must NOT emit TS2344."
    );
}

#[test]
fn construct_only_type_satisfies_application_callable_union_constraint_no_ts2344() {
    let codes = check_source_codes(&with_component_prelude(
        "type Result = RequiresComponent<ComponentClass<any>>;",
    ));
    assert_no_code!(
        codes,
        2344,
        "ComponentClass<any> satisfies ComponentType<any> through construct signatures."
    );
}

#[test]
fn intersection_non_callable_with_type_param_against_application_callable_union_ts2344() {
    // `AnyComponent & C` is an intersection with a type parameter — C is already
    // constrained to `ComponentType<any>`, so no TS2344 at the declaration site.
    let codes = check_source_codes(&with_component_prelude(
        r#"
declare function styled<C extends ComponentType<any>>(
    c: AnyComponent & C
): void;
"#,
    ));
    assert_no_code!(
        codes,
        2344,
        "type parameter C already constrained to ComponentType<any> — no TS2344 on the declaration."
    );
}

#[test]
fn renamed_application_callable_union_constraint_ts2344() {
    // Uses an alias name different from `ComponentType` to prove no hardcoding.
    let codes = check_source_codes(
        r#"
interface Ctor<P> { new(props: P): object; }
interface Fn<P> { (props: P): any; }
type Renderable<P> = Ctor<P> | Fn<P>;

interface Widget { name: string; }
type RequiresRenderable<R extends Renderable<any>> = { r: R };
type Result = RequiresRenderable<Widget>;
"#,
    );
    assert_code!(
        codes,
        2344,
        "non-callable type against renamed Application-callable-union must emit TS2344."
    );
    assert_no_code!(codes, 2589, "TS2589 must not fire.");
}

#[test]
fn callable_type_satisfies_renamed_application_callable_union_no_ts2344() {
    let codes = check_source_codes(
        r#"
interface Ctor<P> { new(props: P): object; }
interface Fn<P> { (props: P): any; }
type Renderable<P> = Ctor<P> | Fn<P>;
type RequiresRenderable<R extends Renderable<any>> = { r: R };

interface MyFn { (props: any): string; }
type Result = RequiresRenderable<MyFn>;
"#,
    );
    assert_no_code!(
        codes,
        2344,
        "callable type satisfying renamed Application-callable-union must NOT emit TS2344."
    );
}

#[test]
fn construct_only_type_satisfies_renamed_application_callable_union_no_ts2344() {
    let codes = check_source_codes(
        r#"
interface Ctor<P> { new(props: P): object; }
interface Fn<P> { (props: P): any; }
type Renderable<P> = Ctor<P> | Fn<P>;
type RequiresRenderable<R extends Renderable<any>> = { r: R };

interface MyCtor { new (props: any): object; }
type Result = RequiresRenderable<MyCtor>;
"#,
    );
    assert_no_code!(
        codes,
        2344,
        "construct-only type satisfying renamed Application-callable-union must NOT emit TS2344."
    );
}
