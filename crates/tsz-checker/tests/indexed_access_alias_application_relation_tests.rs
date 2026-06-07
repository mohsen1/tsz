//! Regression tests for relating two applications of an *indexed-access* type
//! alias (issue #10834 family — recursive utility expansion / structural
//! comparison of computed aliases).
//!
//! `DefKind::TypeAlias` is transparent: `tsc` never compares two applications of
//! a type alias nominally — it substitutes the arguments and relates the
//! resulting structural types. For an alias whose body is an `IndexAccess`
//! transform — the `TypeBox` shape `Static<T> = T['static']` /
//! `Static<T,P> = (T & {params:P})['static']` — tsz previously took the same-base
//! variance fast path, comparing the raw arguments. Through their nested
//! same-base applications that comparison hit the coinductive cycle assumption
//! and silently reported `Static<A>` assignable to `Static<B>` even when the
//! expanded objects differed by a (deeply nested) property. The relation now
//! skips the variance fast path for indexed-access alias bases and compares the
//! evaluated structural forms, matching `tsc`.
//!
//! The assertions check diagnostic *codes* and direction, not the rendered type
//! strings (tsz renders the user's alias name where tsc expands structurally;
//! both are valid and the rendering is asserted elsewhere).

use crate::test_utils::check_source_diagnostics;

fn error_codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .iter()
        .map(|d| d.code)
        .collect()
}

const SCHEMA_PRELUDE: &str = r#"
interface TSchema { static: unknown }
interface TString extends TSchema { static: string }
interface TObject<T extends Record<string, TSchema>> extends TSchema {
  static: { [K in keyof T]: Static<T[K]> }
}
type Static<T extends TSchema> = T["static"];
"#;

/// `Static<In>` whose expansion is missing a deeply nested property must not be
/// assignable to `Static<Out>` — the variance fast path used to relate them.
#[test]
fn indexed_access_alias_application_rejects_missing_nested_property() {
    let source = format!(
        "{SCHEMA_PRELUDE}
type In = TObject<{{ a: TString; b: TObject<{{ c: TString }}> }}>;
type Out = TObject<{{ a: TString; b: TObject<{{ c: TString; d: TString }}> }}>;
const bad: Static<Out> = null as any as Static<In>;
"
    );
    let codes = error_codes(&source);
    assert_eq!(
        codes,
        vec![2322],
        "Static<In> (missing nested `d`) must not be assignable to Static<Out>; got {codes:?}"
    );
}

/// The opposite direction is sound: a structurally wider `Static<Out>` is
/// assignable to the narrower `Static<In>`, so no diagnostic is reported. This
/// guards against the fix over-rejecting (it must expand structurally, not
/// blanket-reject same-base alias applications).
#[test]
fn indexed_access_alias_application_accepts_wider_source() {
    let source = format!(
        "{SCHEMA_PRELUDE}
type In = TObject<{{ a: TString; b: TObject<{{ c: TString }}> }}>;
type Out = TObject<{{ a: TString; b: TObject<{{ c: TString; d: TString }}> }}>;
const ok: Static<In> = null as any as Static<Out>;
"
    );
    let codes = error_codes(&source);
    assert!(
        codes.is_empty(),
        "structurally wider Static<Out> must be assignable to Static<In>; got {codes:?}"
    );
}

/// Equal arguments still relate (the structural-expansion path must agree with
/// the identity case), so two identical `Static<Same>` are mutually assignable.
#[test]
fn indexed_access_alias_application_accepts_identical_arguments() {
    let source = format!(
        "{SCHEMA_PRELUDE}
type Same = TObject<{{ a: TString; b: TObject<{{ c: TString }}> }}>;
const x: Static<Same> = null as any as Static<Same>;
"
    );
    let codes = error_codes(&source);
    assert!(
        codes.is_empty(),
        "identical Static<Same> must be mutually assignable; got {codes:?}"
    );
}

/// The behavior follows the structural shape, not the `Static`/`TObject`
/// spellings: a renamed indexed-access schema alias rejects the same mismatch.
#[test]
fn indexed_access_alias_application_is_name_agnostic() {
    let source = r#"
interface SchemaBase { kind: unknown }
interface StringSchema extends SchemaBase { kind: string }
interface ObjectSchema<T extends Record<string, SchemaBase>> extends SchemaBase {
  kind: { [K in keyof T]: Resolve<T[K]> }
}
type Resolve<T extends SchemaBase> = T["kind"];
type Narrow = ObjectSchema<{ a: StringSchema }>;
type Wide = ObjectSchema<{ a: StringSchema; b: StringSchema }>;
const bad: Resolve<Wide> = null as any as Resolve<Narrow>;
"#;
    let codes = error_codes(source);
    assert_eq!(
        codes,
        vec![2322],
        "renamed indexed-access schema must reject the missing-property mismatch; got {codes:?}"
    );
}

/// A two-parameter indexed-access alias over an intersection
/// (`Static<T,P> = (T & {params:P})['static']`, the full `TypeBox` `Static`) must
/// still expand structurally for the relation and reject a nested mismatch.
#[test]
fn indexed_access_alias_application_two_param_intersection_rejects_mismatch() {
    let source = r#"
interface TSchema { params: unknown[]; static: unknown }
interface TString extends TSchema { static: string }
interface TObject<T extends Record<string, TSchema>> extends TSchema {
  static: { [K in keyof T]: Static<T[K], []> }
}
type Static<T extends TSchema, P extends unknown[] = []> = (T & { params: P })["static"];
type In = TObject<{ a: TString; b: TObject<{ c: TString }> }>;
type Out = TObject<{ a: TString; b: TObject<{ c: TString; d: TString }> }>;
const bad: Static<Out> = null as any as Static<In>;
"#;
    let codes = error_codes(source);
    assert_eq!(
        codes,
        vec![2322],
        "two-param intersection Static must expand structurally and reject; got {codes:?}"
    );
}
