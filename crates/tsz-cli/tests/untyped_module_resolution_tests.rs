//! Regression tests for request-scoped resolution of modules that resolve
//! without a program file.
//!
//! Structural rule:
//! When a module specifier resolves successfully but yields no file the program
//! can index — an untyped JavaScript module under `node_modules` picked up by
//! the resolver's JS probe, or an ambient `declare module` target — the driver
//! can only record it in the mode-agnostic resolved-specifier set, because the
//! request-keyed path map is keyed by a program file index that does not exist
//! for it. `tsc` binds such an import as `any` and reports nothing (or TS7016
//! under `noImplicitAny`); tsz does the same through
//! `CheckerContext::module_resolved_without_program_file_for_request`, which
//! request-scoped consumers consult instead of ignoring the mode-agnostic set
//! whenever request-keyed paths exist at all.
//!
//! Owner layer: `crates/tsz-checker/src/context/resolver.rs` (the query), with
//! the two request-scoped call sites in
//! `crates/tsz-checker/src/declarations/import/equals.rs` (`import x =
//! require(...)`) and
//! `crates/tsz-checker/src/declarations/dynamic_import_checker.rs`
//! (`import(...)`).
//!
//! The negative direction matters as much as the positive one: a specifier
//! that resolves for one request mode and genuinely fails for another must
//! keep its TS2307. That is what the request-scoped guard existed for, and
//! `require_only_exports_still_reports_ts2307_for_esm_dynamic_import` pins it.

use super::args::CliArgs;
use super::driver::compile;
use clap::Parser;
use std::path::Path;
use tsz_common::diagnostics::Diagnostic;

fn write_file(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("failed to create parent directory");
    }
    std::fs::write(path, contents).expect("failed to write file");
}

fn parse_args(args: &[&str]) -> CliArgs {
    CliArgs::try_parse_from(args).expect("test args should parse")
}

fn write_tsconfig(base: &Path, extra_options: &str, files: &[&str]) {
    let files_json = files
        .iter()
        .map(|f| format!("\"{f}\""))
        .collect::<Vec<_>>()
        .join(", ");
    write_file(
        &base.join("tsconfig.json"),
        &format!(
            r#"{{
              "compilerOptions": {{
                "target": "es2015",
                "module": "commonjs",
                "strict": false,
                "noEmit": true,
                "types": []{extra_options}
              }},
              "files": [{files_json}]
            }}"#
        ),
    );
}

/// An untyped package: a `node_modules` entry whose only file is JavaScript,
/// with no `package.json` `types` entry and no sibling declaration file.
fn write_untyped_package(base: &Path, package_name: &str) {
    write_file(
        &base.join(format!("node_modules/{package_name}/index.js")),
        "module.exports = {};\n",
    );
}

fn diagnostics_with_code(diagnostics: &[Diagnostic], code: u32) -> Vec<&Diagnostic> {
    diagnostics
        .iter()
        .filter(|diag| diag.code == code)
        .collect()
}

const CANNOT_FIND_MODULE: u32 = 2307;
const COULD_NOT_FIND_DECLARATION_FILE: u32 = 7016;

/// Every import form that can reach an untyped `node_modules` package binds it
/// as `any` without TS2307. The binder names vary across the matrix (package
/// name and local alias both differ per file) so no name-shaped rule can pass
/// this by accident.
#[test]
fn untyped_node_modules_package_resolves_for_every_import_form() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let base = temp.path();

    write_untyped_package(base, "alpha-pkg");
    write_untyped_package(base, "beta-pkg");
    write_untyped_package(base, "gamma-pkg");
    write_untyped_package(base, "delta-pkg");

    write_file(
        &base.join("equals.ts"),
        "import viaRequire = require(\"alpha-pkg\");\nexport const a = viaRequire;\n",
    );
    write_file(
        &base.join("dynamic.ts"),
        "export async function load() {\n    const mod = await import(\"beta-pkg\");\n    return mod;\n}\n",
    );
    write_file(
        &base.join("star.ts"),
        "import * as everything from \"gamma-pkg\";\nexport const g = everything;\n",
    );
    write_file(
        &base.join("reexport.ts"),
        "export { whatever } from \"delta-pkg\";\n",
    );

    write_tsconfig(
        base,
        "",
        &["equals.ts", "dynamic.ts", "star.ts", "reexport.ts"],
    );

    let args = parse_args(&["tsz", "--noEmit"]);
    let result = compile(&args, base).expect("compile should succeed");

    let unresolved = diagnostics_with_code(&result.diagnostics, CANNOT_FIND_MODULE);
    assert!(
        unresolved.is_empty(),
        "untyped node_modules packages resolve as `any` for every import form, got: {unresolved:#?}"
    );
}

