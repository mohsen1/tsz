use tsz_common::common::ScriptTarget;
use tsz_common::options::checker::CheckerOptions;

fn diags_with_target(source: &str, target: ScriptTarget) -> Vec<crate::diagnostics::Diagnostic> {
    crate::test_utils::check_source(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            target,
            ..CheckerOptions::default()
        },
    )
}

#[test]
fn private_identifier_property_requires_es2015_target() {
    let diags = diags_with_target(
        r#"
class Box {
    #value = 1;
}
"#,
        ScriptTarget::ES5,
    );

    assert!(
        diags.iter().any(|d| d.code == 18028),
        "Expected TS18028 for private field under ES5 target, got: {diags:?}"
    );
}

#[test]
fn private_identifier_method_requires_es2015_target() {
    let diags = diags_with_target(
        r#"
class Box {
    #read() { return 1; }
}
"#,
        ScriptTarget::ES5,
    );

    assert!(
        diags.iter().any(|d| d.code == 18028),
        "Expected TS18028 for private method under ES5 target, got: {diags:?}"
    );
}

#[test]
fn auto_accessor_requires_es2015_target() {
    let diags = diags_with_target(
        r#"
class Box {
    accessor value: number = 0;
}
"#,
        ScriptTarget::ES5,
    );

    assert!(
        diags.iter().any(|d| d.code == 18045),
        "Expected TS18045 for auto-accessor under ES5 target, got: {diags:?}"
    );
}

#[test]
fn class_feature_gates_allow_es2015_target() {
    let diags = diags_with_target(
        r#"
class Box {
    #value = 1;
    accessor current: number = 0;
    #read() { return this.#value; }
}
"#,
        ScriptTarget::ES2015,
    );

    let gated: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 18028 || d.code == 18045)
        .collect();
    assert!(
        gated.is_empty(),
        "Expected private identifiers and auto-accessors to pass ES2015 gate, got: {diags:?}"
    );
}
