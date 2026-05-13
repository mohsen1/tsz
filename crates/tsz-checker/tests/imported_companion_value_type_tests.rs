use tsz_checker::context::CheckerOptions;
use tsz_common::common::ModuleKind;

fn check(files: &[(&str, &str)], entry_file: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_multi_file(
        files,
        entry_file,
        CheckerOptions {
            module: ModuleKind::ESNext,
            strict: true,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .filter(|diagnostic| diagnostic.code != 2318)
    .map(|diagnostic| (diagnostic.code, diagnostic.message_text))
    .collect()
}

#[test]
fn imported_companion_interface_and_const_uses_value_side_for_property_access() {
    let diagnostics = check(
        &[
            (
                "a.ts",
                r#"
export interface CompiledQuery<O = unknown> {
  readonly sql: any
  readonly rows: O[]
}

type CompiledQueryFactory = Readonly<{
  raw(sql: any): CompiledQuery
}>

declare function freeze<T>(value: T): T

export const CompiledQuery: CompiledQueryFactory = freeze({
  raw(sql) {
    return { sql, rows: [] }
  },
})
"#,
            ),
            (
                "b.ts",
                r#"
import { CompiledQuery } from "./a.js"

const q = CompiledQuery.raw("select 1")
const sql: any = q.sql
"#,
            ),
        ],
        "b.ts",
    );

    assert!(
        diagnostics.is_empty(),
        "imported companion interface+const should resolve the value side in expression position, got: {diagnostics:?}"
    );
}

#[test]
fn imported_companion_const_keeps_readonly_alias_annotation_when_eval_fails() {
    let diagnostics = check(
        &[
            (
                "object-utils.ts",
                r#"
export function freeze<T>(value: T): Readonly<T> {
  return value
}
"#,
            ),
            (
                "a.ts",
                r#"
import { freeze } from "./object-utils.js"

export interface Box {
  value: string
}

export type BoxFactory = Readonly<{
  raw(): Box
}>

export const Box: BoxFactory = freeze<BoxFactory>({
  raw() {
    return { value: "x" }
  },
})
"#,
            ),
            (
                "b.ts",
                r#"
import { Box } from "./a.js"

const x = Box.raw()
const value: string = x.value
"#,
            ),
        ],
        "b.ts",
    );

    assert!(
        diagnostics.is_empty(),
        "imported companion const should preserve the Readonly alias annotation when eager alias evaluation cannot reduce it, got: {diagnostics:?}"
    );
}
#[test]
fn imported_companion_const_keeps_private_readonly_alias_annotation() {
    let diagnostics = tsz_checker::test_utils::check_multi_file_with_libs(
        &[
            (
                "object-utils.ts",
                r#"
export function freeze<T>(value: T): Readonly<T> {
  return value
}
"#,
            ),
            (
                "identifier-node.ts",
                r#"
import { freeze } from "./object-utils.js"

export interface IdentifierNode {
  readonly kind: "IdentifierNode"
  readonly name: string
}

type IdentifierNodeFactory = Readonly<{
  create(name: string): Readonly<IdentifierNode>
}>

export const IdentifierNode: IdentifierNodeFactory =
  freeze<IdentifierNodeFactory>({
    create(name) {
      return freeze({
        kind: "IdentifierNode",
        name,
      })
    },
  })
"#,
            ),
            (
                "column-node.ts",
                r#"
import { freeze } from "./object-utils.js"
import { IdentifierNode } from "./identifier-node.js"

export interface ColumnNode {
  readonly kind: "ColumnNode"
  readonly column: IdentifierNode
}

type ColumnNodeFactory = Readonly<{
  create(column: string): Readonly<ColumnNode>
}>

export const ColumnNode: ColumnNodeFactory = freeze<ColumnNodeFactory>({
  create(column) {
    return freeze({
      kind: "ColumnNode",
      column: IdentifierNode.create(column),
    })
  },
})
"#,
            ),
        ],
        "column-node.ts",
        CheckerOptions {
            module: ModuleKind::ESNext,
            strict: true,
            ..CheckerOptions::default()
        },
        &tsz_checker::test_utils::load_lib_files(&["es5.d.ts"]),
    )
    .into_iter()
    .filter(|diagnostic| diagnostic.code != 2318)
    .map(|diagnostic| (diagnostic.code, diagnostic.message_text))
    .collect::<Vec<_>>();

    assert!(
        diagnostics.is_empty(),
        "imported companion const should preserve a private Readonly factory alias annotation, got: {diagnostics:?}"
    );
}

#[test]
fn conflicted_reexport_keeps_local_namespace_surface() {
    let diagnostics = check(
        &[
            (
                "a.ts",
                r#"
function A() {}
export { A };
"#,
            ),
            (
                "b.ts",
                r#"
import { A } from "./a";
type A = 0;
export { A };
"#,
            ),
            (
                "c.ts",
                r#"
import { A } from "./b";
namespace A {
  export const displayName = "A";
}

A();
A.displayName;
"#,
            ),
        ],
        "c.ts",
    );
    let codes: Vec<u32> = diagnostics.iter().map(|(code, _)| *code).collect();

    assert!(
        codes.contains(&2440),
        "conflicted re-export should report TS2440, got: {diagnostics:?}"
    );
    assert!(
        codes.contains(&2349),
        "conflicted namespace call should report TS2349, got: {diagnostics:?}"
    );
    assert!(
        !codes.iter().any(|code| matches!(*code, 2339 | 2722)),
        "conflicted re-export should not resolve through the imported function value, got: {diagnostics:?}"
    );
}
