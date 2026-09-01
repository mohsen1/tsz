use crate::test_utils::check_source_diagnostics;

fn diagnostic_codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

#[test]
fn return_context_does_not_treat_user_promise_alias_as_lib_promise() {
    let source = r#"
type Promise<T> = { value: T };
declare function make<T>(callback: () => Promise<T>): T;

const n: number = make(() => ({ value: "text" }));
"#;

    let codes = diagnostic_codes(source);
    assert!(
        codes.contains(&2322),
        "user-defined Promise<T> should not be unwrapped as the lib Promise<T>; got {codes:?}"
    );
}

#[test]
fn return_context_promise_wrapper_detection_uses_lib_identity_helper() {
    let sources = [
        include_str!("../checkers/call_context.rs"),
        include_str!("../checkers/call_checker/overload_resolution/return_context.rs"),
        include_str!("../types/utilities/return_type.rs"),
    ];
    assert!(
        sources.iter().all(|source| !source.contains(
            "return_context_application_base_has_name(base, &[\"Promise\", \"PromiseLike\"])"
        )),
        "return-context Promise wrapper detection must use lib/global identity, not a name allowlist"
    );
    assert!(
        sources
            .iter()
            .any(|source| source.contains("return_context_application_base_is_lib_promise_like")),
        "expected the call-context path to route Promise wrapper checks through the identity helper"
    );
}
