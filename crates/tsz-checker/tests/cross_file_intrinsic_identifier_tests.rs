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
