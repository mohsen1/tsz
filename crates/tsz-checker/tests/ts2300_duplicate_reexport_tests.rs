//! TS2300 for duplicate `export { ... } from "..."` re-export specifiers.
//!
//! Structural rule: two specs that bind the same EXPORTED name share the file's
//! exports-table slot — independent of type-only-ness, renaming, or source
//! module — and tsc emits TS2300 on each. The `no_ts2300` tests guard against
//! over-firing on the adjacent shapes (aliased rename, wildcard + explicit,
//! import + re-export).

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::diagnostic_count;
use tsz_common::common::ModuleKind;

fn check_diags(files: &[(&str, &str)], entry_file: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_multi_file(
        files,
        entry_file,
        CheckerOptions {
            module: ModuleKind::CommonJS,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|d| (d.code, d.message_text))
    .collect()
}

const SOURCE: &str = "export class Foo { x: number = 0; }\n";
const SOURCE_B: &str = "export class Foo { y: string = \"\"; }\n";

// ─── duplicate same-name value re-exports ─────────────────────────────────────

#[test]
fn duplicate_value_reexport_emits_ts2300() {
    let test = "export { Foo } from './a';\nexport { Foo } from './a';\n";
    let diags = check_diags(&[("a.ts", SOURCE), ("test.ts", test)], "test.ts");
    assert_eq!(
        diagnostic_count(&diags, 2300),
        2,
        "expected TS2300 on each duplicate re-export spec, got: {diags:?}"
    );
}

#[test]
fn duplicate_type_reexport_emits_ts2300() {
    let test = "export type { Foo } from './a';\nexport type { Foo } from './a';\n";
    let diags = check_diags(&[("a.ts", SOURCE), ("test.ts", test)], "test.ts");
    assert_eq!(
        diagnostic_count(&diags, 2300),
        2,
        "expected TS2300 on each duplicate type-only re-export, got: {diags:?}"
    );
}

#[test]
fn value_then_type_reexport_emits_ts2300() {
    let test = "export { Foo } from './a';\nexport type { Foo } from './a';\n";
    let diags = check_diags(&[("a.ts", SOURCE), ("test.ts", test)], "test.ts");
    assert_eq!(
        diagnostic_count(&diags, 2300),
        2,
        "expected TS2300 on both value + type-only re-exports of same name, got: {diags:?}"
    );
}

#[test]
fn type_then_value_reexport_emits_ts2300() {
    let test = "export type { Foo } from './a';\nexport { Foo } from './a';\n";
    let diags = check_diags(&[("a.ts", SOURCE), ("test.ts", test)], "test.ts");
    assert_eq!(
        diagnostic_count(&diags, 2300),
        2,
        "expected TS2300 on both type-only + value re-exports of same name, got: {diags:?}"
    );
}

// ─── renamed export-as collisions ─────────────────────────────────────────────

#[test]
fn duplicate_renamed_export_emits_ts2300() {
    // Both specs export the same name `X`; the `Foo as X` rename does not
    // sidestep the duplicate.
    let test = "export { Foo as X } from './a';\nexport { Foo as X } from './a';\n";
    let diags = check_diags(&[("a.ts", SOURCE), ("test.ts", test)], "test.ts");
    assert_eq!(
        diagnostic_count(&diags, 2300),
        2,
        "expected TS2300 on duplicate aliased re-export name, got: {diags:?}"
    );
}

#[test]
fn duplicate_exported_name_via_different_aliases_emits_ts2300() {
    // Same exported name `X`, different original names — still a duplicate of `X`.
    let a_two = "export class Foo { x: number = 0; }\nexport class Bar { y: string = \"\"; }\n";
    let test = "export { Foo as X } from './a';\nexport { Bar as X } from './a';\n";
    let diags = check_diags(&[("a.ts", a_two), ("test.ts", test)], "test.ts");
    assert_eq!(
        diagnostic_count(&diags, 2300),
        2,
        "expected TS2300 on `X` exported via two different sources, got: {diags:?}"
    );
}

// ─── duplicate across different source modules ───────────────────────────────

#[test]
fn duplicate_same_name_different_sources_emits_ts2300() {
    let test = "export { Foo } from './a';\nexport type { Foo } from './b';\n";
    let diags = check_diags(
        &[("a.ts", SOURCE), ("b.ts", SOURCE_B), ("test.ts", test)],
        "test.ts",
    );
    assert_eq!(
        diagnostic_count(&diags, 2300),
        2,
        "expected TS2300 even when sources differ — name space collision is on the exported name, got: {diags:?}"
    );
}

// ─── no false-positives ──────────────────────────────────────────────────────

#[test]
fn aliased_reexports_with_distinct_names_no_ts2300() {
    let test = "export { Foo as Bar } from './a';\nexport type { Foo } from './a';\n";
    let diags = check_diags(&[("a.ts", SOURCE), ("test.ts", test)], "test.ts");
    assert_eq!(
        diagnostic_count(&diags, 2300),
        0,
        "distinct exported names (Bar vs Foo) must not collide, got: {diags:?}"
    );
}

#[test]
fn wildcard_plus_explicit_reexport_no_ts2300() {
    // The literal repro from issue #11334: `export *` brings Foo in implicitly,
    // an explicit `export { Foo as Bar }` adds the renamed binding, and an
    // explicit `export type { Foo }` adds the type-only binding for `Foo`.
    // Explicit names override the wildcard silently — no duplicate.
    let test = "export * from './a';
export { Foo as Bar } from './a';
export type { Foo } from './a';
";
    let diags = check_diags(&[("a.ts", SOURCE), ("test.ts", test)], "test.ts");
    assert_eq!(
        diagnostic_count(&diags, 2300),
        0,
        "wildcard + explicit re-exports of distinct names must not collide, got: {diags:?}"
    );
}

#[test]
fn import_plus_reexport_no_ts2300() {
    // tsc places `import { X }` in file locals and `export { X } from "mod"`
    // in the file's exports table — different slots, no collision.
    let test = "import { Foo } from './a';\nexport { Foo } from './a';\nconst _: Foo = { x: 1 };\n";
    let diags = check_diags(&[("a.ts", SOURCE), ("test.ts", test)], "test.ts");
    assert_eq!(
        diagnostic_count(&diags, 2300),
        0,
        "import + same-name re-export must not collide, got: {diags:?}"
    );
}

#[test]
fn three_explicit_reexports_all_emit_ts2300() {
    // Each adjacent pair contributes a diagnostic. tsc emits TS2300 on every
    // spec node that participates in the duplicate set (3 here).
    let test = "export { Foo } from './a';
export type { Foo } from './a';
export { Foo } from './a';
";
    let diags = check_diags(&[("a.ts", SOURCE), ("test.ts", test)], "test.ts");
    assert_eq!(
        diagnostic_count(&diags, 2300),
        3,
        "expected TS2300 on each of three duplicate re-export specs, got: {diags:?}"
    );
}
