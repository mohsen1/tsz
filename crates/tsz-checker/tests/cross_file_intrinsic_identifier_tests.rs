use tsz_checker::context::CheckerOptions;
use tsz_common::common::ModuleKind;

fn compile_entry_file(files: &[(&str, &str)], entry_idx: usize) -> Vec<(u32, String)> {
    let entry_file = files[entry_idx].0;
    tsz_checker::test_utils::check_multi_file(
        files,
        entry_file,
        CheckerOptions {
            module: ModuleKind::CommonJS,
            strict: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .filter(|diag| diag.code != 2318)
    .map(|diag| (diag.code, diag.message_text))
    .collect()
}

fn compile_entry_file_with_es5_lib(files: &[(&str, &str)], entry_idx: usize) -> Vec<(u32, String)> {
    let entry_file = files[entry_idx].0;
    let libs = tsz_checker::test_utils::load_lib_files(&["es5.d.ts"]);
    tsz_checker::test_utils::check_multi_file_with_libs(
        files,
        entry_file,
        CheckerOptions {
            module: ModuleKind::CommonJS,
            strict: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
        &libs,
    )
    .into_iter()
    .filter(|diag| diag.code != 2318)
    .map(|diag| (diag.code, diag.message_text))
    .collect()
}

#[test]
fn exported_undefined_alias_does_not_shadow_intrinsic_undefined_in_other_module() {
    let zod_like_exports = r#"
const undefinedType = (params?: {}) => params;
export { undefinedType as undefined };
"#;

    let zod_like_util = r#"
export function find<T>(value: T): T | undefined {
    if (false) return value;
    return undefined;
}
"#;

    let diagnostics = compile_entry_file(
        &[
            ("types.ts", zod_like_exports),
            ("helpers/util.ts", zod_like_util),
        ],
        1,
    );
    let codes: Vec<u32> = diagnostics.iter().map(|(code, _)| *code).collect();

    assert!(
        !codes.contains(&2322),
        "exported alias named undefined from another module must not shadow intrinsic undefined; got {diagnostics:#?}"
    );
}

#[test]
fn imported_numeric_boolean_alias_indexes_type_literal_maps() {
    let diagnostics = compile_entry_file(
        &[
            ("Boolean/_Internal.ts", "export type Boolean = 0 | 1;\n"),
            (
                "Boolean/And.ts",
                r#"
import {Boolean} from './_Internal';

export type And<B1 extends Boolean, B2 extends Boolean> = {
    0: {
      0: 0
      1: 0
    }
    1: {
      0: 0
      1: 1
    }
}[B1][B2];
"#,
            ),
        ],
        1,
    );

    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2536),
        "imported numeric Boolean alias should index Boolean maps without TS2536: {diagnostics:#?}"
    );
}

#[test]
fn imported_numeric_boolean_alias_indexes_type_literal_maps_with_libs() {
    let diagnostics = compile_entry_file_with_es5_lib(
        &[
            ("Boolean/_Internal.ts", "export type Boolean = 0 | 1;\n"),
            (
                "Boolean/And.ts",
                r#"
import {Boolean} from './_Internal';

export type And<B1 extends Boolean, B2 extends Boolean> = {
    0: {
      0: 0
      1: 0
    }
    1: {
      0: 0
      1: 1
    }
}[B1][B2];
"#,
            ),
        ],
        1,
    );

    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2536),
        "imported numeric Boolean alias should shadow global Boolean when libs are loaded: {diagnostics:#?}"
    );
}

#[test]
fn imported_string_aliases_index_nested_type_literal_maps() {
    let diagnostics = compile_entry_file(
        &[
            (
                "Function/_Internal.ts",
                "export type Mode = 'sync' | 'async';\nexport type Input = 'multi' | 'list';\n",
            ),
            (
                "Function/Compose.ts",
                r#"
import {Input, Mode} from './_Internal';

type ComposeMultiSync = { syncMulti: true };
type ComposeListSync = { syncList: true };
type ComposeMultiAsync = { asyncMulti: true };
type ComposeListAsync = { asyncList: true };

export type Compose<mode extends Mode = 'sync', input extends Input = 'multi'> = {
    'sync' : {
        'multi': ComposeMultiSync
        'list' : ComposeListSync
    }
    'async': {
        'multi': ComposeMultiAsync
        'list' : ComposeListAsync
    }
}[mode][input];
"#,
            ),
        ],
        1,
    );

    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2536),
        "imported string aliases should index nested maps without TS2536: {diagnostics:#?}"
    );
}
