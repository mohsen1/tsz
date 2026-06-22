//! Tests for homomorphic mapped types whose source instantiates to `never`
//! (issue #14460).
//!
//! `tsc`'s `instantiateMappedType` maps the homomorphic source type variable
//! through `mapType`, and `mapType(never)` is `never`: a homomorphic mapped type
//! `{ [K in keyof T]: ... }` whose source `T` instantiates to `never` reduces to
//! `never`, independent of the template shape (`T[K]` or a constant), of key
//! remapping (`as`), and of added/removed modifiers.
//!
//! Before the fix, the instantiation short-circuit gated on `is_primitive_type`,
//! which deliberately excludes `never`, so a `never` source fell through to the
//! object-expansion path and materialized a malformed
//! `{ [K in keyof never]: never[K] }` shape (an index access into `never`, since
//! `keyof never` is `string | number | symbol`). That shape is not assignable to
//! `never` and surfaced as a false TS2322.
//!
//! Binder names (`T`/`Source`, `K`/`Key`, alias spellings) vary across cases so
//! the behavior tracks the type shape, not a spelling.
use tsz_checker::test_utils::check_source_diagnostics;

/// A homomorphic mapped type over a `never` source must reduce to `never`, so
/// assigning it to a `never` annotation produces no diagnostic.
fn no_errors(source: &str) {
    let diagnostics = check_source_diagnostics(source);
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|d| !matches!(d.code, 2318 | 2304))
        .collect();
    assert!(
        relevant.is_empty(),
        "Expected no diagnostics, got: {relevant:#?}"
    );
}

/// Assert a TS2322 is present — used by negative controls that must NOT reduce a
/// non-`never` homomorphic source to `never`.
fn has_2322(source: &str) {
    let diagnostics = check_source_diagnostics(source);
    assert!(
        diagnostics.iter().any(|d| d.code == 2322),
        "Expected TS2322, got: {diagnostics:#?}"
    );
}

// ---------------------------------------------------------------------------
// Positive: every homomorphic shape over `never` reduces to `never`.
// ---------------------------------------------------------------------------

/// Identity template `{ [K in keyof T]: T[K] }` over `never` → `never`.
#[test]
fn identity_homomorphic_mapped_over_never_is_never() {
    no_errors(
        r#"
type Identity<T> = { [K in keyof T]: T[K] };
const a: never = (null as any as Identity<never>);
"#,
    );
}

/// Constant template `{ [K in keyof T]: number }` (does NOT read `T[K]`) over
/// `never` → `never`. The reduction must not depend on the template shape.
#[test]
fn constant_template_homomorphic_mapped_over_never_is_never() {
    no_errors(
        r#"
type Constant<Source> = { [Key in keyof Source]: number };
const b: never = (null as any as Constant<never>);
"#,
    );
}

/// Non-identity source-index template `{ [K in keyof T]: T[K][] }` over `never`
/// → `never`.
#[test]
fn nonidentity_source_index_homomorphic_mapped_over_never_is_never() {
    no_errors(
        r#"
type Arrayify<T> = { [K in keyof T]: T[K][] };
const c: never = (null as any as Arrayify<never>);
"#,
    );
}

/// Key-remapping (`as`) homomorphic template over `never` → `never`. The
/// `name_type`/`as` clause must not block the reduction.
#[test]
fn remapped_homomorphic_mapped_over_never_is_never() {
    no_errors(
        r#"
type Prefixed<T> = { [K in keyof T as `p${string & K}`]: T[K] };
const d: never = (null as any as Prefixed<never>);
"#,
    );
}

/// Added `readonly` modifier homomorphic template over `never` → `never`.
#[test]
fn readonly_modifier_homomorphic_mapped_over_never_is_never() {
    no_errors(
        r#"
type Frozen<Source> = { readonly [Key in keyof Source]: Source[Key] };
const e: never = (null as any as Frozen<never>);
"#,
    );
}

/// Added optional modifier homomorphic template over `never` → `never`.
#[test]
fn optional_modifier_homomorphic_mapped_over_never_is_never() {
    no_errors(
        r#"
type Loosen<T> = { [K in keyof T]?: T[K] };
const f: never = (null as any as Loosen<never>);
"#,
    );
}

/// The reduction holds when the `never` source arrives through a conditional
/// type (`T extends ... ? ... : never`) — the project-scale shape behind the
/// never-collapse false positives — rather than the bare `never` literal.
#[test]
fn homomorphic_mapped_over_conditional_never_is_never() {
    no_errors(
        r#"
type OnlyObjects<T> = T extends object ? T : never;
type Identity<T> = { [K in keyof T]: T[K] };
const g: never = (null as any as Identity<OnlyObjects<string>>);
"#,
    );
}

// ---------------------------------------------------------------------------
// Negative controls: non-`never` sources must NOT be reduced to `never`.
// ---------------------------------------------------------------------------

/// A homomorphic mapped type over a real object keeps its shape — it is NOT
/// `never`, so assigning it to a `never` annotation still errors (TS2322).
#[test]
fn homomorphic_mapped_over_object_is_not_never() {
    has_2322(
        r#"
type Identity<T> = { [K in keyof T]: T[K] };
const a: never = (null as any as Identity<{ x: number }>);
"#,
    );
}

/// A homomorphic mapped type over a real object remains usable: its properties
/// survive (no false reduction stripped them).
#[test]
fn homomorphic_mapped_over_object_preserves_properties() {
    no_errors(
        r#"
type Identity<Source> = { [Key in keyof Source]: Source[Key] };
const a: Identity<{ x: number; y: string }> = { x: 1, y: "s" };
"#,
    );
}

/// A non-homomorphic mapped type `{ [P in K]: V }` with an empty (`never`) key
/// union is `{}`, NOT `never`: an empty object literal is assignable, but a
/// `never` annotation is not (TS2322), so the bottom-collapse must not leak here.
#[test]
fn nonhomomorphic_mapped_over_never_key_union_is_empty_object_not_never() {
    no_errors(
        r#"
type FromKeys<K extends PropertyKey> = { [P in K]: number };
const a: FromKeys<never> = {};
"#,
    );
    has_2322(
        r#"
type FromKeys<K extends PropertyKey> = { [P in K]: number };
const b: never = (null as any as FromKeys<never>);
"#,
    );
}