/// A subpath into an untyped package resolves the same way as its entry point.
/// Nesting the target one directory deep exercises the resolver's directory
/// probe rather than the package-index fallback.
#[test]
fn untyped_node_modules_subpath_resolves_for_import_equals_and_dynamic_import() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let base = temp.path();

    write_file(
        &base.join("node_modules/nested-pkg/lib/deep/helper.js"),
        "module.exports = {};\n",
    );

    write_file(
        &base.join("sub_equals.ts"),
        "import deep = require(\"nested-pkg/lib/deep/helper\");\nexport const d = deep;\n",
    );
    write_file(
        &base.join("sub_dynamic.ts"),
        "export async function load() {\n    return await import(\"nested-pkg/lib/deep/helper\");\n}\n",
    );

    write_tsconfig(base, "", &["sub_equals.ts", "sub_dynamic.ts"]);

    let args = parse_args(&["tsz", "--noEmit"]);
    let result = compile(&args, base).expect("compile should succeed");

    let unresolved = diagnostics_with_code(&result.diagnostics, CANNOT_FIND_MODULE);
    assert!(
        unresolved.is_empty(),
        "an untyped subpath resolves as `any` like the package entry, got: {unresolved:#?}"
    );
}

/// Under `noImplicitAny` the same two request-scoped sites report TS7016 — the
/// "no declaration file" diagnostic — and never TS2307. A fix that suppressed
/// the false positive by suppressing the site entirely would lose this.
#[test]
fn untyped_node_modules_package_reports_ts7016_under_no_implicit_any() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let base = temp.path();

    write_untyped_package(base, "implicit-pkg");

    write_file(
        &base.join("equals.ts"),
        "import viaRequire = require(\"implicit-pkg\");\nexport const a = viaRequire;\n",
    );
    write_file(
        &base.join("dynamic.ts"),
        "export async function load() {\n    return await import(\"implicit-pkg\");\n}\n",
    );

    write_tsconfig(
        base,
        ",\n                \"noImplicitAny\": true",
        &["equals.ts", "dynamic.ts"],
    );

    let args = parse_args(&["tsz", "--noEmit"]);
    let result = compile(&args, base).expect("compile should succeed");

    let unresolved = diagnostics_with_code(&result.diagnostics, CANNOT_FIND_MODULE);
    assert!(
        unresolved.is_empty(),
        "`noImplicitAny` reports the missing-declaration diagnostic, not TS2307, got: {unresolved:#?}"
    );

    let missing_declaration =
        diagnostics_with_code(&result.diagnostics, COULD_NOT_FIND_DECLARATION_FILE);
    assert_eq!(
        missing_declaration.len(),
        2,
        "both the import-equals and the dynamic import report TS7016 under `noImplicitAny`, got: {missing_declaration:#?}"
    );
}

