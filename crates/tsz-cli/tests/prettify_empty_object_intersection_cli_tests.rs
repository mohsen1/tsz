//! Empty-object removal in intersections whose co-member is a *mapped type*
//! (the `Prettify`/`Simplify` idiom `{ [K in keyof T]: T[K] } & {}`).
//!
//! Structural rule: when an intersection contains the empty anonymous object
//! type `{}` and at least one other constituent is an object type, tsc drops
//! the redundant `{}` (`getIntersectionType`: `IncludesEmptyObject &&
//! TypeFlags.Object`). A mapped type always evaluates to an object type and can
//! never be `null`/`undefined`, so `{ [K in keyof T]: T[K] } & {}` reduces to
//! the homomorphic mapped identity alone. A generic source `T` then relates to
//! that identity (`T <: { [K in keyof T]: T[K] }`) instead of being forced
//! through the impossible `T <: {}` (an unconstrained `T` may be nullish), so
//! the canonical `Prettify<T>` wrapper no longer raises a false TS2322.
//!
//! Owner: solver type construction (`TypeInterner::normalize_intersection`'s
//! empty-object rule), which previously classified a `Mapped` member as not
//! non-nullish and therefore kept the redundant `{}`.
//!
//! Cases vary the mapped modifiers, key remapping, member order, and binder
//! spellings, and pin the negatives that must stay errors: `T & {}` (a bare
//! type parameter is not an object type, so `{}` survives), `Mapped & { z?: 1 }`
//! (the co-member is not the *empty* object), and the `string & {}` literal
//! idiom (a primitive co-member keeps `{}`), so the rule follows the type shape
//! rather than the `& {}` syntax.

use crate::args::CliArgs;
use clap::Parser;
use tsz_checker::diagnostics::Diagnostic;

/// Compile a single-file `source` with `--strict` and return its diagnostics.
fn compile_source(source: &str) -> Vec<Diagnostic> {
    let dir = tempfile::tempdir().expect("temp dir");
    std::fs::write(dir.path().join("main.ts"), source).expect("write repro file");

    let argv = vec![
        "tsz",
        "--ignoreConfig",
        "--noEmit",
        "--strict",
        "--target",
        "es2022",
        "--lib",
        "es2022",
        "main.ts",
    ];
    let args = CliArgs::try_parse_from(argv).expect("parse args");
    crate::driver::compile(&args, dir.path())
        .expect("compile should succeed")
        .diagnostics
}

fn assignability_errors(diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .filter(|d| d.code == 2322 || d.code == 2345)
        .map(|d| d.message_text.clone())
        .collect()
}

/// The canonical `Prettify<T>` return position: `T` must relate to
/// `{ [K in keyof T]: T[K] } & {}` exactly as it does to the mapped identity.
#[test]
fn generic_source_relates_to_prettify_mapped_intersection_empty_object() {
    let errs = assignability_errors(&compile_source(
        r#"
type Prettify<T> = { [K in keyof T]: T[K] } & {};
function id<T>(x: T): Prettify<T> { return x; }
"#,
    ));
    assert!(
        errs.is_empty(),
        "T should be assignable to {{ [K in keyof T]: T[K] }} & {{}}, got: {errs:?}"
    );
}

/// Inline (un-aliased) form, both member orders, plus `readonly`/`?` modifiers:
/// the empty object is redundant regardless of mapped shape or member order.
#[test]
fn mapped_intersection_empty_object_inline_and_reversed_and_modified() {
    // Inline `Mapped & {}`.
    assert!(
        assignability_errors(&compile_source(
            "function f<U>(x: U): { [P in keyof U]: U[P] } & {} { return x; }",
        ))
        .is_empty(),
        "inline mapped & {{}} must relate"
    );
    // Reversed order `{} & Mapped`.
    assert!(
        assignability_errors(&compile_source(
            "function f<U>(x: U): {} & { [P in keyof U]: U[P] } { return x; }",
        ))
        .is_empty(),
        "{{}} & mapped (reversed) must relate"
    );
    // Two empty objects collapse fully.
    assert!(
        assignability_errors(&compile_source(
            "function f<U>(x: U): { [P in keyof U]: U[P] } & {} & {} { return x; }",
        ))
        .is_empty(),
        "mapped & {{}} & {{}} must relate"
    );
    // Homomorphic modifiers preserved; `{}` still redundant.
    assert!(
        assignability_errors(&compile_source(
            "function f<U>(x: { readonly [P in keyof U]?: U[P] }): \
             { readonly [P in keyof U]?: U[P] } & {} { return x; }",
        ))
        .is_empty(),
        "readonly/optional mapped & {{}} must relate"
    );
}

/// Real-world `Merge` via `Prettify` over a concrete intersection still resolves
/// its members (the empty object does not block the merged shape).
#[test]
fn prettify_merge_over_concrete_intersection_resolves_members() {
    let errs = assignability_errors(&compile_source(
        r#"
type Prettify<T> = { [K in keyof T]: T[K] } & {};
type Merge<A, B> = Prettify<Omit<A, keyof B> & B>;
type R = Merge<{ a: number; b: string }, { b: number; c: boolean }>;
const v: { a: number; b: number; c: boolean } = { a: 1, b: 2, c: true } as R;
"#,
    ));
    assert!(
        errs.is_empty(),
        "Prettify merge must resolve members, got: {errs:?}"
    );
}

/// Anti-hardcoding: the rule follows the mapped/empty-object shape, not the
/// alias or type-parameter spellings.
#[test]
fn prettify_renamed_binders_still_relate() {
    let errs = assignability_errors(&compile_source(
        r#"
type Smoosh<Shape> = { [Key in keyof Shape]: Shape[Key] } & {};
function widget<Widget>(thing: Widget): Smoosh<Widget> { return thing; }
"#,
    ));
    assert!(
        errs.is_empty(),
        "renamed binders must still relate, got: {errs:?}"
    );
}

/// Negative control 1: a bare type parameter is NOT an object type, so `{}`
/// survives `T & {}` and an unconstrained (possibly-nullish) `T` is not
/// assignable to it — TS2322 must remain.
#[test]
fn bare_type_param_intersect_empty_object_still_errors() {
    let errs = assignability_errors(&compile_source("function f<T>(x: T): T & {} { return x; }"));
    assert!(
        !errs.is_empty(),
        "T & {{}} must keep {{}} and still report TS2322 for an unconstrained T"
    );
}

/// Negative control 2: the co-member is a *non-empty* object, so the
/// empty-object rule does not apply and the relation stays an error.
#[test]
fn mapped_intersect_non_empty_object_still_errors() {
    let errs = assignability_errors(&compile_source(
        "function f<T>(x: T): { [K in keyof T]: T[K] } & { z?: 1 } { return x; }",
    ));
    assert!(
        !errs.is_empty(),
        "mapped & {{ z?: 1 }} is not the empty-object case and must still error"
    );
}

/// Negative control 3: the `string & {}` literal-preservation idiom must NOT be
/// collapsed — a primitive co-member is not an object type, so `{}` survives and
/// the union keeps its literal members.
#[test]
fn string_and_empty_object_idiom_preserved() {
    // No diagnostics: the idiom is valid, and the literal stays a member.
    let errs = assignability_errors(&compile_source(
        r#"
type Loose<T extends string> = T | (string & {});
const x: Loose<"a" | "b"> = "anything";
const y: "a" | "b" | (string & {}) = "z";
"#,
    ));
    assert!(
        errs.is_empty(),
        "string & {{}} idiom must compile, got: {errs:?}"
    );
}
