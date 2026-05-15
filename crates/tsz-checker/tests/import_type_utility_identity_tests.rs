use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{
    check_multi_file_with_libs, diagnostic_code_messages, load_lib_files,
};

fn diagnostics_for_import_utility(entry: &str) -> Vec<(u32, String)> {
    let libs = load_lib_files(&["es5.d.ts"]);
    diagnostic_code_messages(check_multi_file_with_libs(
        &[
            (
                "module.ts",
                r#"
export function withArgs(value: string, count: number): boolean {
  return value.length === count;
}

export function renamed(value: string): { ok: true } {
  return { ok: true };
}

export type Wrap<T> = { value: T };

export class Box {
  value: number = 42;
  label: string = "box";
}
"#,
            ),
            ("consumer.ts", entry),
        ],
        "consumer.ts",
        CheckerOptions {
            strict: true,
            ..Default::default()
        },
        &libs,
    ))
}

#[test]
fn typeof_import_function_member_feeds_parameters() {
    let diagnostics = diagnostics_for_import_utility(
        r#"
const ok1: Parameters<typeof import("./module").withArgs> = ["x", 1];
type WithArgs = Parameters<typeof import("./module").withArgs>;
const ok2: WithArgs = ["y", 2];
"#,
    );

    assert!(
        diagnostics.is_empty(),
        "Expected utility types to consume `typeof import(\"./module\").fn` through stable \
         cross-file identity. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn import_type_member_feeds_conditional_type() {
    let diagnostics = diagnostics_for_import_utility(
        r#"
type ExtractWrapped<T> = T extends import("./module").Wrap<infer U> ? U : never;
const extracted: ExtractWrapped<import("./module").Wrap<string>> = "value";

type IsImportedBox<T> = T extends import("./module").Box ? true : false;
const isBox: IsImportedBox<import("./module").Box> = true;
"#,
    );

    assert!(
        diagnostics.is_empty(),
        "Expected `import(\"./module\").T` to preserve identity inside conditional type \
         checks and inferred branches. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn typeof_import_function_member_feeds_return_type() {
    let diagnostics = diagnostics_for_import_utility(
        r#"
const direct: ReturnType<typeof import("./module").withArgs> = true;
type ImportedReturn = ReturnType<typeof import("./module").renamed>;
const aliased: ImportedReturn = { ok: true };
"#,
    );

    assert!(
        diagnostics.is_empty(),
        "Expected `ReturnType<typeof import(\"./module\").fn>` to preserve imported callable \
         return identity. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn import_type_class_member_feeds_keyof() {
    let diagnostics = diagnostics_for_import_utility(
        r#"
const directValueKey: keyof import("./module").Box = "value";
const directLabelKey: keyof import("./module").Box = "label";
type BoxKeys = keyof import("./module").Box;
const aliasValueKey: BoxKeys = "value";
const aliasLabelKey: BoxKeys = "label";
"#,
    );

    assert!(
        diagnostics.is_empty(),
        "Expected `keyof import(\"./module\").Box` to include imported class instance \
         properties. Actual diagnostics: {diagnostics:#?}"
    );
}
