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
    let ts2709 = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2709)
        .collect::<Vec<_>>();
    assert!(
        ts2709.is_empty(),
        "Should not emit TS2709 when export type merges with namespace export. Got: {ts2709:?}. All: {diagnostics:?}"
    );
}

/// Type+value merged symbol is usable through a wildcard barrel.
///
/// When a file has both `export * as NS from './mod'` and `export type NS = ...`,
/// and a barrel does `export * from './that-file'`, the imported symbol must still
/// provide both type meaning (NS<A>) and value meaning (NS.of(...)).
#[test]
fn no_errors_for_type_namespace_merge_through_wildcard_barrel() {
    let something = r#"
export type Something<A> = { value: A }
export declare function of<A>(value: A): Something<A>
"#;
    let prelude = r#"
import * as S from "./Something"
export * as Something from "./Something"
export type Something<A> = S.Something<A>
"#;
    let barrel = "export * from \"./prelude\";\n";
    let usage = r#"
import { Something } from "./barrel"
const _myValue: Something<string> = Something.of("abc")
"#;
    let diagnostics = compile_module_files(
        &[
            ("./Something.ts", something),
            ("./prelude.ts", prelude),
            ("./barrel.ts", barrel),
            ("./usage.ts", usage),
        ],
        3,
    );
    let ts1362 = diagnostics
        .iter()
        .filter(|(c, _)| *c == 1362)
        .collect::<Vec<_>>();
    assert!(
        ts1362.is_empty(),
        "Should not emit TS1362 for type+namespace merge through wildcard barrel. Got: {ts1362:?}. All: {diagnostics:?}"
    );
    let ts2305 = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2305)
        .collect::<Vec<_>>();
    assert!(
        ts2305.is_empty(),
        "Should not emit TS2305 for type+namespace merge through wildcard barrel. Got: {ts2305:?}. All: {diagnostics:?}"
    );
    let ts2339 = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2339)
        .collect::<Vec<_>>();
    assert!(
        ts2339.is_empty(),
        "Should not emit TS2339 for type+namespace merge through wildcard barrel. Got: {ts2339:?}. All: {diagnostics:?}"
    );
    let ts2709 = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2709)
        .collect::<Vec<_>>();
    assert!(
        ts2709.is_empty(),
        "Should not emit TS2709 for type+namespace merge through wildcard barrel. Got: {ts2709:?}. All: {diagnostics:?}"
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
    let ts2709 = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2709)
        .collect::<Vec<_>>();
    assert!(
        ts2709.is_empty(),
        "Should not emit TS2709 when export type merges with export-star namespace. Got: {ts2709:?}. All: {diagnostics:?}"
    );
}

