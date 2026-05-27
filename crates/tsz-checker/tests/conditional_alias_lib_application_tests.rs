//! Regression tests for #10446.
//!
//! Structural rule: when a user conditional alias is applied to a concrete
//! library utility application, the inner application must reduce before the
//! outer conditional is used as an assignment target.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs, load_default_lib_files};

fn diagnostics(source: &str) -> Vec<(u32, String)> {
    let libs = load_default_lib_files();
    check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..Default::default()
        },
        &libs,
    )
    .iter()
    .map(|diagnostic| (diagnostic.code, diagnostic.message_text.clone()))
    .collect()
}

fn assert_clean(source: &str, label: &str) {
    let diagnostics = diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "[{label}] expected clean, got {diagnostics:?}"
    );
}

fn assert_ts2322(source: &str, label: &str) {
    let diagnostics = diagnostics(source);
    assert!(
        diagnostics.iter().any(|(code, _)| *code == 2322),
        "[{label}] expected TS2322, got {diagnostics:?}"
    );
}

#[test]
fn direct_conditional_alias_application_remains_clean() {
    let source = r#"
type Unbox<Input> = Input extends Promise<infer Value> ? Value : Input;
type Result = Unbox<Promise<number>>;
const value: Result = 1;
"#;

    assert_clean(source, "direct Promise<number> application");
}

#[test]
fn conditional_alias_reduces_nonnullable_inner_application() {
    let source = r#"
type Unbox<Input> = Input extends Promise<infer Value> ? Value : Input;
type Result = Unbox<NonNullable<Promise<number>>>;
const value: Result = 1;
"#;

    assert_clean(source, "NonNullable<Promise<number>> inner application");
}

#[test]
fn conditional_alias_reduces_return_type_inner_application() {
    let source = r#"
type Unbox<Input> = Input extends Promise<infer Value> ? Value : Input;
type Result = Unbox<ReturnType<() => Promise<number>>>;
const value: Result = 1;
"#;

    assert_clean(
        source,
        "ReturnType<() => Promise<number>> inner application",
    );
}

#[test]
fn conditional_alias_reduces_typeof_return_type_inner_application() {
    let source = r#"
type Unbox<Subject> = Subject extends Promise<infer Output> ? Output : Subject;
declare function fetchValue(): Promise<number>;
type Result = Unbox<ReturnType<typeof fetchValue>>;
const value: Result = 1;
"#;

    assert_clean(source, "ReturnType<typeof fetchValue> inner application");
}

#[test]
fn conditional_alias_still_rejects_wrong_assignment() {
    let source = r#"
type Unbox<Input> = Input extends Promise<infer Value> ? Value : Input;
type Result = Unbox<ReturnType<() => Promise<number>>>;
const value: Result = "wrong";
"#;

    assert_ts2322(source, "wrong assignment after inner application reduction");
}

#[test]
fn user_alias_inner_application_remains_clean() {
    let source = r#"
type Unbox<Input> = Input extends Promise<infer Value> ? Value : Input;
type Id<Wrapped> = Wrapped;
type Result = Unbox<Id<Promise<number>>>;
const value: Result = 1;
"#;

    assert_clean(source, "user alias inner application");
}
