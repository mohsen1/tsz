//! Coverage for the `.json` declaration-companion resolution rule:
//! a `.json` specifier's sibling `<base>.d.json.ts` declaration file takes
//! priority over the JSON file's own literal shape.
//!
//! Owner layer: module resolution (`tsz_core::module_resolver::file_probing`
//! for on-disk project discovery; `tsz_checker::module_resolution`'s
//! `resolve_specifier_via_file_index_uncached` and `register_canonical_forms`
//! for the checker's specifier → file-index maps).
//!
//! Structural rule (pinned `typescript@7.0.2` oracle,
//! `declarationFileForJsonImport.ts`): `tsc`'s `tryAddingExtensions`
//! `Extension.Json` case tries the `Declaration` extension
//! (`<base>.d.json.ts`) *before* the `Json` extension (`<base>.json`),
//! unconditionally — independent of `resolveJsonModule`. So when both files
//! exist, `import x from "./data.json"` always types `x` from the
//! declaration file, and never reports TS2732 ("consider using
//! `--resolveJsonModule`"), in either `resolveJsonModule` setting.
//!
//! Adjacent cases (§26 generalization gate):
//! 1. Declaration wins with `resolveJsonModule: true` (would otherwise type
//!    from the JSON literal's own inferred shape).
//! 2. Declaration wins with `resolveJsonModule: false` (would otherwise be
//!    TS2732, "cannot find module").
//! 3. Renamed binder/specifier (§25 structural-over-identifier gate).
//! 4. Negative control: without the declaration file, `resolveJsonModule:
//!    false` still errors (the fix must not blanket-suppress it).
//! 5. Negative control: without the declaration file, `resolveJsonModule:
//!    true` still types from the JSON literal's own shape (a real mismatch
//!    still surfaces TS2322).

use tsz_checker::context::CheckerOptions;
use tsz_common::common::ModuleKind;

fn diagnostic_codes(files: &[(&str, &str)], entry: &str, resolve_json_module: bool) -> Vec<u32> {
    tsz_checker::test_utils::check_multi_file(
        files,
        entry,
        CheckerOptions {
            module: ModuleKind::CommonJS,
            resolve_json_module,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|d| d.code)
    .collect()
}

#[test]
fn json_decl_companion_wins_with_resolve_json_module_true() {
    let decl = "declare var val: string; export default val;";
    let json = "{}";
    let consumer = r#"
import data from "./data.json";
let x: string = data;
"#;
    let codes = diagnostic_codes(
        &[
            ("/proj/data.d.json.ts", decl),
            ("/proj/data.json", json),
            ("/proj/main.ts", consumer),
        ],
        "/proj/main.ts",
        true,
    );
    assert!(
        codes.is_empty(),
        "declaration companion should type `data` as string, no diagnostics: {codes:?}",
    );
}

#[test]
fn json_decl_companion_wins_with_resolve_json_module_false() {
    let decl = "declare var val: string; export default val;";
    let json = "{}";
    let consumer = r#"
import data from "./data.json";
let x: string = data;
"#;
    let codes = diagnostic_codes(
        &[
            ("/proj/data.d.json.ts", decl),
            ("/proj/data.json", json),
            ("/proj/main.ts", consumer),
        ],
        "/proj/main.ts",
        false,
    );
    assert!(
        codes.is_empty(),
        "declaration companion resolves even with resolveJsonModule off, no TS2732: {codes:?}",
    );
}

#[test]
fn json_decl_companion_renamed_binder_and_specifier() {
    // §25: structural over the specific identifiers `data`/`val`/`data.json`.
    let decl = "declare var payload: number; export default payload;";
    let json = "{}";
    let consumer = r#"
import cfg from "./settings.json";
let n: number = cfg;
"#;
    let codes = diagnostic_codes(
        &[
            ("/proj/settings.d.json.ts", decl),
            ("/proj/settings.json", json),
            ("/proj/main.ts", consumer),
        ],
        "/proj/main.ts",
        true,
    );
    assert!(
        codes.is_empty(),
        "renamed binder/specifier should behave identically: {codes:?}",
    );
}

#[test]
fn json_without_decl_companion_still_errors_when_resolve_json_module_off() {
    // Negative control: no `.d.json.ts` sibling exists, so the fix must not
    // blanket-suppress the "not resolvable as a value import" diagnostic.
    // This lightweight index-based harness reports TS2306 ("File is not a
    // module") here rather than the real disk-backed resolver's TS2732
    // upgrade (`tsz_core::module_resolver::mod.rs`'s
    // `JsonModuleWithoutResolveJsonModule`, exercised by the CLI/conformance
    // path) — confirmed identical on `main` before this change, so the
    // assertion pins the harness's pre-existing behavior, not a claim about
    // which code tsc itself reports.
    let json = "{}";
    let consumer = r#"
import data from "./data.json";
"#;
    let codes = diagnostic_codes(
        &[("/proj/data.json", json), ("/proj/main.ts", consumer)],
        "/proj/main.ts",
        false,
    );
    assert!(
        !codes.is_empty(),
        "missing declaration companion + resolveJsonModule off should still error: {codes:?}",
    );
}

#[test]
fn json_without_decl_companion_still_types_from_json_literal_when_resolve_json_module_on() {
    // Negative control: no `.d.json.ts` sibling, resolveJsonModule on — the
    // JSON file's own inferred shape (`{}`) still governs, so a real
    // mismatch against `string` still reports TS2322.
    let json = "{}";
    let consumer = r#"
import data from "./data.json";
let x: string = data;
"#;
    let codes = diagnostic_codes(
        &[("/proj/data.json", json), ("/proj/main.ts", consumer)],
        "/proj/main.ts",
        true,
    );
    assert!(
        codes.contains(&2322),
        "missing declaration companion should still type from the JSON literal shape: {codes:?}",
    );
}
