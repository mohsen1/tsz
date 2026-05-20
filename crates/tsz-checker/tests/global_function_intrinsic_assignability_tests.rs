use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs_code_messages, load_lib_files};

const TS2322: u32 = 2322;

fn diagnostics_with_es5(source: &str) -> Vec<(u32, String)> {
    let libs = load_lib_files(&["es5.d.ts"]);
    assert!(!libs.is_empty(), "es5 lib fixture should be available");
    check_source_with_libs_code_messages(source, "test.ts", CheckerOptions::default(), &libs)
}

fn assert_no_ts2322(source: &str) {
    let diagnostics = diagnostics_with_es5(source);
    assert!(
        diagnostics.iter().all(|(code, _)| *code != TS2322),
        "expected no TS2322, got {diagnostics:?}",
    );
}

fn assert_ts2322(source: &str) {
    let diagnostics = diagnostics_with_es5(source);
    assert!(
        diagnostics.iter().any(|(code, _)| *code == TS2322),
        "expected TS2322, got {diagnostics:?}",
    );
}

#[test]
fn callable_value_assigns_to_global_function() {
    assert_no_ts2322(
        r#"
declare const f: (value: number) => string;
const g: Function = f;
"#,
    );
}

#[test]
fn callable_object_assigns_to_global_function() {
    assert_no_ts2322(
        r#"
type CallableObject = {
    (value: number): string;
    tag: string;
};
declare const f: CallableObject;
const g: Function = f;
"#,
    );
}

#[test]
fn global_function_does_not_assign_to_specific_signature() {
    assert_ts2322(
        r#"
declare const g: Function;
const h: (value: number) => string = g;
"#,
    );
}

#[test]
fn aliased_signature_still_rejects_global_function_source() {
    assert_ts2322(
        r#"
type Handler = (value: number) => string;
declare const g: Function;
const h: Handler = g;
"#,
    );
}

#[test]
fn any_source_still_assigns_to_specific_signature() {
    assert_no_ts2322(
        r#"
declare const g: any;
const h: (value: number) => string = g;
"#,
    );
}
