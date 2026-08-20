use crate::context::CheckerOptions;
use crate::test_utils::{
    check_multi_file_with_libs_stamped, check_source_with_libs_code_messages, load_lib_files,
};

fn es5_libs() -> Vec<std::sync::Arc<tsz_binder::lib_loader::LibFile>> {
    load_lib_files(&["es5.d.ts"])
}

#[test]
fn imported_declaration_path_alias_accepts_string_literal_keys() {
    let libs = es5_libs();
    let diagnostics = check_multi_file_with_libs_stamped(
        &[
            (
                "forms.d.ts",
                r#"
export type Path<T> = Extract<keyof T, string>;
export function useForm<T>(options: { defaultValues: T }): {
  register(field: Path<T>): Record<string, unknown>;
};
"#,
            ),
            (
                "domain.ts",
                r#"
export type IssueInput = {
  title: string;
  priority: "low" | "medium" | "high";
  area: "parser" | "checker";
  estimate: number;
};
export const defaults: IssueInput = {
  title: "Fix",
  priority: "medium",
  area: "checker",
  estimate: 1,
};
"#,
            ),
            (
                "main.ts",
                r#"
import { useForm, type Path } from "./forms";
import { defaults, type IssueInput } from "./domain";

const fields: Path<IssueInput>[] = ["title", "priority", "area", "estimate"];
const { register } = useForm<IssueInput>({ defaultValues: defaults });
fields.map((field) => register(field));
"#,
            ),
        ],
        "main.ts",
        CheckerOptions::default(),
        &libs,
    );
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == 2322 || diagnostic.code == 2345)
        .collect();
    assert!(
        relevant.is_empty(),
        "declaration-file Path<T> aliases must reduce Extract<keyof T, string> for string literal keys, got {relevant:#?}"
    );
}

#[test]
fn type_argument_constraint_displays_primitive_key_union_not_property_key_alias() {
    let libs = es5_libs();
    let diagnostics = check_source_with_libs_code_messages(
        r#"
type Bad = Record<object, object>;
"#,
        "test.ts",
        CheckerOptions::default(),
        &libs,
    );
    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2344 && message.contains("constraint 'string | number | symbol'")
        }),
        "TS2344 should display the primitive key union constraint, got {diagnostics:#?}"
    );
    assert!(
        diagnostics
            .iter()
            .all(|(_, message)| !message.contains("constraint 'PropertyKey'")),
        "TS2344 should not repaint this constraint as PropertyKey, got {diagnostics:#?}"
    );
}
