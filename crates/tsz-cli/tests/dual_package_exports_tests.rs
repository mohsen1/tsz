//! Regression tests for dual-package `exports` resolution under Node16/NodeNext
//! and `Bundler` resolution.
//!
//! Structural rule:
//! When a package author writes a `package.json` with a `"types"` field, a
//! top-level `"types"` condition, or a nested `{ "types": ... }` branch inside
//! `"import"`/`"require"`, every successful resolution must produce a single
//! canonical declaration file per importer mode. Repeated lookups for the same
//! `(importer, specifier)` must converge on a stable resolved path, and ESM
//! and CJS importers that share a single types entry must reach the same
//! physical file rather than two declaration siblings.
//!
//! Owner layer: `crates/tsz-core/src/module_resolver/exports_imports.rs`
//! (conditional-export traversal) and
//! `crates/tsz-cli/src/driver/resolution/path_resolution.rs`
//! (path normalization + duplicate-package redirect map).
//!
//! These tests drive the full CLI compile pipeline so they exercise the same
//! resolver, source-discovery, binder, and checker paths as a real `tsz` run.

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

/// Diagnostic codes that surface when one logical module ends up bound under
/// two declaration IDs. Used by every dual-package test in this file to
/// assert the resolver kept a single canonical identity per `(importer,
/// specifier)`.
const DUPLICATE_DECLARATION_CODES: [u32; 4] = [2300, 2308, 2484, 2649];

fn assert_no_duplicate_declaration_diags(diagnostics: &[Diagnostic], context: &str) {
    let duplicates: Vec<_> = diagnostics
        .iter()
        .filter(|diag| DUPLICATE_DECLARATION_CODES.contains(&diag.code))
        .collect();
    assert!(
        duplicates.is_empty(),
        "{context}: dual-package resolution must keep one declaration identity, got: {duplicates:#?}"
    );
}

/// Write a Node16/NodeNext-flavored `tsconfig.json` that drives the dual-package
/// resolver paths the tests below exercise. `module_kind` is the value used for
/// both the `module` and `moduleResolution` fields, matching how tsc treats
/// node16 and nodenext as a paired switch.
fn write_node16_tsconfig(base: &Path, module_kind: &str, files: &[&str]) {
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
            "module": "{module_kind}",
            "moduleResolution": "{module_kind}",
            "target": "es2022",
            "strict": true,
            "noEmit": true
          }},
          "files": [{files_json}]
        }}"#
        ),
    );
}

/// When a dual-package author lists `"types"` ahead of `"import"`/`"require"`
/// inside a single `exports` conditional, every importer (ESM `.mts`, CJS
/// `.cts`, neutral `.ts`) must resolve to the single shared declaration file.
/// No duplicate-symbol diagnostic (TS2300, TS2308, TS2484, TS2649) may fire,
/// and no TS2305 (no exported member) may surface for the named imports.
#[test]
fn dual_package_shared_types_condition_resolves_once_for_every_module_kind() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let base = temp.path();

    write_node16_tsconfig(base, "node16", &["main.mts", "main.cts", "main.ts"]);

    write_file(
        &base.join("node_modules/shared-pkg/package.json"),
        r#"{
          "name": "shared-pkg",
          "version": "1.0.0",
          "type": "module",
          "exports": {
            ".": {
              "types": "./shared/index.d.ts",
              "import": "./esm/index.js",
              "require": "./cjs/index.cjs"
            }
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/shared-pkg/shared/index.d.ts"),
        "export interface Greeter { greet(): string; }\nexport declare const banner: \"shared\";\n",
    );
    write_file(
        &base.join("node_modules/shared-pkg/esm/index.js"),
        "export const banner = 'shared';\nexport class Impl { greet() { return banner; } }\n",
    );
    write_file(
        &base.join("node_modules/shared-pkg/cjs/index.cjs"),
        "exports.banner = 'shared';\nexports.Impl = class { greet() { return exports.banner; } };\n",
    );

    let importer = "import { banner, Greeter } from 'shared-pkg';\nconst _b: typeof banner = 'shared';\nexport const value: Greeter = { greet: () => _b };\n";
    write_file(&base.join("main.mts"), importer);
    write_file(&base.join("main.cts"), importer);
    write_file(&base.join("main.ts"), importer);

    let args = parse_args(&["tsz", "--noEmit"]);
    let result = compile(&args, base).expect("compile should succeed");

    assert_no_duplicate_declaration_diags(&result.diagnostics, "shared types entry");

    let missing_member: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|diag| diag.code == 2305 && diag.message_text.contains("shared-pkg"))
        .collect();
    assert!(
        missing_member.is_empty(),
        "shared types entry must expose `banner`/`Greeter` to every importer, got: {missing_member:#?}"
    );
}

