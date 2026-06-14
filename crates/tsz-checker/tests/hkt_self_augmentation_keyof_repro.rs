//! Repro + adjacent matrix for #13509: a cross-file `declare module` interface
//! augmentation must be folded into the interface BEFORE `keyof` / constraint
//! satisfaction, even when the interface is referenced within its own declaring
//! file (the fp-ts higher-kinded-types pattern).
//!
//! Structural rule: when an exported interface `I` declared in module `M` is
//! augmented via `declare module "M" { interface I { ... } }` from any file,
//! `keyof I` (and constraint satisfaction against it) must include every merged
//! member. Gated on export-ness so non-exported file-local interfaces keep their
//! self-module augmentation isolated (#6164).

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_multi_file_with_global_index;

fn diagnostics(files: &[(&str, &str)], entry: &str) -> Vec<(u32, String)> {
    check_multi_file_with_global_index(
        files,
        entry,
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

/// Core fp-ts HKT witness: `Kind<URI, A>` where `URI = "Array"` is registered
/// into the central `URItoKind` interface via a cross-file `declare module`.
#[test]
fn fp_ts_hkt_keyof_sees_cross_file_self_augmentation() {
    let diags = diagnostics(
        &[
            (
                "HKT.ts",
                r#"
export interface URItoKind<A> {}
export type URIS = keyof URItoKind<unknown>;
export type Kind<U extends URIS, A> = URItoKind<A>[U];
"#,
            ),
            (
                "Array.ts",
                r#"
import { Kind } from "./HKT";
declare module "./HKT" {
    interface URItoKind<A> {
        readonly Array: Array<A>;
    }
}
export const URI = "Array";
export type URI = typeof URI;
export const of = <A>(a: A): Kind<URI, A> => [a];
"#,
            ),
        ],
        "Array.ts",
    );

    assert_eq!(
        count_code(&diags, 2344),
        0,
        "unexpected TS2344 (keyof missed cross-file self-augmentation); got {diags:#?}"
    );
}

/// Anti-hardcoding: the rule is structural, not name-driven. Every binder is
/// renamed (module, registry interface, key, alias) and the behavior holds.
#[test]
fn keyof_self_augmentation_rule_is_binder_name_independent() {
    let diags = diagnostics(
        &[
            (
                "registry.ts",
                r#"
export interface Slots<T> {}
export type Tags = keyof Slots<unknown>;
export type Pick2<K extends Tags, T> = Slots<T>[K];
"#,
            ),
            (
                "widget.ts",
                r#"
import { Pick2 } from "./registry";
declare module "./registry" {
    interface Slots<T> {
        readonly Widget: ReadonlyArray<T>;
    }
}
export const TAG = "Widget";
export type TAG = typeof TAG;
export const wrap = <T>(t: T): Pick2<TAG, T> => [t];
"#,
            ),
        ],
        "widget.ts",
    );

    assert_eq!(
        count_code(&diags, 2344),
        0,
        "renamed-binder HKT registry should also see the augmented key; got {diags:#?}"
    );
}

/// Two type-parameter registry (`URItoKind2<E, A>`): the 2-ary form must merge
/// the same way.
#[test]
fn keyof_self_augmentation_two_arity_registry() {
    let diags = diagnostics(
        &[
            (
                "HKT.ts",
                r#"
export interface URItoKind2<E, A> {}
export type URIS2 = keyof URItoKind2<unknown, unknown>;
export type Kind2<U extends URIS2, E, A> = URItoKind2<E, A>[U];
"#,
            ),
            (
                "Either.ts",
                r#"
import { Kind2 } from "./HKT";
declare module "./HKT" {
    interface URItoKind2<E, A> {
        readonly Either: { left: E; right: A };
    }
}
export const URI = "Either";
export type URI = typeof URI;
export const right = <E, A>(a: A): Kind2<URI, E, A> => ({ left: undefined as any, right: a });
"#,
            ),
        ],
        "Either.ts",
    );

    assert_eq!(
        count_code(&diags, 2344),
        0,
        "2-ary HKT registry should see the augmented key; got {diags:#?}"
    );
}

/// Multiple sibling files registering distinct URIs all land in `keyof`.
#[test]
fn keyof_self_augmentation_accumulates_across_many_files() {
    let diags = diagnostics(
        &[
            (
                "HKT.ts",
                r#"
export interface URItoKind<A> {}
export type URIS = keyof URItoKind<unknown>;
export type Kind<U extends URIS, A> = URItoKind<A>[U];
"#,
            ),
            (
                "Array.ts",
                r#"
import { Kind } from "./HKT";
declare module "./HKT" { interface URItoKind<A> { readonly Array: Array<A>; } }
export const URI = "Array";
export type URI = typeof URI;
export const ofA = <A>(a: A): Kind<URI, A> => [a];
"#,
            ),
            (
                "Option.ts",
                r#"
import { Kind } from "./HKT";
declare module "./HKT" { interface URItoKind<A> { readonly Option: { value: A }; } }
export const URI = "Option";
export type URI = typeof URI;
export const ofO = <A>(a: A): Kind<URI, A> => ({ value: a });
"#,
            ),
        ],
        "Option.ts",
    );

    assert_eq!(
        count_code(&diags, 2344),
        0,
        "every sibling-registered URI should satisfy the keyof constraint; got {diags:#?}"
    );
}

/// Negative control (#6164): a NON-exported file-local interface that is
/// self-augmented via `declare module "./self"` must NOT absorb the augmented
/// member, so an object literal carrying that member is still an excess property
/// (TS2353). Guards against over-merging.
#[test]
fn non_exported_self_augmented_interface_does_not_merge() {
    let diags = diagnostics(
        &[(
            "test.ts",
            r#"
interface Local {
    a: number;
}

declare module "./test" {
    interface Local {
        b: string;
    }
}

const v: Local = { a: 1, b: "x" };
export {};
"#,
        )],
        "test.ts",
    );

    assert_eq!(
        count_code(&diags, 2353),
        1,
        "non-exported local interface must keep its self-augmentation isolated; got {diags:#?}"
    );
}
