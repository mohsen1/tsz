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

// ── #14489: generic wrapper-alias `infer` in the conditional EXTENDS-type ──────
//
// When the conditional's extends-type is a generic wrapper alias carrying the
// `infer` (`X extends AB<infer U>` with `type AB<T> = Promise<T[]>`) and the
// check-source is the EXPANDED structural form (`Promise<number[]>`, not written
// via the alias), the pattern alias must be reduced head-only to its body
// application form (`Promise<(infer U)[]>`) so the infer binds. Previously tsz
// failed to reduce the alias pattern, collapsed the conditional to its false
// branch (`never`), and emitted a false TS2322 at the use site.

#[test]
fn wrapper_alias_infer_pattern_binds_on_expanded_source() {
    // R must resolve to `number`; assigning a number is clean.
    let source = r#"
type AB<T> = Promise<T[]>;
type Ex<X> = X extends AB<infer U> ? U : never;
type R = Ex<Promise<number[]>>;
const a: R = 7;
"#;
    assert_clean(source, "wrapper-alias infer pattern binds U=number");
}

#[test]
fn wrapper_alias_infer_pattern_payload_shape_binds() {
    let source = r#"
type W<T> = Promise<{ payload: T[] }>;
type Ex<X> = X extends W<infer U> ? U : never;
type R = Ex<Promise<{ payload: string[] }>>;
const a: R = "ok";
"#;
    assert_clean(source, "payload-shaped wrapper alias binds U=string");
}

#[test]
fn wrapper_alias_infer_pattern_set_promise_binds() {
    // The infer parameter sits one level deeper (`Set<Promise<T>>`).
    let source = r#"
type SW<T> = Set<Promise<T>>;
type Ex<X> = X extends SW<infer U> ? U : never;
type R = Ex<Set<Promise<boolean>>>;
const a: R = true;
"#;
    assert_clean(source, "Set<Promise<T>> wrapper alias binds U=boolean");
}

#[test]
fn wrapper_alias_infer_pattern_still_rejects_wrong_assignment() {
    // With U bound to `number`, assigning a string is a real TS2322 — the fix
    // must not over-accept.
    let source = r#"
type AB<T> = Promise<T[]>;
type Ex<X> = X extends AB<infer U> ? U : never;
type R = Ex<Promise<number[]>>;
const b: R = "no";
"#;
    assert_ts2322(source, "wrong assignment to inferred U=number");
}

#[test]
fn wrapper_alias_infer_pattern_false_branch_when_source_mismatches() {
    // A source that genuinely does not match the wrapper must take the false
    // branch (`never`), so assigning any value is rejected — proving the
    // reduction did not loosen the match.
    let source = r#"
type AB<T> = Promise<T[]>;
type Ex<X> = X extends AB<infer U> ? U : never;
type R = Ex<string>;
const a: R = "anything";
"#;
    assert_ts2322(source, "non-matching source stays never");
}
