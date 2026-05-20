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

#[test]
fn imported_interface_const_merge_uses_type_side_for_annotations() {
    let node = r#"
export interface ItemNode {
  readonly kind: 'ItemNode';
  readonly id: string;
}

type ItemNodeFactory = Readonly<{
  create(id: string): Readonly<ItemNode>;
}>;

export const ItemNode: ItemNodeFactory = {
  create(id) {
    return { kind: 'ItemNode', id };
  },
};
"#;
    let usage = r#"
import { ItemNode } from "./node.js";

export interface Holder {
  readonly node: ItemNode;
}

const node: ItemNode = ItemNode.create('id');
const holder: Holder = { node };
"#;
    let lib_files = tsz_checker::test_utils::load_lib_files(&["es5.d.ts"]);
    let diagnostics = tsz_checker::test_utils::check_multi_file_with_libs(
        &[("./node.ts", node), ("./usage.ts", usage)],
        "./usage.ts",
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
    let relevant = diagnostics
        .iter()
        .filter(|(code, _)| matches!(*code, 2322 | 2345 | 2739 | 2740))
        .collect::<Vec<_>>();
    assert!(
        relevant.is_empty(),
        "Imported interface+const merge should use the interface type side in annotations. Got: {relevant:?}. All: {diagnostics:?}"
    );
}

#[test]
fn imported_class_type_alias_callback_uses_instance_side() {
    let node = r#"
export interface ColumnDefinitionNode {
  readonly kind: 'ColumnDefinitionNode';
  readonly column: string;
}

type ColumnDefinitionNodeFactory = Readonly<{
  create(column: string): Readonly<ColumnDefinitionNode>;
}>;

export const ColumnDefinitionNode: ColumnDefinitionNodeFactory = {
  create(column) {
    return { kind: 'ColumnDefinitionNode', column };
  },
};
"#;
    let builder = r#"
import { ColumnDefinitionNode } from './node.js';

export class ColumnDefinitionBuilder {
  readonly #node: ColumnDefinitionNode;

  constructor(node: ColumnDefinitionNode) {
    this.#node = node;
  }

  toOperationNode(): string {
    return this.#node.column;
  }
}

export type ColumnDefinitionBuilderCallback = (
  builder: ColumnDefinitionBuilder,
) => ColumnDefinitionBuilder;
"#;
    let usage = r#"
import {
  ColumnDefinitionBuilder,
  type ColumnDefinitionBuilderCallback,
} from './builder.js';
import { ColumnDefinitionNode } from './node.js';

const noop = <T>(obj: T): T => obj;

export function useCallback(build: ColumnDefinitionBuilderCallback = noop): string {
  const builder = build(new ColumnDefinitionBuilder(ColumnDefinitionNode.create('id')));
  return builder.toOperationNode();
}
"#;
    let lib_files = tsz_checker::test_utils::load_lib_files(&["es5.d.ts"]);
    let diagnostics = tsz_checker::test_utils::check_multi_file_with_libs(
        &[
            ("./node.ts", node),
            ("./builder.ts", builder),
            ("./usage.ts", usage),
        ],
        "./usage.ts",
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
    let relevant = diagnostics
        .iter()
        .filter(|(code, _)| matches!(*code, 2322 | 2339 | 2345))
        .collect::<Vec<_>>();
    assert!(
        relevant.is_empty(),
        "Imported type alias callback should use class instance side, not the constructor side. Got: {relevant:?}. All: {diagnostics:?}"
    );
}

#[test]
fn cross_file_lowering_cache_is_scoped_by_requesting_file() {
    let a = r#"
export class Builder {
  a(): string {
    return 'a';
  }
}
"#;
    let bbase = r#"
export class Builder {
  b(): string {
    return 'b';
  }
}

export function createBuilder(): Builder {
  return new Builder();
}
"#;
    let callback = r#"
import { Builder } from './bbase.js';

export type Callback = (builder: Builder) => Builder;
"#;
    let usage = r#"
import { Builder } from './a.js';
import { createBuilder } from './bbase.js';
import type { Callback } from './callback.js';

type CachedInEntryFile = Builder;
const useCached: CachedInEntryFile = new Builder();
useCached.a();

const identity: Callback = (builder) => builder;
const out = identity(createBuilder());
out.b();
"#;
    let lib_files = tsz_checker::test_utils::load_lib_files(&["es5.d.ts"]);
    let diagnostics = tsz_checker::test_utils::check_multi_file_with_libs(
        &[
            ("./a.ts", a),
            ("./bbase.ts", bbase),
            ("./callback.ts", callback),
            ("./usage.ts", usage),
        ],
        "./usage.ts",
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
    let relevant = diagnostics
        .iter()
        .filter(|(code, _)| matches!(*code, 2322 | 2339 | 2345))
        .collect::<Vec<_>>();
    assert!(
        relevant.is_empty(),
        "Text-based lowering cache entries must not leak between files with same-named imports. Got: {relevant:?}. All: {diagnostics:?}"
    );
}

#[test]
fn imported_interface_const_merge_in_same_file_props_uses_type_side() {
    let node = r#"
export interface AlterTableNode {
  readonly kind: 'AlterTableNode';
  readonly table: string;
}

type AlterTableNodeFactory = Readonly<{
  cloneWithTableProps(node: AlterTableNode, props: { table?: string }): Readonly<AlterTableNode>;
}>;

export const AlterTableNode: AlterTableNodeFactory = {
  cloneWithTableProps(node, props) {
    return { ...node, ...props };
  },
};
"#;
    let usage = r#"
import { AlterTableNode } from './node.js';

export interface AlterTableBuilderProps {
  readonly node: AlterTableNode;
}

export class AlterTableBuilder {
  readonly props: AlterTableBuilderProps;

  constructor(props: AlterTableBuilderProps) {
    this.props = props;
  }

  renameTo(table: string): Readonly<AlterTableNode> {
    return AlterTableNode.cloneWithTableProps(this.props.node, { table });
  }
}
"#;
    let lib_files = tsz_checker::test_utils::load_lib_files(&["es5.d.ts"]);
    let diagnostics = tsz_checker::test_utils::check_multi_file_with_libs(
        &[("./node.ts", node), ("./usage.ts", usage)],
        "./usage.ts",
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
    let relevant = diagnostics
        .iter()
        .filter(|(code, _)| matches!(*code, 2322 | 2345 | 2739 | 2740))
        .collect::<Vec<_>>();
    assert!(
        relevant.is_empty(),
        "Imported interface+const merge should keep type-side props as the interface. Got: {relevant:?}. All: {diagnostics:?}"
    );
}