/// A bare side-effect import (`import "pkg";`) of an untyped `node_modules`
/// package must stay silent under `noImplicitAny`, unlike every other import
/// form. `noUncheckedSideEffectImports` (on by default as of the pinned
/// 7.0.2 oracle) only controls whether a genuinely *unresolvable*
/// side-effect import gets a diagnostic (TS2882); it says nothing about a
/// specifier that resolved fine but lacks declarations, so TS7016 stays
/// suppressed here regardless of the flag's value.
#[test]
fn untyped_node_modules_package_side_effect_import_stays_silent_under_no_implicit_any() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let base = temp.path();

    write_untyped_package(base, "silent-pkg");

    write_file(
        &base.join("side_effect.ts"),
        "import \"silent-pkg\";\nexport const marker = 1;\n",
    );

    write_tsconfig(
        base,
        ",\n                \"noImplicitAny\": true",
        &["side_effect.ts"],
    );

    let args = parse_args(&["tsz", "--noEmit"]);
    let result = compile(&args, base).expect("compile should succeed");

    let unresolved = diagnostics_with_code(&result.diagnostics, CANNOT_FIND_MODULE);
    assert!(
        unresolved.is_empty(),
        "a side-effect import never reports TS2307 for an untyped package, got: {unresolved:#?}"
    );

    let missing_declaration =
        diagnostics_with_code(&result.diagnostics, COULD_NOT_FIND_DECLARATION_FILE);
    assert!(
        missing_declaration.is_empty(),
        "a resolved untyped package is not a side-effect-import resolution failure, so TS7016 stays suppressed, got: {missing_declaration:#?}"
    );
}

/// Explicitly enabling `noUncheckedSideEffectImports` (redundant with the
/// real default, but pins the behavior independent of it) must not change
/// the previous case: TS7016 still stays suppressed for a resolved-but-
/// untyped side-effect import target.
#[test]
fn untyped_node_modules_package_side_effect_import_stays_silent_with_unchecked_side_effect_imports_on()
 {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let base = temp.path();

    write_untyped_package(base, "checked-pkg");

    write_file(
        &base.join("side_effect.ts"),
        "import \"checked-pkg\";\nexport const marker = 1;\n",
    );

    write_tsconfig(
        base,
        ",\n                \"noImplicitAny\": true,\n                \"noUncheckedSideEffectImports\": true",
        &["side_effect.ts"],
    );

    let args = parse_args(&["tsz", "--noEmit"]);
    let result = compile(&args, base).expect("compile should succeed");

    let missing_declaration =
        diagnostics_with_code(&result.diagnostics, COULD_NOT_FIND_DECLARATION_FILE);
    assert!(
        missing_declaration.is_empty(),
        "a resolved untyped package is not a side-effect-import resolution failure, so TS7016 stays suppressed even with noUncheckedSideEffectImports on, got: {missing_declaration:#?}"
    );
}

/// Negative direction for the TS7016-suppression fix above: a side-effect
/// import of a specifier that does NOT resolve at all is a genuine
/// resolution failure, which `noUncheckedSideEffectImports` (on by default)
/// DOES gate — tsc reports TS2882, not silence and not TS7016. The fix must
/// not conflate "resolved but untyped" with "did not resolve".
#[test]
fn missing_module_side_effect_import_reports_ts2882_by_default() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let base = temp.path();

    write_file(
        &base.join("side_effect.ts"),
        "import \"totally-missing-pkg\";\nexport const marker = 1;\n",
    );

    write_tsconfig(base, "", &["side_effect.ts"]);

    let args = parse_args(&["tsz", "--noEmit"]);
    let result = compile(&args, base).expect("compile should succeed");

    const CANNOT_FIND_MODULE_OR_TYPE_DECLARATIONS_FOR_SIDE_EFFECT_IMPORT_OF: u32 = 2882;
    let side_effect_not_found = diagnostics_with_code(
        &result.diagnostics,
        CANNOT_FIND_MODULE_OR_TYPE_DECLARATIONS_FOR_SIDE_EFFECT_IMPORT_OF,
    );
    assert_eq!(
        side_effect_not_found.len(),
        1,
        "noUncheckedSideEffectImports defaults to on, so a genuinely missing side-effect import reports TS2882, got: {:#?}",
        result.diagnostics
    );

    let missing_declaration =
        diagnostics_with_code(&result.diagnostics, COULD_NOT_FIND_DECLARATION_FILE);
    assert!(
        missing_declaration.is_empty(),
        "a genuinely missing module reports TS2882, never TS7016, got: {missing_declaration:#?}"
    );
}

