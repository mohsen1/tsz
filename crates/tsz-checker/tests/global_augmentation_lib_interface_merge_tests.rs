//! Regressions for `declare global { interface X { ... } }` (or top-level
//! script-mode `interface X { ... }`) that augments a lib interface.
//!
//! Before the fix, the type-position resolution of `X` went through
//! `delegate_cross_arena_interface_type`, which uses the lib's binder for
//! declaration lookup. The lib binder does not see user-side augmentation
//! declarations, so the returned `TypeId` was the lib-only interface body.
//! Value-position property access already consulted
//! `binder.global_augmentations` independently (via
//! `resolve_object_type_global_augmentation`), but type-position accesses
//! such as `keyof X` and `X[K]` only see the merged shape through the
//! `TypeId` returned by lazy resolution.
//!
//! After the fix, `resolve_lib_type_by_name` (and
//! `resolve_lib_type_with_params`) register the augmentation-merged body
//! into `definition_store` and the type environment, so all lazy
//! resolutions return the augmented interface.

use tsz_checker::context::CheckerOptions;
use tsz_common::common::ModuleKind;

fn check_with_dom(source: &str) -> Vec<(u32, String)> {
    let libs = tsz_checker::test_utils::load_default_lib_files();
    tsz_checker::test_utils::check_multi_file_with_libs(
        &[("test.ts", source)],
        "test.ts",
        CheckerOptions {
            module: ModuleKind::CommonJS,
            strict: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
        &libs,
    )
    .into_iter()
    .filter(|d| d.code != 2318)
    .map(|d| (d.code, d.message_text))
    .collect()
}

/// `declare global` augmentation that adds only an index signature must
/// still preserve the lib interface's named property keys for type-position
/// indexed access (`X["div"]`).
#[test]
fn declare_global_index_sig_augment_preserves_lib_property_indexing() {
    let source = r#"
declare global {
    interface ElementTagNameMap {
        [index: number]: HTMLElement;
    }
}
type DivType = ElementTagNameMap["div"];
declare const div: DivType;
const _check: HTMLDivElement = div;
export {};
"#;
    let diags = check_with_dom(source);
    let bad_2536: Vec<_> = diags
        .iter()
        .filter(|(code, msg)| {
            *code == 2536 && msg.contains("\"div\"") && msg.contains("ElementTagNameMap")
        })
        .collect();
    assert!(
        bad_2536.is_empty(),
        "expected `\"div\"` to remain a valid index of `ElementTagNameMap` after global augmentation; got: {diags:?}"
    );
    let bad_2322: Vec<_> = diags.iter().filter(|(code, _)| *code == 2322).collect();
    assert!(
        bad_2322.is_empty(),
        "expected `ElementTagNameMap[\"div\"]` to resolve to `HTMLDivElement` after global augmentation; got: {diags:?}"
    );
}

/// `keyof X` over an augmented lib interface must include both the lib's
/// property keys and the augmented index-signature kind.
#[test]
fn declare_global_index_sig_augment_keyof_includes_lib_keys() {
    let source = r#"
declare global {
    interface ElementTagNameMap {
        [index: number]: HTMLElement;
    }
}
type K = keyof ElementTagNameMap;
// Lib-defined string key must be assignable to K.
const a: K = "div";
// User-side index-signature kind (number) must be assignable to K too.
const b: K = 5;
export {};
"#;
    let diags = check_with_dom(source);
    let assign_errors: Vec<_> = diags.iter().filter(|(code, _)| *code == 2322).collect();
    assert!(
        assign_errors.is_empty(),
        "expected `keyof ElementTagNameMap` to include both lib property keys and the user index-signature kind; got: {diags:?}"
    );
}

/// Variant: augmentation adds both a property and an index signature.
/// The merged shape must accept indexing by lib property keys, user
/// property keys, and the user's index-signature kind without TS2536.
#[test]
fn declare_global_property_and_index_sig_augment_all_keys_valid() {
    let source = r#"
declare global {
    interface ElementTagNameMap {
        [index: number]: HTMLElement;
        myCustom: HTMLElement;
    }
}
type T1 = ElementTagNameMap["div"];      // lib property key
type T2 = ElementTagNameMap["myCustom"]; // user property key
type T3 = ElementTagNameMap[5];          // user index-signature kind
declare const d: T1;
declare const c: T2;
declare const n: T3;
export {};
"#;
    let diags = check_with_dom(source);
    let bad_2536: Vec<_> = diags.iter().filter(|(code, _)| *code == 2536).collect();
    assert!(
        bad_2536.is_empty(),
        "expected merged ElementTagNameMap to expose lib + user + index-sig key spaces without TS2536; got: {diags:?}"
    );
}

/// Augmenting a different lib interface (`HTMLElement`) with an index
/// signature must not regress type-position access on the lib's existing
/// members, and the augmented index signature must be reachable.
#[test]
fn declare_global_html_element_augment_preserves_lib_members() {
    let source = r#"
declare global {
    interface HTMLElement {
        [index: number]: HTMLElement;
    }
}
declare const el: HTMLElement;
const tag: string = el.tagName;     // lib member
const child: HTMLElement = el[0];   // augmented index signature
export {};
"#;
    let diags = check_with_dom(source);
    let bad: Vec<_> = diags
        .iter()
        .filter(|(code, _)| *code == 2536 || *code == 2322 || *code == 2339)
        .collect();
    assert!(
        bad.is_empty(),
        "expected augmented HTMLElement to retain `tagName` and gain numeric index access; got: {diags:?}"
    );
}

/// Top-level script-mode augmentation (no `declare global`) of a lib
/// interface must apply the same merge to type-position accesses. tsc
/// allows scripts to augment built-in globals directly.
#[test]
fn script_mode_augment_property_preserves_lib_property_indexing() {
    let source = r#"
interface ElementTagNameMap {
    myCustom: HTMLElement;
}
type T1 = ElementTagNameMap["div"];      // lib property key
type T2 = ElementTagNameMap["myCustom"]; // user property key
declare const d: T1;
declare const c: T2;
"#;
    let diags = check_with_dom(source);
    let bad_2536: Vec<_> = diags.iter().filter(|(code, _)| *code == 2536).collect();
    assert!(
        bad_2536.is_empty(),
        "expected script-mode augmentation to merge with lib members for type-position access without TS2536; got: {diags:?}"
    );
}

/// Negative case: the `string === T` comparison used to fire TS2367
/// because `keyof ElementTagNameMap` was reduced to only the user's
/// index-signature key kind (`number`), so `string === T` looked like a
/// disjoint comparison. After the merge fix, T extends a union that
/// includes string-literal keys from the lib, so tsc does not consider
/// the comparison unintentional.
#[test]
fn declare_global_augment_does_not_emit_ts2367_for_keyof_constrained_param() {
    let source = r#"
declare global {
    interface ElementTagNameMap {
        [index: number]: HTMLElement;
    }
}
export function tagEquals<T extends keyof ElementTagNameMap>(t: T): boolean {
    const lower: string = "div";
    return lower === t;
}
"#;
    let diags = check_with_dom(source);
    let unintentional_comparisons: Vec<_> =
        diags.iter().filter(|(code, _)| *code == 2367).collect();
    assert!(
        unintentional_comparisons.is_empty(),
        "expected no TS2367 for `string === T` where T extends keyof augmented ElementTagNameMap; got: {diags:?}"
    );
}
