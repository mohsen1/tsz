use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{
    check_multi_file_with_libs, check_source_code_messages, check_source_with_libs_code_messages,
    load_compiled_lib_files,
};
use tsz_common::common::ModuleKind;

#[test]
fn primitive_type_reference_properties_keep_intrinsic_types() {
    let diagnostics = check_source_code_messages(
        r#"
interface I {
    n: number;
    s: string;
    b: boolean;
    tag: "ok";
}

const ok: I = { n: 1, s: "x", b: true, tag: "ok" };
const badNumber: I = { n: "x", s: "x", b: true, tag: "ok" };
const badTag: I = { n: 1, s: "x", b: true, tag: "no" };
"#,
    );

    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect();
    assert_eq!(
        ts2322.len(),
        2,
        "expected TS2322s for primitive and literal property mismatches, got {diagnostics:?}",
    );
    assert!(
        ts2322.iter().any(
            |(_, message)| message.contains("Type 'string' is not assignable to type 'number'")
        ),
        "expected primitive number target in TS2322, got {ts2322:?}",
    );
    assert!(
        ts2322.iter().any(
            |(_, message)| message.contains("Type '\"no\"' is not assignable to type '\"ok\"'")
        ),
        "expected string-literal target in TS2322, got {ts2322:?}",
    );
}

#[test]
fn composite_and_array_properties_keep_lowered_types() {
    let diagnostics = check_source_code_messages(
        r#"
interface I {
    choice: number | "x";
    list: number[];
    pair: [number, "ok"];
}

const ok: I = { choice: "x", list: [1], pair: [1, "ok"] };
const badChoice: I = { choice: true, list: [1], pair: [1, "ok"] };
const badList: I = { choice: 1, list: ["x"], pair: [1, "ok"] };
const badPair: I = { choice: 1, list: [1], pair: ["x", "ok"] };
"#,
    );

    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect();
    assert_eq!(
        ts2322.len(),
        3,
        "expected TS2322s for union, array, and tuple property mismatches, got {diagnostics:?}",
    );
    assert!(
        ts2322
            .iter()
            .any(|(_, message)| message.contains("Type 'true' is not assignable to type")),
        "expected union target in TS2322, got {ts2322:?}",
    );
    assert!(
        ts2322
            .iter()
            .filter(
                |(_, message)| message.contains("Type 'string' is not assignable to type 'number'")
            )
            .count()
            >= 2,
        "expected array and tuple number targets in TS2322, got {ts2322:?}",
    );
}

#[test]
fn missing_interface_lib_rows_keep_lib_shapes() {
    let lib_files = load_compiled_lib_files(&[
        "lib.es5.d.ts",
        "lib.es2015.iterable.d.ts",
        "lib.es2020.symbol.wellknown.d.ts",
        "lib.es2022.regexp.d.ts",
    ]);
    assert!(
        !lib_files.is_empty(),
        "compiled lib fixtures should be available"
    );

    let diagnostics = check_source_with_libs_code_messages(
        r#"
const descriptor: PropertyDescriptor = { configurable: "yes" };
const descriptorMap: PropertyDescriptorMap = { field: { enumerable: "yes" } };
declare const indices: RegExpIndicesArray;
const firstIndex: [number, number] = indices[0];
"#,
        "test.ts",
        CheckerOptions::default(),
        &lib_files,
    );

    assert!(
        diagnostics.iter().any(|(code, message)| *code == 2322
            && message.contains("Type 'string' is not assignable to type 'boolean'")),
        "expected PropertyDescriptor boolean property mismatch, got {diagnostics:?}",
    );
    assert!(
        diagnostics.iter().any(|(code, message)| *code == 2322
            && message.contains("Type 'unknown' is not assignable to type '[number, number]'")),
        "expected RegExpIndicesArray indexed value mismatch, got {diagnostics:?}",
    );
}

#[test]
fn missing_interface_lib_row_names_do_not_capture_imported_user_interfaces() {
    let lib_files = load_compiled_lib_files(&["lib.es5.d.ts"]);
    assert!(
        !lib_files.is_empty(),
        "compiled lib fixtures should be available"
    );

    let diagnostics = check_multi_file_with_libs(
        &[
            (
                "defs.ts",
                r#"
export interface PropertyDescriptor {
    custom: string;
}
"#,
            ),
            (
                "main.ts",
                r#"
import type { PropertyDescriptor } from "./defs";

const bad: PropertyDescriptor = {};
"#,
            ),
        ],
        "main.ts",
        CheckerOptions {
            module: ModuleKind::CommonJS,
            ..CheckerOptions::default()
        },
        &lib_files,
    );

    let messages: Vec<_> = diagnostics
        .iter()
        .map(|diag| (diag.code, diag.message_text.as_str()))
        .collect();
    assert!(
        messages
            .iter()
            .any(|(code, message)| *code == 2741 && message.contains("custom")),
        "imported user interface named PropertyDescriptor must keep its own shape, got {messages:?}",
    );
}
