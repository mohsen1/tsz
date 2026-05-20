use tsz_checker::context::{CheckerOptions, ScriptTarget};
use tsz_checker::test_utils::check_source;

fn check_with_no_unused_locals(source: &str) -> Vec<u32> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            no_unused_locals: true,
            use_unknown_in_catch_variables: false,
            target: ScriptTarget::ES2022,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|diagnostic| diagnostic.code)
    .collect()
}

#[test]
fn destructured_catch_bindings_are_exempt_from_no_unused_locals() {
    let codes = check_with_no_unused_locals(
        r#"
export function f() {
  try {
    throw { message: "boom" };
  } catch ({ message }) {
  }
}
"#,
    );

    assert!(
        !codes.contains(&6133),
        "Expected no TS6133 for destructured catch binding. Got: {codes:?}"
    );
}
