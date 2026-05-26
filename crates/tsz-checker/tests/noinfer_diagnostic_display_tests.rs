use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::diagnostic_codes;

fn strict_messages(source: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_source(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|diagnostic| (diagnostic.code, diagnostic.message_text))
    .collect()
}

#[test]
fn ts2345_union_noinfer_parameter_displays_failed_inner_target() {
    let diagnostics = strict_messages(
        r#"
declare function f<T>(a: T, b: NoInfer<T> | null): T;
f("a", "b");
"#,
    );
    let ts2345: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| {
            *code == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
        })
        .collect();

    assert_eq!(ts2345.len(), 1, "expected one TS2345, got {diagnostics:?}");
    let message = &ts2345[0].1;
    assert!(
        message.contains("parameter of type '\"a\"'"),
        "expected failed inner target display, got: {message}"
    );
    assert!(
        !message.contains("NoInfer") && !message.contains("null"),
        "expected NoInfer/null elided from TS2345 target, got: {message}"
    );
}

#[test]
fn ts2345_intersection_noinfer_parameter_uses_renamed_inner_target() {
    let diagnostics = strict_messages(
        r#"
declare function f<U>(a: U, b: NoInfer<U> & {}): U;
f("a", "b");
"#,
    );
    let ts2345: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| {
            *code == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
        })
        .collect();

    assert_eq!(ts2345.len(), 1, "expected one TS2345, got {diagnostics:?}");
    let message = &ts2345[0].1;
    assert!(
        message.contains("parameter of type '\"a\"'"),
        "expected renamed inner target display, got: {message}"
    );
    assert!(
        !message.contains("NoInfer"),
        "expected NoInfer elided from TS2345 target, got: {message}"
    );
}

#[test]
fn ts2353_noinfer_union_excess_property_preserves_union_surface() {
    let diagnostics = strict_messages(
        r#"
declare function f<T extends { x: string }>(
  a: T,
  b: NoInfer<T> | (() => NoInfer<T>),
): void;
f({ x: "foo" }, { x: "bar", y: 42 });
"#,
    );
    let ts2353: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| {
            *code
                == diagnostic_codes::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_AND_DOES_NOT_EXIST_IN_TYPE
        })
        .collect();

    assert_eq!(ts2353.len(), 1, "expected one TS2353, got {diagnostics:?}");
    let message = &ts2353[0].1;
    assert!(
        message.contains("NoInfer<{ x: string; }> | (() => NoInfer<{ x: string; }>)"),
        "expected TS2353 to preserve NoInfer union surface, got: {message}"
    );
}

#[test]
fn ts2559_noinfer_weak_type_parameter_preserves_wrapper_surface() {
    let diagnostics = strict_messages(
        r#"
type Partial<T> = { [P in keyof T]?: T[P] };
declare const partialObj1: Partial<{ a: unknown; b: unknown }>;
declare const partialObj2: Partial<{ c: unknown; d: unknown }>;
declare const someObj1: { x: string };

declare function test1<T>(a: T, b: NoInfer<T> & { prop?: unknown }): void;
test1(partialObj1, someObj1);

declare function test2<T1, T2>(
  a: T1,
  b: T2,
  c: NoInfer<T1> & NoInfer<T2>,
): void;
test2(partialObj1, partialObj2, someObj1);
"#,
    );
    let ts2559: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_HAS_NO_PROPERTIES_IN_COMMON_WITH_TYPE)
        .collect();

    assert_eq!(ts2559.len(), 2, "expected two TS2559, got {diagnostics:?}");
    assert!(
        ts2559.iter().any(|(_, message)| message
            .contains("NoInfer<Partial<{ a: unknown; b: unknown; }>> & { prop?: unknown; }")),
        "expected weak-type display to preserve NoInfer intersection wrapper, got: {ts2559:?}"
    );
    assert!(
        ts2559.iter().any(|(_, message)| message.contains(
            "NoInfer<Partial<{ a: unknown; b: unknown; }>> & NoInfer<Partial<{ c: unknown; d: unknown; }>>"
        )),
        "expected weak-type display to preserve both NoInfer wrappers, got: {ts2559:?}"
    );
}
