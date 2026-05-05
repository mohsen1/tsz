use tsz_checker::context::{CheckerOptions, ScriptTarget};
use tsz_checker::test_utils::check_source;

fn check_js(source: &str) -> Vec<u32> {
    check_source(
        source,
        "test.js",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            strict: true,
            target: ScriptTarget::ES2022,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|diagnostic| diagnostic.code)
    .collect()
}

#[test]
fn arguments_property_name_does_not_create_implicit_rest_parameter() {
    let codes = check_js(
        r#"
// @ts-check

/** @type {{ arguments: string }} */
const holder = { arguments: "not the function arguments object" };

/**
 * @param {number} value
 */
function f(value) {
  holder.arguments;
}

f(1, 2);
"#,
    );

    assert!(
        codes.contains(&2554),
        "Expected TS2554 for extra argument when only a property is named `arguments`. Got: {codes:?}"
    );
}

#[test]
fn real_arguments_reference_still_creates_implicit_rest_parameter() {
    let codes = check_js(
        r#"
// @ts-check

/**
 * @param {number} value
 */
function f(value) {
  arguments[0];
}

f(1, 2);
"#,
    );

    assert!(
        !codes.contains(&2554),
        "Expected no TS2554 when function body references real `arguments`. Got: {codes:?}"
    );
}
