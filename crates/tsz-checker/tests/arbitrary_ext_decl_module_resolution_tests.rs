//! End-to-end coverage for arbitrary-extension declaration files
//! (`<base>.d.<ext>.ts`, `<ext>` outside TS/JS/JSON).
//!
//! Owner layer: checker module resolution. The structural rule is that such a
//! file must be reachable through the user-written `<base>.<ext>` specifier
//! (e.g. `./component.html`), never through the naive `<base>.d.<ext>` strip
//! form. Section §25 anti-hardcoding: the same rule must hold for any
//! identifier and any extension, so each test pairs the reported repro with
//! at least one renamed variant.

use tsz_checker::context::CheckerOptions;
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
