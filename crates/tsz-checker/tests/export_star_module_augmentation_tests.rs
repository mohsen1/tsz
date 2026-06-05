//! Coverage for #11353: `export *` re-export chains must surface
//! exports added by `declare module 'M' { ... }` augmentations targeting the
//! re-exported module.
//!
//! Structural rule: when module `C` does `export * from 'M'` and another file
//! augments `M` to add a new exported binding `X`, `X` must be reachable
//! through `C` for both named imports (`import { X } from './C'`) and
//! namespace imports (`import * as ns from './C'; ns.X`). The augmentation's
//! contribution to `M`'s public surface must be carried along every transitive
//! `export *` edge.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_multi_file;

fn diagnostics(files: &[(&str, &str)], entry: &str) -> Vec<(u32, String)> {
    check_multi_file(
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

/// Build the standard `a.ts` / `augment.ts` / `barrel.ts` / `use.ts` scaffold,
/// run the checker, and assert that none of `expected_zero` codes appear.
fn assert_export_star_augmentation(augment_body: &str, use_source: &str, expected_zero: &[u32]) {
    let augment_source = format!(
        r#"
import {{}} from "./a";
declare module "./a" {{
{augment_body}
}}
"#
    );
    let diags = diagnostics(
        &[
            ("a.ts", "\nexport class Foo {}\n"),
            ("augment.ts", augment_source.as_str()),
            ("barrel.ts", "\nexport * from \"./a\";\n"),
            ("use.ts", use_source),
        ],
        "use.ts",
    );

    for &code in expected_zero {
        assert_eq!(
            count_code(&diags, code),
            0,
            "unexpected TS{code}; got {diags:#?}"
        );
    }
}

/// Direct named import through `export *` should see an augmentation-added
/// value export from a different file.
#[test]
fn export_star_chain_carries_augmentation_added_value_export() {
    assert_export_star_augmentation(
        "    export class Bar { ping(): string; }",
        r#"
import { Bar } from "./barrel";
const b: Bar = new Bar();
b.ping();
"#,
        &[2304, 2305],
    );
}

/// Named import through a multi-hop `export *` chain must still see the
/// augmentation-added export. (Custom file set: requires an extra hop.)
#[test]
fn export_star_multi_hop_chain_carries_augmentation_added_export() {
    let diags = diagnostics(
        &[
            ("a.ts", "\nexport class Foo {}\n"),
            (
                "augment.ts",
                r#"
import {} from "./a";
declare module "./a" {
    export interface Bar {
        x: number;
    }
}
"#,
            ),
            ("mid.ts", "\nexport * from \"./a\";\n"),
            ("barrel.ts", "\nexport * from \"./mid\";\n"),
            (
                "use.ts",
                r#"
import { Bar } from "./barrel";
const b: Bar = { x: 1 };
"#,
            ),
        ],
        "use.ts",
    );

    for &code in &[2304, 2305] {
        assert_eq!(
            count_code(&diags, code),
            0,
            "unexpected TS{code} on multi-hop chain; got {diags:#?}"
        );
    }
}

/// `import * as ns` enumeration through `export *` must include
/// augmentation-added exports as namespace members in type position.
#[test]
fn namespace_import_through_export_star_includes_augmentation_added_type() {
    assert_export_star_augmentation(
        "    export interface Bar { method(): number; }",
        r#"
import * as ns from "./barrel";
const b: ns.Bar = { method() { return 1; } };
b.method();
"#,
        &[2339, 2503, 2694],
    );
}

/// Renaming the augmented export name should not change the rule.
#[test]
fn export_star_carries_augmentation_renamed_export() {
    assert_export_star_augmentation(
        "    export class Renamed { run(): void; }",
        r#"
import { Renamed } from "./barrel";
const r: Renamed = new Renamed();
r.run();
"#,
        &[2304, 2305],
    );
}

/// Augmentation-added type-only export through `export *` must surface for
/// type-position usage.
#[test]
fn export_star_carries_augmentation_added_type_only_export() {
    assert_export_star_augmentation(
        "    export type Aliased = { tag: 'aug' };",
        r#"
import { Aliased } from "./barrel";
const v: Aliased = { tag: 'aug' };
"#,
        &[2304, 2305],
    );
}

/// Augmenting an existing class with additional instance members via
/// `interface Foo { ... }` inside a `declare module 'M' { ... }` block must
/// not drop the original symbol when `M` is re-exported through `export *`.
#[test]
fn export_star_preserves_class_when_augmented_interface_in_other_file() {
    let diags = diagnostics(
        &[
            (
                "a.ts",
                r"
export class Foo {
    base(): number { return 0; }
}
",
            ),
            (
                "augment.ts",
                r#"
import {} from "./a";
declare module "./a" {
    interface Foo {
        extra: string;
    }
}
"#,
            ),
            ("barrel.ts", "\nexport * from \"./a\";\n"),
            (
                "use.ts",
                r#"
import { Foo } from "./barrel";
const f = new Foo();
f.base();
const s: string = f.extra;
"#,
            ),
        ],
        "use.ts",
    );

    for &code in &[2304, 2305, 2339] {
        assert_eq!(
            count_code(&diags, code),
            0,
            "unexpected TS{code} on class augmented by another file; got {diags:#?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Interface-MERGE through `export *` (distinct from new-export reachability).
//
// When a `declare module 'M' { interface I { … } }` augmentation *merges* a
// required member into an interface `I` that already exists in `M`, reaching
// `I` through an `export *` barrel must surface the merged member: a literal
// missing it is TS2741, and supplying it is *not* an excess-property TS2353.
// tsz previously dropped the merged member across the `export *` edge (it only
// applied augmentations when the interface was imported directly from `M`),
// so it reported a spurious TS2353 on the well-formed literal and missed the
// TS2741 on the under-specified one.
// ---------------------------------------------------------------------------

/// `a.ts` declares `interface Opts { a }`; `augment.ts` merges `{ b: string }`;
/// `barrel.ts` re-exports `M` via `export *`. `use_body` is appended after the
/// imports in `use.ts`.
fn merged_interface_diags(import_line: &str, use_body: &str) -> Vec<(u32, String)> {
    let use_source = format!("\n{import_line}\n{use_body}\n");
    diagnostics(
        &[
            ("a.ts", "\nexport interface Opts { a: number; }\n"),
            (
                "augment.ts",
                "\nimport {} from \"./a\";\ndeclare module \"./a\" {\n    interface Opts { b: string; }\n}\n",
            ),
            ("barrel.ts", "\nexport * from \"./a\";\n"),
            ("use.ts", use_source.as_str()),
        ],
        "use.ts",
    )
}

/// Named import through `export *`: a literal missing the merged member is
/// TS2741.
#[test]
fn export_star_named_import_requires_merged_augmentation_member() {
    let diags = merged_interface_diags(
        "import { Opts } from \"./barrel\";",
        "const bad: Opts = { a: 1 };",
    );
    assert_eq!(
        count_code(&diags, 2741),
        1,
        "expected TS2741 for missing merged member; got {diags:#?}"
    );
}

/// Named import through `export *`: supplying the merged member is well-formed —
/// no spurious excess-property TS2353 and no TS2741.
#[test]
fn export_star_named_import_accepts_merged_augmentation_member() {
    let diags = merged_interface_diags(
        "import { Opts } from \"./barrel\";",
        "const ok: Opts = { a: 1, b: \"x\" };",
    );
    for &code in &[2353, 2741] {
        assert_eq!(
            count_code(&diags, code),
            0,
            "unexpected TS{code} on well-formed merged literal; got {diags:#?}"
        );
    }
}

/// `import * as ns` then `ns.Opts` in type position must also carry the merged
/// member across the `export *` edge.
#[test]
fn export_star_namespace_qualified_type_requires_merged_member() {
    let diags = merged_interface_diags(
        "import * as ns from \"./barrel\";",
        "const bad: ns.Opts = { a: 1 };",
    );
    assert_eq!(
        count_code(&diags, 2741),
        1,
        "expected TS2741 through namespace-qualified merged type; got {diags:#?}"
    );
}

/// Multi-hop `export *` chain (`barrel` -> `mid` -> `a`) must still merge the
/// augmentation member. `resolve_export_in_file` follows the chain transitively.
#[test]
fn export_star_multi_hop_requires_merged_augmentation_member() {
    let diags = diagnostics(
        &[
            ("a.ts", "\nexport interface Widget { id: number; }\n"),
            (
                "augment.ts",
                "\nimport {} from \"./a\";\ndeclare module \"./a\" {\n    interface Widget { label: string; }\n}\n",
            ),
            ("mid.ts", "\nexport * from \"./a\";\n"),
            ("barrel.ts", "\nexport * from \"./mid\";\n"),
            (
                "use.ts",
                "\nimport { Widget } from \"./barrel\";\nconst bad: Widget = { id: 1 };\n",
            ),
        ],
        "use.ts",
    );
    assert_eq!(
        count_code(&diags, 2741),
        1,
        "expected TS2741 through multi-hop merged interface; got {diags:#?}"
    );
}

/// Anti-hardcoding: the rule is structural, not name-driven. Renaming every
/// binder (module names, interface name, members) preserves the behavior.
#[test]
fn export_star_merged_member_rule_is_binder_name_independent() {
    let diags = diagnostics(
        &[
            (
                "origin.ts",
                "\nexport interface Zephyr { alpha: number; }\n",
            ),
            (
                "grow.ts",
                "\nimport {} from \"./origin\";\ndeclare module \"./origin\" {\n    interface Zephyr { omega: boolean; }\n}\n",
            ),
            ("gate.ts", "\nexport * from \"./origin\";\n"),
            (
                "main.ts",
                "\nimport * as port from \"./gate\";\nconst bad: port.Zephyr = { alpha: 1 };\n",
            ),
        ],
        "main.ts",
    );
    assert_eq!(
        count_code(&diags, 2741),
        1,
        "expected TS2741 with renamed binders; got {diags:#?}"
    );
}

/// Negative control: with no augmentation present, reaching the interface
/// through `export *` must not synthesize a phantom required member, and excess
/// properties are still rejected (TS2353). Guards against over-merging.
#[test]
fn export_star_without_augmentation_keeps_plain_interface_surface() {
    let diags = diagnostics(
        &[
            ("a.ts", "\nexport interface Plain { a: number; }\n"),
            ("barrel.ts", "\nexport * from \"./a\";\n"),
            (
                "use.ts",
                "\nimport { Plain } from \"./barrel\";\nconst ok: Plain = { a: 1 };\nconst excess: Plain = { a: 1, b: 2 };\n",
            ),
        ],
        "use.ts",
    );
    assert_eq!(
        count_code(&diags, 2741),
        0,
        "no phantom required member without augmentation; got {diags:#?}"
    );
    assert_eq!(
        count_code(&diags, 2353),
        1,
        "excess property still rejected without augmentation; got {diags:#?}"
    );
}
