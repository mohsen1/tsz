//! Repro + adjacent matrix for #13653: a cross-file `declare module` interface
//! augmentation must be folded into the interface's materialized structural type
//! at every use site — `keyof`, indexed access, AND structural assignability —
//! not only the same-module path covered by #13509.
//!
//! Structural rule: when an exported interface `I` declared in home file `H` is
//! augmented from a different file via `declare module "./H" { interface I { ... } }`,
//! `keyof I`, `I[K]`, and assignability against `I` must all observe the merged
//! members. The fix folds augmentations into the interface body at the canonical
//! `get_type_of_symbol` resolution point so every shared def-store / type-env
//! cache observes the same augmented body across files.
//!
//! These use `check_all_multi_file_with_global_index`, which checks every file
//! through one shared definition store — the multi-file path where an
//! un-augmented body registered while checking a consumer would otherwise shadow
//! the augmented body (the regression this guards).

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

/// Core fp-ts HKT witness: a 1-ary registry augmented cross-file, consumed via a
/// conditional `Kind` that indexes the registry.
#[test]
fn cross_file_augmentation_seen_by_keyof_and_indexed_access() {
    let diags = diagnostics(&[
        (
            "HKT.ts",
            r#"
export interface URItoKind<A> {}
export type URIS = keyof URItoKind<unknown>;
export type Kind<U extends URIS, A> = U extends URIS ? URItoKind<A>[U] : never;
"#,
        ),
        (
            "Array.ts",
            r#"
import { Kind, URItoKind } from "./HKT";
declare module "./HKT" {
    interface URItoKind<A> {
        readonly MyArray: ReadonlyArray<A>;
    }
}
export type MyArr = Kind<"MyArray", number>;
"#,
        ),
    ]);

    assert_eq!(
        count_code(&diags, 2344),
        0,
        "cross-file augmented key missed by keyof/indexed-access; got {diags:#?}"
    );
}

// Note: the computed-`[URI]`-key augmentation form (the faithful fp-ts `typeof
// URI` tag) is exercised end-to-end via the CLI / fp-ts project-compile-guard
// (see PR Verification) and unit-tested at the name-resolution layer in
// `crates/tsz-checker/src/types/module_augmentation.rs`
// (`augmentation_member_key_name_in_arena`). It is omitted from this lib-less
// multi-file harness, which cannot materialize the lib-backed computed-key
// member shape the real driver resolves.

/// Anti-hardcoding: rename every binder (module, registry interface, key, alias);
/// the rule is structural, not name-driven.
#[test]
fn cross_file_augmentation_is_binder_name_independent() {
    let diags = diagnostics(&[
        (
            "registry.ts",
            r#"
export interface Slots<T> {}
export type Tags = keyof Slots<unknown>;
export type Lookup<K extends Tags, T> = K extends Tags ? Slots<T>[K] : never;
"#,
        ),
        (
            "widget.ts",
            r#"
import { Lookup, Slots } from "./registry";
declare module "./registry" {
    interface Slots<T> {
        readonly Widget: ReadonlyArray<T>;
    }
}
export type W = Lookup<"Widget", number>;
"#,
        ),
    ]);

    assert_eq!(
        count_code(&diags, 2344),
        0,
        "renamed-binder registry should also see the augmented key; got {diags:#?}"
    );
}

/// Augmentation declared in a THIRD file: home declares, a sibling augments, a
/// consumer (neither home nor augmenter) reads the augmented key.
#[test]
fn cross_file_augmentation_visible_from_third_consumer_file() {
    let diags = diagnostics(&[
        (
            "HKT.ts",
            r#"
export interface URItoKind<A> {}
export type URIS = keyof URItoKind<unknown>;
export type Kind<U extends URIS, A> = U extends URIS ? URItoKind<A>[U] : never;
"#,
        ),
        (
            "Array.ts",
            r#"
import { URItoKind } from "./HKT";
declare module "./HKT" {
    interface URItoKind<A> {
        readonly MyArray: ReadonlyArray<A>;
    }
}
export {};
"#,
        ),
        (
            "use.ts",
            r#"
import { Kind } from "./HKT";
import "./Array";
export type MyArr = Kind<"MyArray", number>;
"#,
        ),
    ]);

    assert_eq!(
        count_code(&diags, 2344),
        0,
        "third-file consumer should see the augmentation; got {diags:#?}"
    );
}

