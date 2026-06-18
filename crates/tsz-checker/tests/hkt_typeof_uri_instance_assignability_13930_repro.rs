//! Repro + adjacent matrix for #13930: the fp-ts higher-kinded-types tag idiom
//! `const URI = "..."; type URI = typeof URI` must resolve `typeof URI` to the
//! VALUE-space literal everywhere a `TypeQuery(SymbolRef)` is resolved by the
//! solver — including the deferred indexed-access `URItoKind*<...>[URI]` that
//! `Kind`/`Kind2` expand to during relation checking — not to the cyclic
//! type-alias body (which collapses to `undefined`).
//!
//! Structural rule: when a single symbol carries BOTH a value declaration and a
//! type-space declaration (interface OR type alias) sharing its `SymbolRef`,
//! `typeof <symbol>` is value-space and must resolve to the value declaration's
//! type. Before this fix only the interface+value merge registered the
//! value-space type for `resolve_type_query`; the type-alias+value merge (the
//! fp-ts `URI` tag) was skipped, so `Kind2<URI, E, A>` evaluated to `undefined`
//! and a concrete instance object produced a false `TS2322` against the
//! HKT-typed interface (e.g. `Filterable2C`/`Partition1`).

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_all_multi_file_with_global_index;

fn diagnostics(files: &[(&str, &str)]) -> Vec<(u32, String)> {
    check_all_multi_file_with_global_index(
        files,
        CheckerOptions {
            strict: true,
            ..Default::default()
        },
    )
    .into_iter()
    .map(|d| (d.code, d.message_text))
    .collect()
}

fn count_code(diags: &[(u32, String)], expected: u32) -> usize {
    diags.iter().filter(|(code, _)| *code == expected).count()
}

/// Core #13930 witness: a concrete instance object assigned to an interface
/// whose generic method signatures are typed in `Kind2<F, E, _>` where `F` is
/// bound through the `typeof URI` tag. tsc accepts; tsz emitted a false TS2322
/// because `Kind2<typeof URI, E, A>` resolved to `undefined`.
#[test]
fn hkt_instance_assignable_through_typeof_uri_tag() {
    let diags = diagnostics(&[
        (
            "HKT.ts",
            r#"
export interface URItoKind2<E, A> {}
export type URIS2 = keyof URItoKind2<any, any>;
export type Kind2<F extends URIS2, E, A> = F extends URIS2 ? URItoKind2<E, A>[F] : never;
export interface Functor2<F extends URIS2> {
    readonly map: <E, A, B>(fa: Kind2<F, E, A>, f: (a: A) => B) => Kind2<F, E, B>;
}
"#,
        ),
        (
            "IOEither.ts",
            r#"
import { Functor2, URItoKind2 } from "./HKT";
declare module "./HKT" {
    interface URItoKind2<E, A> {
        readonly IOEither: IOEither<E, A>;
    }
}
export interface IOEither<E, A> { (): E | A }
export const URI = "IOEither";
export type URI = typeof URI;

declare const ioMap: <E, A, B>(fa: IOEither<E, A>, f: (a: A) => B) => IOEither<E, B>;

export const Functor: Functor2<URI> = {
    map: ioMap,
};
"#,
        ),
    ]);

    assert_eq!(
        count_code(&diags, 2322),
        0,
        "HKT instance through `typeof URI` tag should be assignable; got {diags:#?}"
    );
}

/// Anti-hardcoding: rename every binder (registry interface, tag value/type,
/// alias, module). The rule is structural — a value+type-alias name collision
/// resolved value-space for `typeof` — not name-driven.
#[test]
fn hkt_instance_assignability_is_binder_name_independent() {
    let diags = diagnostics(&[
        (
            "registry.ts",
            r#"
export interface Slots2<E, A> {}
export type Tags2 = keyof Slots2<any, any>;
export type Pick2<G extends Tags2, E, A> = G extends Tags2 ? Slots2<E, A>[G] : never;
export interface Mapper2<G extends Tags2> {
    readonly transform: <E, A, B>(fa: Pick2<G, E, A>, f: (a: A) => B) => Pick2<G, E, B>;
}
"#,
        ),
        (
            "widget.ts",
            r#"
import { Mapper2, Slots2 } from "./registry";
declare module "./registry" {
    interface Slots2<E, A> {
        readonly Widget: Widget<E, A>;
    }
}
export interface Widget<E, A> { (): E | A }
export const TAG = "Widget";
export type TAG = typeof TAG;

declare const widgetMap: <E, A, B>(fa: Widget<E, A>, f: (a: A) => B) => Widget<E, B>;

export const Mapper: Mapper2<TAG> = {
    transform: widgetMap,
};
"#,
        ),
    ]);

    assert_eq!(
        count_code(&diags, 2322),
        0,
        "renamed-binder HKT registry instance should also be assignable; got {diags:#?}"
    );
}

/// `typeof URI` used as a direct indexed-access index resolves to the
/// value-space literal, so `URItoKind2<E, A>[typeof URI]` finds the augmented
/// member rather than `undefined`. Exercises the resolution gap independently
/// of the `Kind2` conditional wrapper.
#[test]
fn typeof_uri_tag_indexes_augmented_registry_member() {
    let diags = diagnostics(&[
        (
            "HKT.ts",
            r#"
export interface URItoKind2<E, A> {}
"#,
        ),
        (
            "Either.ts",
            r#"
import { URItoKind2 } from "./HKT";
declare module "./HKT" {
    interface URItoKind2<E, A> {
        readonly Either: { left: E; right: A };
    }
}
export const URI = "Either";
export type URI = typeof URI;

// `URItoKind2<string, number>[URI]` must resolve to `{ left: string; right: number }`.
export const cell: URItoKind2<string, number>[URI] = { left: "e", right: 1 };
"#,
        ),
    ]);

    assert_eq!(
        count_code(&diags, 2322),
        0,
        "augmented member indexed by the `typeof URI` tag should accept the value; got {diags:#?}"
    );
    assert_eq!(
        count_code(&diags, 2741),
        0,
        "no spurious missing-property diagnostic; got {diags:#?}"
    );
}

/// Negative control: the `typeof URI` tag still narrows to the exact literal, so
/// a non-matching literal assigned to `type URI = typeof URI` stays a TS2322.
/// Guards against the fix widening the value-space type to `string`.
#[test]
fn typeof_uri_tag_preserves_literal_narrowing() {
    let diags = diagnostics(&[(
        "tag.ts",
        r#"
const URI = "IOEither";
type URI = typeof URI;
const ok: URI = "IOEither";
const bad: URI = "Option";
export {};
"#,
    )]);

    assert_eq!(
        count_code(&diags, 2322),
        1,
        "the tag must keep its exact literal type (one mismatch); got {diags:#?}"
    );
}