/// When a package places the `"types"` condition INSIDE each
/// `"import"`/`"require"` branch and the two branches point at distinct
/// declaration files, an ESM importer must select the import-branch types and
/// a CJS importer must select the require-branch types. Each branch is its
/// own module — neither importer may end up loading both branches' declaration
/// files, and no spurious TS2300/TS2305 may surface from the conditional fan
/// out.
#[test]
fn dual_package_nested_types_branches_route_per_importer_mode_without_dup() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let base = temp.path();

    write_node16_tsconfig(base, "node16", &["esm-side.mts", "cjs-side.cts"]);

    write_file(
        &base.join("node_modules/split-pkg/package.json"),
        r#"{
          "name": "split-pkg",
          "version": "1.0.0",
          "type": "module",
          "exports": {
            ".": {
              "import": {
                "types": "./esm/index.d.ts",
                "default": "./esm/index.js"
              },
              "require": {
                "types": "./cjs/index.d.cts",
                "default": "./cjs/index.cjs"
              }
            }
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/split-pkg/esm/index.d.ts"),
        "export declare const flavor: 'esm';\nexport declare function describe(): 'esm';\n",
    );
    write_file(
        &base.join("node_modules/split-pkg/esm/index.js"),
        "export const flavor = 'esm';\nexport function describe() { return flavor; }\n",
    );
    write_file(
        &base.join("node_modules/split-pkg/cjs/index.d.cts"),
        "export declare const flavor: 'cjs';\nexport declare function describe(): 'cjs';\n",
    );
    write_file(
        &base.join("node_modules/split-pkg/cjs/index.cjs"),
        "exports.flavor = 'cjs';\nexports.describe = () => exports.flavor;\n",
    );

    write_file(
        &base.join("esm-side.mts"),
        "import { flavor, describe } from 'split-pkg';\nconst _f: 'esm' = flavor;\nexport const out = describe();\n",
    );
    write_file(
        &base.join("cjs-side.cts"),
        "import { flavor, describe } from 'split-pkg';\nconst _f: 'cjs' = flavor;\nexport const out = describe();\n",
    );

    let args = parse_args(&["tsz", "--noEmit"]);
    let result = compile(&args, base).expect("compile should succeed");

    assert_no_duplicate_declaration_diags(&result.diagnostics, "per-mode types branches");

    let no_exported_member: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|diag| diag.code == 2305)
        .collect();
    assert!(
        no_exported_member.is_empty(),
        "per-mode types branches must each expose `flavor` and `describe`, got TS2305: {no_exported_member:#?}"
    );

    let flavor_conflict: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|diag| diag.code == 2322 && diag.message_text.contains("flavor"))
        .collect();
    assert!(
        flavor_conflict.is_empty(),
        "ESM and CJS branches must each return their own `flavor` literal, got: {flavor_conflict:#?}"
    );
}

/// Repeated imports of the same dual-package specifier from a single file must
/// converge on one resolved declaration ID. The structural guarantee here is
/// that the resolver caches `(importer, specifier)` lookups deterministically;
/// regressing it would re-bind the same module multiple times and surface as
/// duplicate-identifier diagnostics or shifting `resolvedModule` paths.
#[test]
fn dual_package_repeated_import_is_idempotent() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let base = temp.path();

    write_node16_tsconfig(base, "node16", &["main.mts"]);

    write_file(
        &base.join("node_modules/repeat-pkg/package.json"),
        r#"{
          "name": "repeat-pkg",
          "version": "1.0.0",
          "type": "module",
          "main": "./esm/index.js",
          "types": "./esm/index.d.ts",
          "exports": {
            ".": {
              "types": "./esm/index.d.ts",
              "import": "./esm/index.js"
            }
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/repeat-pkg/esm/index.d.ts"),
        "export declare const tag: 'repeat';\nexport interface Shape { kind: 'repeat'; }\n",
    );
    write_file(
        &base.join("node_modules/repeat-pkg/esm/index.js"),
        "export const tag = 'repeat';\n",
    );

    write_file(
        &base.join("main.mts"),
        r#"import { tag, Shape } from 'repeat-pkg';
import type { Shape as Shape2 } from 'repeat-pkg';
import { tag as tag2 } from 'repeat-pkg';

const _a: Shape = { kind: tag };
const _b: Shape2 = { kind: tag2 };
export const out = _a.kind === _b.kind;
"#,
    );

    let args = parse_args(&["tsz", "--noEmit"]);
    let result = compile(&args, base).expect("compile should succeed");

    assert_no_duplicate_declaration_diags(&result.diagnostics, "repeated dual-package imports");

    let shape_errors: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|diag| {
            diag.code == 2322
                || diag.code == 2345
                || (diag.code == 2305 && diag.message_text.contains("repeat-pkg"))
        })
        .collect();
    assert!(
        shape_errors.is_empty(),
        "`Shape` and `Shape2` must refer to the same declaration, got: {shape_errors:#?}"
    );
}

