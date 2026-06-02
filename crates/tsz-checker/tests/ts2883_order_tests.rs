//! TS2883 cited-name selection must be structural and deterministic.
//!
//! The diagnostic text names the first non-portable type reference reachable
//! from an exported inferred type. That "first" must follow the solver
//! traversal/source order, not hash-set iteration or `TypeId` allocation order.

use tsz_checker::context::CheckerOptions;
use tsz_common::common::{ModuleKind, ScriptTarget};

fn declaration_diagnostic_messages(files: &[(&str, &str)], entry_file: &str) -> Vec<String> {
    tsz_checker::test_utils::check_multi_file(
        files,
        entry_file,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            module: ModuleKind::NodeNext,
            strict: true,
            emit_declarations: true,
            no_lib: true,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .filter(|diagnostic| diagnostic.code == 2883)
    .map(|diagnostic| diagnostic.message_text)
    .collect()
}

#[test]
fn ts2883_names_first_nonportable_object_property_in_source_order() {
    let messages = declaration_diagnostic_messages(
        &[
            (
                "/node_modules/pkg/node_modules/inner/index.d.ts",
                "export interface FirstHidden { first: string; }\n\
                 export interface SecondHidden { second: string; }\n",
            ),
            (
                "/src/index.ts",
                r#"
import type { FirstHidden, SecondHidden } from "../node_modules/pkg/node_modules/inner";

declare const first: FirstHidden;
declare const second: SecondHidden;

export const value = { second, first };
"#,
            ),
        ],
        "/src/index.ts",
    );

    assert_one_ts2883(&messages);
    assert!(
        messages[0].contains("'SecondHidden'"),
        "TS2883 should name the first non-portable property by source order: {messages:#?}"
    );
    assert!(
        !messages[0].contains("'FirstHidden'"),
        "TS2883 should not drift to the later property: {messages:#?}"
    );
}

#[test]
fn ts2883_names_outer_nonportable_application_before_inner_argument() {
    let messages = declaration_diagnostic_messages(
        &[
            (
                "/node_modules/pkg/node_modules/inner/index.d.ts",
                "export interface InnerHidden { inner: string; }\n\
                 export interface WrapperHidden<T> { value: T; }\n",
            ),
            (
                "/src/index.ts",
                r#"
import type { InnerHidden, WrapperHidden } from "../node_modules/pkg/node_modules/inner";

declare const wrapped: WrapperHidden<InnerHidden>;

export const value = wrapped;
"#,
            ),
        ],
        "/src/index.ts",
    );

    assert_one_ts2883(&messages);
    assert!(
        messages[0].contains("'WrapperHidden'"),
        "TS2883 should name the outer non-portable application first: {messages:#?}"
    );
    assert!(
        !messages[0].contains("'InnerHidden'"),
        "TS2883 should not drift to the nested argument before the wrapper: {messages:#?}"
    );
}

#[test]
fn ts2883_names_function_parameter_before_return_type() {
    let messages = declaration_diagnostic_messages(
        &[
            (
                "/node_modules/pkg/node_modules/inner/index.d.ts",
                "export interface ParameterHidden { input: string; }\n\
                 export interface ReturnHidden { output: string; }\n",
            ),
            (
                "/src/index.ts",
                r#"
import type { ParameterHidden, ReturnHidden } from "../node_modules/pkg/node_modules/inner";

declare const fn: (input: ParameterHidden) => ReturnHidden;

export const value = fn;
"#,
            ),
        ],
        "/src/index.ts",
    );

    assert_one_ts2883(&messages);
    assert!(
        messages[0].contains("'ParameterHidden'"),
        "TS2883 should name the first function parameter before the return type: {messages:#?}"
    );
    assert!(
        !messages[0].contains("'ReturnHidden'"),
        "TS2883 should not drift to the return type before parameters: {messages:#?}"
    );
}

fn assert_one_ts2883(messages: &[String]) {
    assert_eq!(
        messages.len(),
        1,
        "expected one TS2883 diagnostic, got {messages:#?}"
    );
}
