use tsz_checker::context::CheckerOptions;
use tsz_common::common::{ModuleKind, ScriptTarget};

fn declaration_diagnostic_codes(files: &[(&str, &str)], entry_file: &str) -> Vec<u32> {
    let libs = tsz_checker::test_utils::load_lib_files(&["es5.d.ts"]);
    tsz_checker::test_utils::check_multi_file_with_libs(
        files,
        entry_file,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            module: ModuleKind::NodeNext,
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
