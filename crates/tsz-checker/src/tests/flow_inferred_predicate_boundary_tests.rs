//! Focused coverage for inferred predicate guard narrowing through flow
//! query-boundary helpers.

use crate::context::CheckerOptions;
use crate::diagnostics::diagnostic_codes;
use crate::test_utils::{check_multi_file, check_source_strict_codes as check_strict};
use tsz_common::common::ModuleKind;

#[test]
fn inferred_predicate_narrows_both_call_branches() {
    let codes = check_strict(
        r#"
const isText = (candidate: string | number) => typeof candidate === "string";

function use(value: string | number) {
    if (isText(value)) {
        const text: string = value;
        text.toUpperCase();
    } else {
        const count: number = value;
        count.toFixed();
    }
}
"#,
    );

    assert!(
        !codes.contains(&2322) && !codes.contains(&2339),
        "expected inferred predicate to narrow both branches, got codes: {codes:?}"
    );
}

#[test]
fn explicit_boolean_annotation_does_not_infer_predicate() {
    let codes = check_strict(
        r#"
const isText = (candidate: string | number): boolean => typeof candidate === "string";

function use(value: string | number) {
    if (isText(value)) {
        const text: string = value;
    }
}
"#,
    );

    assert!(
        codes.contains(&2322),
        "expected explicit boolean annotation to keep value wide, got codes: {codes:?}"
    );
}

#[test]
fn imported_alias_inferred_predicate_narrows_true_branch() {
    let diagnostics = check_multi_file(
        &[
            (
                "./types.ts",
                r#"
export type TextOrCount = string | number;
"#,
            ),
            (
                "./main.ts",
                r#"
import type { TextOrCount } from "./types";

const isText = (candidate: TextOrCount) => typeof candidate === "string";

function use(value: TextOrCount) {
    if (isText(value)) {
        const text: string = value;
        text.toUpperCase();
    } else {
        value;
    }
}
"#,
            ),
        ],
        "./main.ts",
        CheckerOptions {
            module: ModuleKind::CommonJS,
            strict: true,
            ..CheckerOptions::default()
        },
    );
    let codes = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect::<Vec<_>>();

    assert!(
        !codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
            && !codes.contains(&diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        "expected imported alias predicate to narrow the true branch, got diagnostics: {diagnostics:?}"
    );
}