/// Direct literal indexed access bypasses the normal `resolve_lazy` call chain
/// in a few syntax/diagnostic paths. With body publication enabled, those reads
/// must still observe the augmented registry body instead of the empty base.
#[test]
#[ignore = "requires TSZ_MODULE_AUG_BODY_PUBLISH=1"]
fn direct_literal_indexed_access_observes_augmented_registry_body() {
    let diags = diagnostics(&[
        (
            "HKT.ts",
            r#"
export interface URItoKind<A> {}
"#,
        ),
        (
            "Array.ts",
            r#"
import { URItoKind } from "./HKT";
declare module "./HKT" {
    interface URItoKind<A> {
        readonly MyArray: ReadonlyArray<A>;
    }
}
export const wrong: URItoKind<number>["MyArray"] = ["x"];
"#,
        ),
    ]);

    assert_eq!(
        count_code(&diags, 2339) + count_code(&diags, 2536),
        0,
        "direct indexed access should see the augmented key; got {diags:#?}"
    );
    assert_eq!(
        count_code(&diags, 2322),
        1,
        "augmented body should preserve the ReadonlyArray<number> element type; got {diags:#?}"
    );
}

/// Same direct-read path, but with the registry hidden behind an alias. This
/// guards alias-body probes that inspect a `Lazy(DefId)` body directly.
#[test]
#[ignore = "requires TSZ_MODULE_AUG_BODY_PUBLISH=1"]
fn alias_wrapped_indexed_access_observes_augmented_registry_body() {
    let diags = diagnostics(&[
        (
            "registry.ts",
            r#"
export interface Slots<T> {}
export type Registry<T> = Slots<T>;
"#,
        ),
        (
            "widget.ts",
            r#"
import { Registry, Slots } from "./registry";
declare module "./registry" {
    interface Slots<T> {
        readonly Widget: ReadonlyArray<T>;
    }
}
export const wrong: Registry<number>["Widget"] = ["x"];
"#,
        ),
    ]);

    assert_eq!(
        count_code(&diags, 2339) + count_code(&diags, 2536),
        0,
        "alias-wrapped indexed access should see the augmented key; got {diags:#?}"
    );
    assert_eq!(
        count_code(&diags, 2322),
        1,
        "alias-wrapped augmented body should preserve the element type; got {diags:#?}"
    );
}

/// Assignability alias probes can inspect a `Lazy(DefId)` body directly before
/// relation fallback. With body publication enabled, that probe must see the
/// augmented registry body so the missing augmented member is still required.
#[test]
#[ignore = "requires TSZ_MODULE_AUG_BODY_PUBLISH=1"]
fn alias_assignability_observes_augmented_registry_body() {
    let diags = diagnostics(&[
        (
            "registry.ts",
            r#"
export interface Slots<T> {}
export type Registry<T> = Slots<T>;
"#,
        ),
        (
            "widget.ts",
            r#"
import { Registry, Slots } from "./registry";
declare module "./registry" {
    interface Slots<T> {
        readonly Widget: ReadonlyArray<T>;
    }
}
export const r: Registry<number> = {};
"#,
        ),
    ]);

    assert_eq!(
        count_code(&diags, 2741),
        1,
        "alias assignability should require the augmented member; got {diags:#?}"
    );
}

/// Structural assignability: the cross-file augmented member is REQUIRED, so an
/// empty object literal assigned to the interface is TS2741 (missing member).
/// tsz previously accepted this (the augmented member was absent from the
/// materialized structural type cross-file).
#[test]
fn cross_file_augmentation_member_is_required_for_assignability() {
    let diags = diagnostics(&[
        (
            "HKT.ts",
            r#"
export interface URItoKind<A> {}
"#,
        ),
        (
            "Array.ts",
            r#"
import { URItoKind } from "./HKT";
declare module "./HKT" {
    interface URItoKind<A> {
        readonly MyArray: ReadonlyArray<A>;
    }
}
export const r: URItoKind<number> = {};
"#,
        ),
    ]);

    assert_eq!(
        count_code(&diags, 2741),
        1,
        "cross-file augmented member should be required (TS2741); got {diags:#?}"
    );
}

/// Negative control (#6164): a NON-exported file-local interface that is
/// self-augmented via `declare module "./self"` must NOT absorb the augmented
/// member, so an object literal carrying that member stays an excess property
/// (TS2353). Guards against over-merging.
#[test]
fn non_exported_self_augmented_interface_does_not_merge() {
    let diags = diagnostics(&[(
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
    )]);

    assert_eq!(
        count_code(&diags, 2353),
        1,
        "non-exported local interface must keep its self-augmentation isolated; got {diags:#?}"
    );
}
