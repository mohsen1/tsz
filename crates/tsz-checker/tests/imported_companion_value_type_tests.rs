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
