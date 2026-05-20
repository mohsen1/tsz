//! End-to-end coverage for arbitrary-extension declaration files
//! (`<base>.d.<ext>.ts`, `<ext>` outside TS/JS/JSON).
//!
//! Owner layer: checker module resolution. The structural rule is that such a
//! file must be reachable through the user-written `<base>.<ext>` specifier
//! (e.g. `./component.html`), never through the naive `<base>.d.<ext>` strip
//! form. Section §25 anti-hardcoding: the same rule must hold for any
//! identifier and any extension, so each test pairs the reported repro with
//! at least one renamed variant.
//!
//! Adjacent cases covered (§26 generalization gate):
//!
//! 1. Resolution succeeds (happy path) — various extensions and directory depths.
//! 2. Named-export shapes: `export const`, `export default`, `export =`.
//! 3. Import type errors are checked: a wrong type on the imported value
//!    surfaces TS2322, not a missing-module error.
//! 4. Within a declaration file (`.d.ts`): imports of arbitrary-ext files
//!    must always resolve without a TS6263 flag warning.
//! 5. `probe_file_name_index` bare-path probe for project-relative paths.

use tsz_checker::context::CheckerOptions;
use tsz_checker::module_resolution::{FileNameIndex, probe_file_name_index};
use tsz_checker::test_utils::check_multi_file;

