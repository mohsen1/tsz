//! Repro + adjacent matrix for the `declare global` analogue of #13509/#13653:
//! interface declarations spread across multiple `declare global { ... }` blocks
//! (in one file, or across files) must be folded into the global interface
//! BEFORE type-level operations — `keyof X`, indexed access `X[K]`,
//! assignability — observe it, not only value-position member access.
//!
//! Structural rule: when a global interface `X` is declared by more than one
//! `declare global { interface X { ... } }` block, every type-level consumer of
//! `X` must see the union of all blocks' members. Each block binds a SEPARATE
//! symbol (the binder restores the boundary scope between blocks so they cannot
//! shadow lib globals), so a bare type reference resolves only one partial
//! symbol unless the global augmentations are folded back in at the canonical
//! symbol-type resolution point (`apply_self_global_augmentations`).

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

/// Two `declare global { interface Registry }` blocks in one file: `keyof` and
/// indexed access must see both members, and only a genuinely-absent key errors.
#[test]
fn two_global_blocks_merge_into_keyof_and_indexed_access() {
    let diags = diagnostics(
        &[(
            "main.ts",
            r#"
declare global { interface Registry { a: number } }
declare global { interface Registry { b: string } }
type Va = Registry["a"];
type Vb = Registry["b"];
const va: Va = 1;
const vb: Vb = "x";
type K = keyof Registry;
const k1: K = "a";
const k2: K = "b";
const kbad: K = "c";
export {};
"#,
        )],
        "main.ts",
    );

    // Both members are real keys: indexed access must resolve them (no TS2339),
    // and the literal-key assignments must hold (no spurious TS2322 / TS2536).
    assert_eq!(
        count_code(&diags, 2339),
        0,
        "indexed access of a merged-global member must resolve; got {diags:#?}"
    );
    assert_eq!(
        count_code(&diags, 2536),
        0,
        "merged-global indexed access must not raise TS2536; got {diags:#?}"
    );
    // Exactly one TS2322: the bogus key `"c"`.
    assert_eq!(
        count_code(&diags, 2322),
        1,
        "only the absent key `\"c\"` should be rejected; got {diags:#?}"
    );
}

/// Anti-hardcoding twin of the indexed-access half: rename every binder
/// (interface, both members, aliases) and the two-block merge must still be
/// published to the interface's `DefId` so indexed access `X[K]` on either
/// block's member resolves — the rule keys on the `declare global` shape, not
/// the `Registry`/`a`/`b` names.
#[test]
fn renamed_two_global_blocks_merge_into_indexed_access() {
    let diags = diagnostics(
        &[(
            "main.ts",
            r#"
declare global { interface Catalog { widget: number } }
declare global { interface Catalog { gadget: string } }
type Vw = Catalog["widget"];
type Vg = Catalog["gadget"];
const vw: Vw = 1;
const vg: Vg = "x";
export {};
"#,
        )],
        "main.ts",
    );

    assert_eq!(
        count_code(&diags, 2339),
        0,
        "indexed access of either merged-global block member must resolve; got {diags:#?}"
    );
    assert_eq!(
        count_code(&diags, 2322),
        0,
        "both indexed-access member types must hold; got {diags:#?}"
    );
}

/// Negative control: a key declared by NO block is still rejected — the fold
/// merges members, it does not widen `keyof` to `string` or the value to `any`.
#[test]
fn merged_global_keyof_still_rejects_absent_key() {
    let diags = diagnostics(
        &[(
            "main.ts",
            r#"
declare global { interface Registry { a: number } }
declare global { interface Registry { b: string } }
type Missing = Registry["nope"];
export {};
"#,
        )],
        "main.ts",
    );

    assert_eq!(
        count_code(&diags, 2339),
        1,
        "an absent key must still raise TS2339; got {diags:#?}"
    );
}

/// Anti-hardcoding: the rule is structural, not name-driven. Rename every binder
/// (interface, members, alias) and the merge still holds.
#[test]
fn merged_global_keyof_rule_is_binder_name_independent() {
    let diags = diagnostics(
        &[(
            "main.ts",
            r#"
declare global { interface Slots { widget: number } }
declare global { interface Slots { gadget: string } }
type Tags = keyof Slots;
const t1: Tags = "widget";
const t2: Tags = "gadget";
const tbad: Tags = "absent";
export {};
"#,
        )],
        "main.ts",
    );

    assert_eq!(
        count_code(&diags, 2322),
        1,
        "renamed-binder global registry should merge both keys, rejecting only \
         the absent one; got {diags:#?}"
    );
}
