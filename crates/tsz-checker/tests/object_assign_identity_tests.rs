use tsz_checker::context::CheckerOptions;
use tsz_common::common::{ModuleKind, ScriptTarget};

fn declaration_diagnostic_codes(files: &[(&str, &str)], entry_file: &str) -> Vec<u32> {
    declaration_diagnostic_codes_with_module(files, entry_file, ModuleKind::NodeNext)
}

fn declaration_diagnostic_codes_with_module(
    files: &[(&str, &str)],
    entry_file: &str,
    module: ModuleKind,
) -> Vec<u32> {
    let libs = tsz_checker::test_utils::load_lib_files(&["es5.d.ts"]);
    tsz_checker::test_utils::check_multi_file_with_libs(
        files,
        entry_file,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            module,
            strict: true,
            emit_declarations: true,
            ..CheckerOptions::default()
        },
        &libs,
    )
    .into_iter()
    .map(|diagnostic| diagnostic.code)
    .collect()
}

#[test]
fn local_object_assign_does_not_trigger_builtin_object_assign_portability_check() {
    let codes = declaration_diagnostic_codes(
        &[
            (
                "/node_modules/pkg/node_modules/inner/index.d.ts",
                r#"
export interface Hidden {
  value: string;
}
"#,
            ),
            (
                "/src/index.ts",
                r#"
import type { Hidden } from "../node_modules/pkg/node_modules/inner";
declare const hiddenValue: Hidden;
const Object = {
  assign<T, U>(target: T, source: U): T & U {
    return target as T & U;
  }
};

export default Object.assign({}, { hiddenValue });
"#,
            ),
        ],
        "/src/index.ts",
    );

    assert!(
        !codes.contains(&2883),
        "local Object.assign must not be treated as the built-in Object.assign declaration-emit portability path: {codes:?}"
    );
}

#[test]
fn builtin_object_assign_still_reports_nonportable_default_export() {
    let codes = declaration_diagnostic_codes(
        &[
            (
                "/node_modules/pkg/node_modules/inner/index.d.ts",
                r#"
export interface Hidden {
  value: string;
}
"#,
            ),
            (
                "/src/index.ts",
                r#"
import type { Hidden } from "../node_modules/pkg/node_modules/inner";
declare const hiddenValue: Hidden;

export default Object.assign({}, { hiddenValue });
"#,
            ),
        ],
        "/src/index.ts",
    );

    assert!(
        codes.contains(&2883),
        "built-in Object.assign should still use the declaration-emit portability path: {codes:?}"
    );
}

#[test]
fn builtin_object_assign_reports_nonportable_tagged_template_mapped_alias_default_export() {
    let codes = declaration_diagnostic_codes_with_module(
        &[
            (
                "/node_modules/outer/node_modules/inner/index.d.ts",
                r#"
interface LocalStatics {
  hidden: string;
}
declare namespace innerStatics {
  type PickPublic<T> = { [K in Exclude<keyof T, keyof LocalStatics>]: T[K] };
}
export = innerStatics;
"#,
            ),
            (
                "/node_modules/outer/index.d.ts",
                r#"
import * as innerStatics from "inner";
export interface BaseShape {
  tag: string;
}
export type PublicShape<T extends string> =
  string
  & BaseShape
  & innerStatics.PickPublic<T>;
export interface Factory {
  div(a: TemplateStringsArray): PublicShape<"renamed">;
}
declare const factory: Factory;
export default factory;
"#,
            ),
            (
                "/src/index.ts",
                r#"
import factory from "outer";

const first = factory.div``;
const second = factory.div``;
export const third = factory.div``;

export default Object.assign(first, {
  second,
  third,
});
"#,
            ),
        ],
        "/src/index.ts",
        ModuleKind::CommonJS,
    );

    assert!(
        codes.contains(&2883),
        "built-in Object.assign should report TS2883 for tagged-template nested mapped aliases from transitive dependencies: {codes:?}"
    );
}
