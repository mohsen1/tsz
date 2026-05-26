//! Regression coverage for #9742: value exports from `declare module` blocks
//! must keep their declared type when imported.
//!
//! Structural rule: when an ambient module exports a declared value, imports
//! of that value must resolve to the declared value type at every use site,
//! including property access and assignment.

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

fn has_code(diags: &[(u32, String)], expected: u32) -> bool {
    diags.iter().any(|(code, _)| *code == expected)
}

#[test]
fn ambient_module_named_const_import_property_access_uses_declared_type() {
    let diags = diagnostics(
        &[
            (
                "pkg.d.ts",
                r#"
declare module "pkg" {
  export const obj: { foo: number };
}
"#,
            ),
            (
                "use.ts",
                r#"
import { obj } from "pkg";
obj.bar;
"#,
            ),
        ],
        "use.ts",
    );

    assert!(
        has_code(&diags, 2339),
        "expected TS2339 for missing property on imported ambient value; got {diags:#?}"
    );
}

#[test]
fn ambient_module_namespace_const_import_property_access_uses_declared_type() {
    let diags = diagnostics(
        &[
            (
                "pkg.d.ts",
                r#"
declare module "pkg" {
  export const settings: { port: number };
}
"#,
            ),
            (
                "use.ts",
                r#"
import * as ns from "pkg";
ns.settings.missing;
"#,
            ),
        ],
        "use.ts",
    );

    assert!(
        has_code(&diags, 2339),
        "expected TS2339 for missing property through namespace import; got {diags:#?}"
    );
}

#[test]
fn ambient_module_named_const_import_assignment_uses_declared_type() {
    let diags = diagnostics(
        &[
            (
                "pkg.d.ts",
                r#"
declare module "pkg" {
  export const value: { expected: number };
}
"#,
            ),
            (
                "use.ts",
                r#"
import { value } from "pkg";
const assigned: { nope: number } = value;
"#,
            ),
        ],
        "use.ts",
    );

    assert!(
        has_code(&diags, 2741) || has_code(&diags, 2322),
        "expected assignment diagnostic for imported ambient value; got {diags:#?}"
    );
}

#[test]
fn ambient_module_default_const_export_import_uses_declared_type() {
    let diags = diagnostics(
        &[
            (
                "pkg.d.ts",
                r#"
declare module "pkg" {
  const exported: { ready: boolean };
  export default exported;
}
"#,
            ),
            (
                "use.ts",
                r#"
import exported from "pkg";
exported.absent;
"#,
            ),
        ],
        "use.ts",
    );

    assert!(
        has_code(&diags, 2339),
        "expected TS2339 for missing property through default import; got {diags:#?}"
    );
}
