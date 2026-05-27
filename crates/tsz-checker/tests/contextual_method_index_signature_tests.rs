use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::{check_source, diagnostic_count};

fn check_default(source: &str) -> Vec<Diagnostic> {
    check_source(source, "test.ts", CheckerOptions::default())
}

#[test]
fn index_signature_mapped_intersection_contextualizes_method_parameters() {
    for source in [
        r#"
declare function createHandlers(
  handlers: { [key: string]: (state: string) => void } & {
    run: object;
  }
): void;

createHandlers({
  run(value) { value.toUpperCase(); },
});
"#,
        r#"
declare function createHandlers<T>(
  handlers: { [key: string]: (state: string) => void } & {
    [P in keyof T]: object;
  }
): void;

createHandlers({
  run(value) { value.toUpperCase(); },
});
"#,
        // Renamed type-param (Source) and mapped variable (Key) — proves the
        // rule is not tied to the `T`/`P` spellings.
        r#"
declare function createHandlers<Source>(
  handlers: { [key: string]: (state: string) => void } & {
    [Key in keyof Source]: object;
  }
): void;

createHandlers({
  run(value) { value.toUpperCase(); },
});
"#,
    ] {
        let diagnostics = check_default(source);
        assert_eq!(
            diagnostic_count(&diagnostics, 7006),
            0,
            "Index-signature intersection contexts should type method parameters, got diagnostics={diagnostics:?}"
        );
    }
}

#[test]
fn unknown_index_signature_context_keeps_method_parameter_implicit_any() {
    let source = r#"
declare function acceptUnknownTable(table: { [key: string]: unknown }): void;

acceptUnknownTable({
  run(value) { value; },
});
"#;

    let diagnostics = check_default(source);
    assert_eq!(
        diagnostic_count(&diagnostics, 7006),
        1,
        "Unknown index signatures must not suppress method parameter implicit-any diagnostics"
    );
}
