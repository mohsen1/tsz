use crate::context::{CheckerOptions, ScriptTarget};
use crate::diagnostics::Diagnostic;
use crate::test_utils::{check_source_with_libs, load_default_lib_files};

fn diagnostics_with_code(diagnostics: &[Diagnostic], code: u32) -> Vec<&Diagnostic> {
    diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == code)
        .collect()
}

#[test]
fn ts2635_lib_constructor_instantiation_uses_annotation_identity_display() {
    let libs = load_default_lib_files();
    let diags = check_source_with_libs(
        r#"
function f() {
    const A = Array<string, number>;
}
"#,
        "test.ts",
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
        &libs,
    );

    let ts2635 = diagnostics_with_code(&diags, 2635);
    assert_eq!(ts2635.len(), 1, "Expected one TS2635, got: {diags:?}");
    let message = &ts2635[0].message_text;
    assert!(
        message.contains("Type 'ArrayConstructor' has no signatures"),
        "Expected TS2635 to display the lib constructor annotation name, got: {message:?}"
    );
    assert!(
        !message.contains("arrayLength"),
        "TS2635 should not expand the lib constructor into structural call signatures, got: {message:?}"
    );
}