fn diagnostic_codes(files: &[(&str, &str)], entry: &str) -> Vec<u32> {
    check_multi_file(files, entry, CheckerOptions::default())
        .into_iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn html_arbitrary_ext_decl_resolves_under_user_form() {
    // `component.d.html.ts` is the canonical TS arbitrary-extension decl
    // shape. The user writes `import "./component.html"` and the symbol
    // resolves to the declaration file.
    let decl = r#"
declare const value: number;
export = value;
"#;
    let consumer = r#"
import value from "./component.html";
const n: number = value;
"#;
    let codes = diagnostic_codes(
        &[
            ("/proj/component.d.html.ts", decl),
            ("/proj/main.ts", consumer),
        ],
        "/proj/main.ts",
    );
    assert!(
        codes.is_empty(),
        "user-form specifier should resolve: {codes:?}",
    );
}

#[test]
fn html_arbitrary_ext_decl_renamed_extension_still_resolves() {
    // §25 generalization: the rule is structural over `<ext>`, not keyed on
    // `html`. The same shape works for `.svelte`, `.css`, etc.
    let decl = r#"
declare const config: { url: string };
export = config;
"#;
    let consumer = r#"
import config from "./Button.svelte";
const u: string = config.url;
"#;
    let codes = diagnostic_codes(
        &[
            ("/proj/Button.d.svelte.ts", decl),
            ("/proj/main.ts", consumer),
        ],
        "/proj/main.ts",
    );
    assert!(
        codes.is_empty(),
        "renamed extension should still resolve: {codes:?}",
    );
}

#[test]
fn nested_arbitrary_ext_decl_resolves_with_directory_prefix() {
    // The arbitrary-ext rule preserves the directory portion of the
    // specifier; a renamed name (`Card` vs `Button`) keeps the rule
    // structural over the file system layout.
    let decl = r#"
declare const config: { title: string };
export = config;
"#;
    let consumer = r#"
import config from "./widgets/Card.svelte";
const t: string = config.title;
"#;
    let codes = diagnostic_codes(
        &[
            ("/proj/widgets/Card.d.svelte.ts", decl),
            ("/proj/main.ts", consumer),
        ],
        "/proj/main.ts",
    );
    assert!(
        codes.is_empty(),
        "nested arbitrary-ext decl should resolve: {codes:?}",
    );
}

// ---------------------------------------------------------------------------
// Named-export shapes (§26 adjacent cases: `export const`, `export default`,
// `export =`). Each variant proves the fix is structural over export syntax.
// ---------------------------------------------------------------------------

#[test]
fn arbitrary_ext_decl_named_export_resolves() {
    // `export const` shape — the imported name is checked for type correctness.
    let decl = "export declare const asset: string;";
    let consumer = r#"
import { asset } from "./image.svg";
const s: string = asset;
"#;
    let codes = diagnostic_codes(
        &[("/proj/image.d.svg.ts", decl), ("/proj/main.ts", consumer)],
        "/proj/main.ts",
    );
    assert!(
        codes.is_empty(),
        "named export from arbitrary-ext decl should resolve: {codes:?}",
    );
}

#[test]
fn arbitrary_ext_decl_default_export_resolves() {
    // `export default` shape.
    let decl = "declare const style: string; export default style;";
    let consumer = r#"
import style from "./theme.css";
const s: string = style;
"#;
    let codes = diagnostic_codes(
        &[("/proj/theme.d.css.ts", decl), ("/proj/main.ts", consumer)],
        "/proj/main.ts",
    );
    assert!(
        codes.is_empty(),
        "default export from arbitrary-ext decl should resolve: {codes:?}",
    );
}

#[test]
fn arbitrary_ext_decl_type_error_on_imported_value() {
    // When the imported value is used with the wrong type, TS2322 must fire,
    // NOT a missing-module diagnostic. This proves the module's type info is
    // available after resolution.
    let decl = "export declare const count: number;";
    let consumer = r#"
import { count } from "./data.vue";
const s: string = count;
"#;
    let codes = diagnostic_codes(
        &[("/proj/data.d.vue.ts", decl), ("/proj/main.ts", consumer)],
        "/proj/main.ts",
    );
    assert!(
        codes.contains(&2322),
        "type mismatch on arbitrary-ext import should surface TS2322: {codes:?}",
    );
    assert!(
        !codes.contains(&2307),
        "TS2307 must not fire when module resolves: {codes:?}",
    );
}

// ---------------------------------------------------------------------------
// Within-declaration-file imports (§26 adjacent case 4).
// Importing an arbitrary-ext file from a `.d.ts` file must always resolve
// without any diagnostic — regardless of `allowArbitraryExtensions` setting.
// ---------------------------------------------------------------------------

#[test]
fn arbitrary_ext_decl_from_within_declaration_file_no_error() {
    // A `.d.ts` file importing `./component.html` should resolve cleanly.
    // tsc does NOT emit TS6263 when the importer is itself a declaration file.
    let html_decl = "export declare const html: string;";
    let wrapper_dts = r#"
export { html } from "./component.html";
"#;
    let consumer = r#"
import { html } from "./wrapper";
const s: string = html;
"#;
    let codes = diagnostic_codes(
        &[
            ("/proj/component.d.html.ts", html_decl),
            ("/proj/wrapper.d.ts", wrapper_dts),
            ("/proj/main.ts", consumer),
        ],
        "/proj/main.ts",
    );
    assert!(
        codes.is_empty(),
        "import from within .d.ts should produce no errors: {codes:?}",
    );
}

#[test]
fn arbitrary_ext_decl_svelte_from_declaration_file_no_error() {
    // Same rule holds for .svelte — proves generality (§25).
    let svelte_decl = "export declare const props: { title: string };";
    let wrapper_dts = r#"
export { props } from "./Card.svelte";
"#;
    let consumer = r#"
import { props } from "./wrapper";
const t: string = props.title;
"#;
    let codes = diagnostic_codes(
        &[
            ("/proj/Card.d.svelte.ts", svelte_decl),
            ("/proj/wrapper.d.ts", wrapper_dts),
            ("/proj/main.ts", consumer),
        ],
        "/proj/main.ts",
    );
    assert!(
        codes.is_empty(),
        "svelte import from within .d.ts should produce no errors: {codes:?}",
    );
}

// ---------------------------------------------------------------------------
// `probe_file_name_index` bare-path probe (§26 adjacent case 5).
// Project-relative bare paths like `packages/ui/Button.svelte` must also
// resolve via `probe_file_name_index`, which now includes the arbitrary-ext
// probe.
// ---------------------------------------------------------------------------

fn index_from_paths(paths: &[(&str, usize)]) -> FileNameIndex {
    paths.iter().map(|(p, i)| ((*p).to_string(), *i)).collect()
}

#[test]
fn probe_arbitrary_ext_bare_path_html() {
    // `probe_file_name_index` is used for project-relative bare paths.  It must
    // also resolve `packages/ui/Banner.html` → `packages/ui/Banner.d.html.ts`.
    let idx = index_from_paths(&[
        ("/proj/packages/ui/Banner.d.html.ts", 0),
        ("/proj/packages/ui/widget.d.svg.ts", 1),
    ]);

    assert_eq!(
        probe_file_name_index("/proj/packages/ui/Banner.html", &idx),
        Some(0),
        "bare html path must resolve to index 0",
    );
    assert_eq!(
        probe_file_name_index("/proj/packages/ui/widget.svg", &idx),
        Some(1),
        "bare svg path must resolve to index 1",
    );
    // Wrong name+ext must not collide (§25 structural rule).
    assert_eq!(
        probe_file_name_index("/proj/packages/ui/Banner.svg", &idx),
        None,
        "wrong name+ext must not match",
    );
}

#[test]
fn probe_arbitrary_ext_bare_path_vue_and_css() {
    // Second name variant (§25): the rule holds for `.vue` and `.css` just
    // as for `.html`.
    let idx = index_from_paths(&[
        ("/lib/components/Modal.d.vue.ts", 0),
        ("/lib/styles/reset.d.css.ts", 1),
    ]);

    assert_eq!(
        probe_file_name_index("/lib/components/Modal.vue", &idx),
        Some(0),
    );
    assert_eq!(
        probe_file_name_index("/lib/styles/reset.css", &idx),
        Some(1),
    );
    // Recognized TS/JS extensions must NOT trigger the arbitrary-ext probe.
    assert_eq!(
        probe_file_name_index("/lib/components/Modal.ts", &idx),
        None,
        "recognized ext must not match arbitrary-ext declaration",
    );
}
