use tsz_checker::context::CheckerOptions;
use tsz_common::common::ModuleKind;

fn check(files: &[(&str, &str)], entry_file: &str) -> Vec<(u32, String)> {
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
fn local_value_shadowing_type_only_import_keeps_literal_members() {
    let diagnostics = check(
        &[
            ("a.ts", r#"export type A = "a";"#),
            (
                "b.ts",
                r#"
import type { A } from "./a";
const A: A = "a";
A.toUpperCase();
"#,
            ),
        ],
        "b.ts",
    );

    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2339),
        "local value shadowing import type should not lose string-literal members, got: {diagnostics:?}"
    );
}

#[test]
fn type_only_reexport_import_can_merge_with_local_namespace_value() {
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
        "import/type conflict should still report TS2440, got: {diagnostics:?}"
    );
    assert!(
        codes.contains(&2349),
        "calling the namespace-merged type-only import should report TS2349, got: {diagnostics:?}"
    );
    assert!(
        !codes.contains(&2339) && !codes.contains(&2722),
        "namespace property access should not cascade TS2339/TS2722, got: {diagnostics:?}"
    );
}
