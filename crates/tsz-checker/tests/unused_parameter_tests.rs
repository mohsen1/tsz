use tsz_checker::context::{CheckerOptions, ScriptTarget};
use tsz_checker::test_utils::check_source;

fn check_with_no_unused_parameters(source: &str) -> Vec<u32> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            no_unused_parameters: true,
            target: ScriptTarget::ES2022,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|diagnostic| diagnostic.code)
    .collect()
}

#[test]
fn nested_destructured_overload_parameters_are_exempt() {
    let codes = check_with_no_unused_parameters(
        r#"
export function f({
  a: {
    b: {
      c
    }
  }
}: {
  a: {
    b: {
      c: string
    }
  }
}): void;
export function f(arg?: unknown) {
  void arg;
}
"#,
    );

    assert!(
        !codes.contains(&6133),
        "Expected no TS6133 for nested destructured overload parameter. Got: {codes:?}"
    );
}
