//! Imported predicate false-branch narrowing coverage.

use crate::context::CheckerOptions;
use crate::diagnostics::{Diagnostic, diagnostic_codes};
use crate::test_utils::check_multi_file;
use tsz_common::common::ModuleKind;

fn check_imported_predicate(source: &str) -> Vec<Diagnostic> {
    check_multi_file(
        &[
            (
                "./types.ts",
                r#"
export type TextOrCount = string | number;
export type LabelOrFlag = "label" | false;
"#,
            ),
            ("./main.ts", source),
        ],
        "./main.ts",
        CheckerOptions {
            module: ModuleKind::CommonJS,
            strict: true,
            ..CheckerOptions::default()
        },
    )
}

fn assert_no_assignability_or_property_errors(diagnostics: &[Diagnostic]) {
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                || diagnostic.code == diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE
        })
        .map(|diagnostic| {
            (
                diagnostic.code,
                diagnostic.file.as_str(),
                diagnostic.start,
                diagnostic.message_text.as_str(),
            )
        })
        .collect();

    assert!(
        relevant.is_empty(),
        "expected imported predicate to narrow the false branch, got diagnostics: {relevant:?}",
    );
}

#[test]
fn imported_alias_inferred_predicate_narrows_false_branch() {
    let codes = check_imported_predicate(
        r#"
import type { TextOrCount } from "./types";

const isText = (candidate: TextOrCount) => typeof candidate === "string";

function use(value: TextOrCount) {
    if (isText(value)) {
        const text: string = value;
    } else {
        const count: number = value;
        count.toFixed();
    }
}
"#,
    );

    assert_no_assignability_or_property_errors(&codes);
}

#[test]
fn imported_alias_explicit_predicate_narrows_false_branch() {
    let codes = check_imported_predicate(
        r#"
import type { LabelOrFlag } from "./types";

function isLabel(input: LabelOrFlag): input is "label" {
    return input === "label";
}

function use(value: LabelOrFlag) {
    if (isLabel(value)) {
        const label: "label" = value;
    } else {
        const flag: false = value;
    }
}
"#,
    );

    assert_no_assignability_or_property_errors(&codes);
}