/// `export default ImportedValue` preserves the imported value type through
/// `import * as ns`; the synthetic default wrapper must not become the module
/// namespace object.
#[test]
fn namespace_default_export_of_named_import_keeps_value_type() {
    let ctor = r#"
export interface Ctor {
    x: number;
}
export type ExtendedCtor<T> = { x: number, ext: T };
export interface CtorConstructor {
    extends<T>(x: T): ExtendedCtor<T extends unknown ? Ctor : undefined>;
}
export const Ctor: CtorConstructor;
"#;
    let index_dts = r#"
import { Ctor } from "./ctor";
export default Ctor;
"#;
    let usage = r#"
import * as ns from "mod";
const Ctor = ns.default;
export const MyComp = Ctor.extends({ foo: "bar" });
"#;
    let diagnostics = compile_module_files(
        &[
            ("./node_modules/mod/ctor.d.ts", ctor),
            ("./node_modules/mod/index.d.ts", index_dts),
            ("./index.ts", usage),
        ],
        2,
    );
    let ts2339 = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2339)
        .collect::<Vec<_>>();
    assert!(
        ts2339.is_empty(),
        "Should not emit TS2339 when namespace default is a named imported value. Got: {ts2339:?}. All: {diagnostics:?}"
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

/// Verify that a type-alias-only export (no matching value) through a wildcard
/// barrel correctly produces TS1362 when used as a value. This is the control
/// case that proves we *do* correctly detect type-only symbols.
#[test]
fn ts1362_for_type_only_through_wildcard_barrel() {
    let types_file = r#"
export type Config = { debug: boolean };
"#;
    let barrel = r#"
export * from "./types";
"#;
    let usage = r#"
import { Config } from "./barrel";
Config.debug;
"#;
    let diagnostics = compile_module_files(
        &[
            ("./types.ts", types_file),
            ("./barrel.ts", barrel),
            ("./usage.ts", usage),
        ],
        2,
    );
    // Config is only a type, so using it as a value should produce TS2693 or
    // TS1362 (or similar type-used-as-value diagnostic). It must NOT be silent.
    let has_type_usage_error = diagnostics
        .iter()
        .any(|(c, _)| [1362u32, 2693, 2339, 2448].contains(c));
    assert!(
        has_type_usage_error,
        "Should emit a type-used-as-value error for type-only wildcard export. Got: {diagnostics:?}"
    );
}

/// When two `export *` sources each provide the same name — one as a type-only
/// re-export and one as a real value — the resolved binding must be the value.
/// tsc sees no ambiguity because one is type-only; tsz must not emit TS1362 or
/// TS2308 (ambiguous re-export) for that case.
#[test]
fn no_ts1362_for_type_value_split_across_wildcard_sources() {
    // Use different shapes so we can tell which one is actually used at the
    // value site: value has `{ count: number }`, type has `{ debug: boolean }`.
    let types_file = r#"
export type Config = { debug: boolean };
"#;
    let values_file = r#"
export const Config = { count: 42 };
"#;
    let barrel = r#"
export * from "./types";
export * from "./values";
"#;
    // Accessing .count — only valid if the VALUE (not the type alias) is used.
    // Accessing as type Config — only valid if the TYPE alias is used in type position.
    // tsc resolves value-position `Config` to the VALUE (from values.ts), not the TYPE.
    let usage = r#"
import { Config } from "./barrel";
const _n: number = Config.count;
"#;
    let diagnostics = compile_module_files(
        &[
            ("./types.ts", types_file),
            ("./values.ts", values_file),
            ("./barrel.ts", barrel),
            ("./usage.ts", usage),
        ],
        3,
    );
    let ts1362 = diagnostics
        .iter()
        .filter(|(c, _)| *c == 1362)
        .collect::<Vec<_>>();
    assert!(
        ts1362.is_empty(),
        "Should not emit TS1362 when type and value for same name come from separate wildcard sources. Got: {ts1362:?}. All: {diagnostics:?}"
    );
    let ts2308 = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2308)
        .collect::<Vec<_>>();
    assert!(
        ts2308.is_empty(),
        "Should not emit TS2308 for type+value from separate wildcard sources. Got: {ts2308:?}. All: {diagnostics:?}"
    );
    let ts2339 = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2339)
        .collect::<Vec<_>>();
    assert!(
        ts2339.is_empty(),
        "Should not emit TS2339 for .count access when VALUE provides 'count' (types-first barrel). Got: {ts2339:?}. All: {diagnostics:?}"
    );
}

/// `export type *` makes even value-bearing declarations type-only along that
/// path. A later value wildcard source with the same name must win in value
/// context.
#[test]
fn no_ts1362_for_type_only_wildcard_value_source_before_value_source() {
    let classes_file = r#"
export class Config {}
"#;
    let values_file = r#"
export const Config = { count: 42 };
"#;
    let barrel = r#"
export type * from "./classes";
export * from "./values";
"#;
    let usage = r#"
import { Config } from "./barrel";
const _n: number = Config.count;
"#;
    let diagnostics = compile_module_files(
        &[
            ("./classes.ts", classes_file),
            ("./values.ts", values_file),
            ("./barrel.ts", barrel),
            ("./usage.ts", usage),
        ],
        3,
    );
    let ts1362 = diagnostics
        .iter()
        .filter(|(c, _)| *c == 1362)
        .collect::<Vec<_>>();
    assert!(
        ts1362.is_empty(),
        "Should not emit TS1362 when an earlier type-only wildcard path points at a value-bearing declaration. Got: {ts1362:?}. All: {diagnostics:?}"
    );
    let ts2308 = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2308)
        .collect::<Vec<_>>();
    assert!(
        ts2308.is_empty(),
        "Should not emit TS2308 for type-only wildcard plus later value wildcard source. Got: {ts2308:?}. All: {diagnostics:?}"
    );
    let ts2339 = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2339)
        .collect::<Vec<_>>();
    assert!(
        ts2339.is_empty(),
        "Should not emit TS2339 for .count access when the later VALUE export provides 'count'. Got: {ts2339:?}. All: {diagnostics:?}"
    );
}