/// Explicitly disabling `noUncheckedSideEffectImports` flips the previous
/// case back to silence: a side-effect import of a specifier that does not
/// resolve at all reports nothing when the flag is off.
#[test]
fn missing_module_side_effect_import_stays_silent_with_unchecked_side_effect_imports_off() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let base = temp.path();

    write_file(
        &base.join("side_effect.ts"),
        "import \"totally-missing-pkg\";\nexport const marker = 1;\n",
    );

    write_tsconfig(
        base,
        ",\n                \"noUncheckedSideEffectImports\": false",
        &["side_effect.ts"],
    );

    let args = parse_args(&["tsz", "--noEmit"]);
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "noUncheckedSideEffectImports: false silences a genuinely missing side-effect import entirely, got: {:#?}",
        result.diagnostics
    );
}

/// Fallback direction: a specifier with nothing behind it at all still reports
/// TS2307 at both request-scoped sites. Nothing about the fix may make a
/// genuinely missing module look resolved.
#[test]
fn genuinely_missing_module_still_reports_ts2307_at_both_request_sites() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let base = temp.path();

    write_file(
        &base.join("equals.ts"),
        "import absent = require(\"no-such-pkg\");\nexport const a = absent;\n",
    );
    write_file(
        &base.join("dynamic.ts"),
        "export async function load() {\n    return await import(\"also-absent-pkg\");\n}\n",
    );

    write_tsconfig(base, "", &["equals.ts", "dynamic.ts"]);

    let args = parse_args(&["tsz", "--noEmit"]);
    let result = compile(&args, base).expect("compile should succeed");

    let unresolved = diagnostics_with_code(&result.diagnostics, CANNOT_FIND_MODULE);
    assert_eq!(
        unresolved.len(),
        2,
        "a module with nothing behind it keeps TS2307 at both sites, got: {unresolved:#?}"
    );
}

/// The guard this change relaxes existed so that a specifier resolving under
/// one request mode could not hide a genuine failure under another. Pin it: a
/// package whose `exports` map only has a `require` condition resolves for the
/// `.cts` import-equals and genuinely fails for the `.mts` dynamic import, and
/// `tsc` reports TS2307 for exactly the second one.
#[test]
fn require_only_exports_still_reports_ts2307_for_esm_dynamic_import() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let base = temp.path();

    write_file(
        &base.join("node_modules/require-only-pkg/package.json"),
        r#"{
          "name": "require-only-pkg",
          "version": "1.0.0",
          "exports": {
            ".": {
              "require": {
                "types": "./index.d.cts",
                "default": "./index.cjs"
              }
            }
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/require-only-pkg/index.d.cts"),
        "export declare const value: number;\n",
    );
    write_file(
        &base.join("node_modules/require-only-pkg/index.cjs"),
        "module.exports = { value: 1 };\n",
    );

    write_file(
        &base.join("esm.mts"),
        "export async function load() {\n    return await import(\"require-only-pkg\");\n}\n",
    );
    write_file(
        &base.join("cjs.cts"),
        "import requireOnly = require(\"require-only-pkg\");\nexport const v = requireOnly;\n",
    );

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2022",
            "module": "node16",
            "moduleResolution": "node16",
            "strict": false,
            "noEmit": true,
            "types": []
          },
          "files": ["esm.mts", "cjs.cts"]
        }"#,
    );

    let args = parse_args(&["tsz", "--noEmit"]);
    let result = compile(&args, base).expect("compile should succeed");

    let unresolved = diagnostics_with_code(&result.diagnostics, CANNOT_FIND_MODULE);
    assert_eq!(
        unresolved.len(),
        1,
        "the ESM-mode dynamic import must keep its own TS2307, got: {unresolved:#?}"
    );
    assert!(
        unresolved[0].message_text.contains("require-only-pkg"),
        "TS2307 names the require-only package, got: {unresolved:#?}"
    );
}
