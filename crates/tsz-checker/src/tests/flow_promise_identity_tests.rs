use crate::test_utils::check_source_codes;
use std::fs;

#[test]
fn await_assignment_flow_does_not_unwrap_local_promise_alias_by_name() {
    let source = r#"
type Promise<T> = { value: T };
declare function getBox(): Promise<string>;

async function f() {
    let value: Promise<string> | number = 0;
    value = await getBox();
    const text: string = value;
}
"#;

    let codes = check_source_codes(source);
    assert!(
        codes.contains(&2322),
        "local Promise<T> alias must not be treated as lib Promise<T> during await assignment flow; got {codes:?}"
    );
}

#[test]
fn await_assignment_flow_still_unwraps_lib_promise_return() {
    let source = r#"
async function getText(): Promise<string> {
    return "ok";
}

async function f() {
    let value: string | number = 0;
    value = await getText();
    const text: string = value;
}
"#;

    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2322),
        "lib Promise<T> returns should still unwrap during await assignment flow; got {codes:?}"
    );
}

#[test]
fn flow_await_fallback_promise_detection_uses_identity_not_name_allowlist() {
    let source = fs::read_to_string("src/flow/control_flow/assignment_fallback.rs")
        .expect("failed to read flow assignment fallback source");

    assert!(
        !source.contains("is_builtin_promise_like_name"),
        "flow await fallback must use binder/lib Promise identity, not a raw Promise/PromiseLike name allowlist"
    );
    assert!(
        !source.contains("name == \"Promise\" || name == \"PromiseLike\""),
        "flow await fallback must not hardcode Promise/PromiseLike spellings for semantic decisions"
    );
}