/// `Bundler` resolution shares the same `resolve_package_exports_with_conditions`
/// traversal as Node16/NodeNext, so the dual-package guarantees must also hold
/// when the importer's effective resolution kind is `bundler`. This guards
/// against regressions where bundler mode silently drops a `types` branch and
/// falls back to probing a sibling `.d.ts` next to the runtime entry, creating
/// a second declaration file for a single module specifier.
#[test]
fn dual_package_bundler_resolution_keeps_single_declaration_identity() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let base = temp.path();

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "module": "preserve",
            "moduleResolution": "bundler",
            "target": "es2022",
            "strict": true,
            "noEmit": true
          },
          "files": ["main.ts"]
        }"#,
    );

    write_file(
        &base.join("node_modules/bundler-pkg/package.json"),
        r#"{
          "name": "bundler-pkg",
          "version": "1.0.0",
          "exports": {
            ".": {
              "types": "./types/index.d.ts",
              "import": "./dist/index.mjs",
              "require": "./dist/index.cjs",
              "default": "./dist/index.js"
            }
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/bundler-pkg/types/index.d.ts"),
        "export declare const id: number;\nexport interface Wrapped { id: number }\n",
    );
    // Sibling .d.ts files next to runtime entries. The resolver must NOT pick
    // these up — picking either would produce a second declaration identity
    // that collides with the canonical `types/index.d.ts`.
    write_file(
        &base.join("node_modules/bundler-pkg/dist/index.d.ts"),
        "export declare const id: string;\nexport interface Wrapped { id: string }\n",
    );
    write_file(
        &base.join("node_modules/bundler-pkg/dist/index.d.mts"),
        "export declare const id: string;\nexport interface Wrapped { id: string }\n",
    );
    write_file(
        &base.join("node_modules/bundler-pkg/dist/index.d.cts"),
        "export declare const id: string;\nexport interface Wrapped { id: string }\n",
    );
    write_file(
        &base.join("node_modules/bundler-pkg/dist/index.mjs"),
        "export const id = 1;\n",
    );
    write_file(
        &base.join("node_modules/bundler-pkg/dist/index.cjs"),
        "exports.id = 1;\n",
    );
    write_file(
        &base.join("node_modules/bundler-pkg/dist/index.js"),
        "export const id = 1;\n",
    );

    write_file(
        &base.join("main.ts"),
        r#"import { id, Wrapped } from 'bundler-pkg';
const _n: number = id;
export const out: Wrapped = { id };
"#,
    );

    let args = parse_args(&["tsz", "--noEmit"]);
    let result = compile(&args, base).expect("compile should succeed");

    assert_no_duplicate_declaration_diags(&result.diagnostics, "bundler-mode dual-package");

    // The `types` branch (a `number`) must win over any sibling `.d.ts` (a
    // `string`). If the resolver drifted to a sibling, `_n: number = id` would
    // surface TS2322.
    let type_mismatch: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|diag| {
            (diag.code == 2322 || diag.code == 2345)
                && (diag.file.ends_with("main.ts") || diag.file.contains("main.ts"))
        })
        .collect();
    assert!(
        type_mismatch.is_empty(),
        "bundler-mode dual-package must use the `types` branch (number), got: {type_mismatch:#?}"
    );
}

