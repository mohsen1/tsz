use tsz_checker::test_utils::{check_source_diagnostics, diagnostic_code_message_refs};

fn codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

fn assert_no_ts2322(source: &str, context: &str) {
    let diagnostics = check_source_diagnostics(source);
    assert!(
        !diagnostics.iter().any(|diagnostic| diagnostic.code == 2322),
        "{context}: expected no TS2322, got {:#?}",
        diagnostic_code_message_refs(&diagnostics),
    );
}

fn assert_no_ts7006(source: &str, context: &str) {
    let diagnostics = check_source_diagnostics(source);
    assert!(
        !diagnostics.iter().any(|diagnostic| diagnostic.code == 7006),
        "{context}: expected no TS7006, got {:#?}",
        diagnostic_code_message_refs(&diagnostics),
    );
}

#[test]
fn generic_object_callback_return_arrow_no_implicit_any() {
    assert_no_ts7006(
        r#"
declare function repro<T>(config: {
    params: T;
    callback: () => (params: T) => number;
}): void;

repro({
    params: 1,
    callback: () => { return value => value + 1 },
});
"#,
        "returned arrow parameter should inherit resolved generic context",
    );
}

#[test]
fn generic_object_method_return_arrow_no_implicit_any() {
    assert_no_ts7006(
        r#"
declare function repro<T>(config: {
    params: T;
    callback: () => (params: T) => number;
}): void;

repro({
    params: 1,
    callback() { return item => item + 1 },
});
"#,
        "returned method arrow parameter should inherit resolved generic context",
    );
}

#[test]
fn nullish_callback_return_does_not_self_infer_wrapped_result() {
    assert_no_ts2322(
        r#"
interface PLike<T> {
    then<R1 = T>(onfulfilled: (value: T) => R1 | PLike<R1>): PLike<R1>;
}

function fwd<A, B = A>(
    p: PLike<A>,
    onfulfilled: (value: A) => B | PLike<B>,
): PLike<B> {
    return p.then(either => onfulfilled(either) ?? (either as unknown as B));
}
"#,
        "context-sensitive nullish callback should not infer R1 := PLike<R1>",
    );
}

#[test]
fn nullish_callback_return_occurs_check_is_name_independent() {
    assert_no_ts2322(
        r#"
interface Task<Value> {
    then<Out = Value>(callback: (value: Value) => Out | Task<Out>): Task<Out>;
}

function forward<Input, Output = Input>(
    task: Task<Input>,
    callback: (value: Input) => Output | Task<Output>,
): Task<Output> {
    return task.then(item => callback(item) ?? (item as unknown as Output));
}
"#,
        "renamed generic callback should not infer Out := Task<Out>",
    );
}

#[test]
fn concrete_bad_callback_return_still_reports_assignment_error() {
    let source = r#"
interface PLike<T> {
    then<R1 = T>(onfulfilled: (value: T) => R1 | PLike<R1>): PLike<R1>;
}

function bad(p: PLike<number>): PLike<number> {
    return p.then(() => "bad");
}
"#;
    let actual = codes(source);
    assert!(
        actual.contains(&2322),
        "concrete incompatible callback return must still report TS2322, got {actual:?}",
    );
}
