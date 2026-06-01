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