/// Subpath dual-package exports (`./feature`) must also converge on a single
/// declaration file per importer mode. This guards against the failure mode
/// where the subpath probe falls through to a `typesVersions` fallback after
/// already resolving an exports-map subpath, causing the same declaration to
/// be reached via two distinct paths and bound twice.
#[test]
fn dual_package_subpath_resolution_keeps_single_declaration_identity() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let base = temp.path();

    write_node16_tsconfig(base, "node16", &["main.mts", "main.cts"]);

    write_file(
        &base.join("node_modules/sub-pkg/package.json"),
        r#"{
          "name": "sub-pkg",
          "version": "1.0.0",
          "type": "module",
          "exports": {
            "./feature": {
              "types": "./types/feature.d.ts",
              "import": "./esm/feature.js",
              "require": "./cjs/feature.cjs"
            }
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/sub-pkg/types/feature.d.ts"),
        "export declare function compute(value: number): number;\nexport interface Feature { compute: typeof compute }\n",
    );
    write_file(
        &base.join("node_modules/sub-pkg/esm/feature.js"),
        "export function compute(value) { return value + 1; }\n",
    );
    write_file(
        &base.join("node_modules/sub-pkg/cjs/feature.cjs"),
        "exports.compute = (value) => value + 1;\n",
    );

    let importer = "import { compute, Feature } from 'sub-pkg/feature';\nconst _x: number = compute(1);\nexport const out: Feature = { compute };\n";
    write_file(&base.join("main.mts"), importer);
    write_file(&base.join("main.cts"), importer);

    let args = parse_args(&["tsz", "--noEmit"]);
    let result = compile(&args, base).expect("compile should succeed");

    assert_no_duplicate_declaration_diags(&result.diagnostics, "subpath dual-package exports");

    let missing: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|diag| diag.code == 2305)
        .collect();
    assert!(
        missing.is_empty(),
        "subpath dual-package exports must expose `compute`/`Feature`, got TS2305: {missing:#?}"
    );
}

/// pnpm-style layout: a single physical declaration file under
/// `node_modules/.pnpm/<pkg>@<ver>/node_modules/<pkg>/` is symlinked into the
/// project's `node_modules/<pkg>/`. Both forms of the path must resolve to
/// the same logical declaration identity. Without correct symlink handling
/// the two paths become two file indices and the shared exports get bound
/// twice, surfacing as duplicate-declaration diagnostics or as TS2322 when
/// a single `Shape` value is assigned through both views.
#[cfg(unix)]
#[test]
fn dual_package_pnpm_style_symlink_keeps_single_declaration_identity() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::TempDir::new().expect("temp dir");
    let base = temp.path();

    let real_pkg = base.join("node_modules/.pnpm/sym-pkg@1.0.0/node_modules/sym-pkg");
    std::fs::create_dir_all(&real_pkg).expect("create real pkg dir");
    let real_pkg_esm = real_pkg.join("esm");
    std::fs::create_dir_all(&real_pkg_esm).expect("create esm dir");

    write_file(
        &real_pkg.join("package.json"),
        r#"{
          "name": "sym-pkg",
          "version": "1.0.0",
          "type": "module",
          "exports": {
            ".": {
              "types": "./esm/index.d.ts",
              "import": "./esm/index.js"
            }
          }
        }"#,
    );
    write_file(
        &real_pkg_esm.join("index.d.ts"),
        "export interface Shape { kind: 'pnpm' }\nexport declare const tag: 'pnpm';\n",
    );
    write_file(
        &real_pkg_esm.join("index.js"),
        "export const tag = 'pnpm';\n",
    );

    std::fs::create_dir_all(base.join("node_modules")).expect("create node_modules");
    symlink(&real_pkg, base.join("node_modules/sym-pkg")).expect("create symlink");

    write_node16_tsconfig(base, "node16", &["main.mts", "extra.mts"]);

    // Two independent importers that resolve the same dual-package specifier.
    // If the symlinked and real paths diverge into two file indices, the same
    // `Shape` interface gets bound twice and the round-trip assignment in
    // `extra.mts` surfaces TS2322 against the foreign `Shape`.
    write_file(
        &base.join("main.mts"),
        "import { Shape, tag } from 'sym-pkg';\nexport const value: Shape = { kind: tag };\n",
    );
    write_file(
        &base.join("extra.mts"),
        "import { Shape, tag } from 'sym-pkg';\nimport { value } from './main.mjs';\nexport const echoed: Shape = { kind: value.kind };\nexport const _t: 'pnpm' = tag;\n",
    );

    let args = parse_args(&["tsz", "--noEmit"]);
    let result = compile(&args, base).expect("compile should succeed");

    assert_no_duplicate_declaration_diags(&result.diagnostics, "pnpm-symlinked dual-package");

    let shape_mismatch: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|diag| diag.code == 2322 || diag.code == 2345)
        .collect();
    assert!(
        shape_mismatch.is_empty(),
        "pnpm-symlinked dual-package must keep one `Shape` identity, got: {shape_mismatch:#?}"
    );
}
