//! Tests for TS1362 false positives when export type merges with namespace export.
//!
//! When `export type X = ...` merges with `export * as X from "..."`, the merged
//! symbol provides both type and value meaning. Using X as a value should NOT
//! trigger TS1362 ("cannot be used as a value because it was exported using
//! 'export type'").

use tsz_checker::context::CheckerOptions;
use tsz_common::common::ModuleKind;

fn compile_module_files(files: &[(&str, &str)], entry_idx: usize) -> Vec<(u32, String)> {
    let entry_file = files[entry_idx].0;
    tsz_checker::test_utils::check_multi_file(
        files,
        entry_file,
        CheckerOptions {
            module: ModuleKind::CommonJS,
            strict: true,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .filter(|d| d.code != 2318)
    .map(|d| (d.code, d.message_text))
    .collect()
}

/// Reproduces typeAndNamespaceExportMerge.ts:
/// `export type Drink = 0 | 1` merged with `export * as Drink from "./constants"`
/// tsc expects NO errors; we should not emit TS1362.
#[test]
fn no_ts1362_for_type_and_namespace_export_merge() {
    let constants = r#"
export const COFFEE = 0;
export const TEA = 1;
"#;
    let drink = r#"
export type Drink = 0 | 1;
export * as Drink from "./constants";
"#;
    let index = r#"
import { Drink } from "./drink";
const x: Drink = Drink.TEA;
"#;
    let diagnostics = compile_module_files(
        &[
            ("./constants.ts", constants),
            ("./drink.ts", drink),
            ("./index.ts", index),
        ],
        2,
    );
    let ts1362 = diagnostics
        .iter()
        .filter(|(c, _)| *c == 1362)
        .collect::<Vec<_>>();
    assert!(
        ts1362.is_empty(),
        "Should not emit TS1362 when export type merges with namespace export. Got: {ts1362:?}. All: {diagnostics:?}"
    );
}

/// Reproduces exportTypeMergedWithExportStarAsNamespace.ts
#[test]
fn no_ts1362_for_export_type_merged_with_export_star_as_namespace() {
    let something = r#"
export type Something<A> = { value: A }
export type SubType<A> = { value: A }
export declare function of<A>(value: A): Something<A>
"#;
    let prelude = r#"
import * as S from "./Something"
export * as Something from "./Something"
export type Something<A> = S.Something<A>
"#;
    let usage = r#"
import { Something } from "./prelude"
export const myValue: Something<string> = Something.of("abc")
export type MyType = Something.SubType<string>
"#;
    let diagnostics = compile_module_files(
        &[
            ("./Something.ts", something),
            ("./prelude.ts", prelude),
            ("./usage.ts", usage),
        ],
        2,
    );
    let ts1362 = diagnostics
        .iter()
        .filter(|(c, _)| *c == 1362)
        .collect::<Vec<_>>();
    assert!(
        ts1362.is_empty(),
        "Should not emit TS1362 when export type merges with export * as namespace. Got: {ts1362:?}. All: {diagnostics:?}"
    );
}

/// Reproduces importElisionConstEnumMerge1.ts
#[test]
fn no_ts1362_for_import_merged_with_namespace_then_reexported() {
    let enum_file = r#"
export const enum Enum {
  One = 1,
}
"#;
    let merge = r#"
import { Enum } from "./enum";
namespace Enum {
  export type Foo = number;
}
export { Enum };
"#;
    let index = r#"
import { Enum } from "./merge";
Enum.One;
"#;
    let diagnostics = compile_module_files(
        &[
            ("./enum.ts", enum_file),
            ("./merge.ts", merge),
            ("./index.ts", index),
        ],
        2,
    );
    let ts1362 = diagnostics
        .iter()
        .filter(|(c, _)| *c == 1362)
        .collect::<Vec<_>>();
    assert!(
        ts1362.is_empty(),
        "Should not emit TS1362 when imported const enum is merged with namespace and re-exported. Got: {ts1362:?}. All: {diagnostics:?}"
    );
}

/// Reproduces noCrashOnImportShadowing.ts:
/// `import * as B` merged with `interface B`, then `export { B }`.
/// The namespace import provides value meaning despite the interface merge.
/// NOTE: This passes in the full parallel pipeline (conformance test still fails
/// due to per-file binder differences in alias resolution for namespace imports
/// merged with interfaces). Ignored until full-pipeline unit test infra is available.
#[test]
fn no_ts1362_for_namespace_import_merged_with_interface_then_reexported() {
    let b = r#"
export const zzz = 123;
"#;
    let a = r#"
import * as B from "./b";
interface B { x: string; }
const x: B = { x: "" };
B.zzz;
export { B };
"#;
    let index = r#"
import { B } from "./a";
const x: B = { x: "" };
B.zzz;
"#;
    let diagnostics =
        compile_module_files(&[("./b.ts", b), ("./a.ts", a), ("./index.ts", index)], 2);
    let ts1362 = diagnostics
        .iter()
        .filter(|(c, _)| *c == 1362)
        .collect::<Vec<_>>();
    assert!(
        ts1362.is_empty(),
        "Should not emit TS1362 when namespace import merged with interface is re-exported. Got: {ts1362:?}. All: {diagnostics:?}"
    );
}

#[test]
fn imported_interface_const_merge_uses_value_side_for_property_access() {
    let node = r#"
import { IdentifierNode } from "./identifier.js";

export interface ColumnNode {
  readonly kind: 'ColumnNode';
  readonly column: IdentifierNode;
}

type ColumnNodeFactory = Readonly<{
  create(column: string): Readonly<ColumnNode>;
}>;

export const ColumnNode: ColumnNodeFactory = {
  create(column) {
    return {
      kind: 'ColumnNode',
      column: IdentifierNode.create(column),
    };
  },
};
"#;
    let identifier = r#"
export interface IdentifierNode {
  readonly kind: 'IdentifierNode';
  readonly name: string;
}

type IdentifierNodeFactory = Readonly<{
  create(name: string): Readonly<IdentifierNode>;
}>;

export const IdentifierNode: IdentifierNodeFactory = {
  create(column) {
    return { kind: 'IdentifierNode', name: column };
  },
};
"#;
    let lib_files = tsz_checker::test_utils::load_lib_files(&["es5.d.ts"]);
    let diagnostics = tsz_checker::test_utils::check_multi_file_with_libs(
        &[("./node.ts", node), ("./identifier.ts", identifier)],
        "./node.ts",
        CheckerOptions {
            module: ModuleKind::CommonJS,
            strict: true,
            ..CheckerOptions::default()
        },
        &lib_files,
    )
    .into_iter()
    .filter(|d| d.code != 2318)
    .map(|d| (d.code, d.message_text))
    .collect::<Vec<_>>();
    let ts2339 = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2339)
        .collect::<Vec<_>>();
    assert!(
        ts2339.is_empty(),
        "Imported interface+const merge should use the const value side in expression context. Got: {ts2339:?}. All: {diagnostics:?}"
    );
}
