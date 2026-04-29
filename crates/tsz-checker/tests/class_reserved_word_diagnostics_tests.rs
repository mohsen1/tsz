use tsz_checker::test_utils::check_source_code_messages;

#[test]
fn class_reserved_word_diagnostics_match_strict_class_context() {
    let diagnostics = check_source_code_messages(
        r#"
interface public { }

class C<public, private> {
    constructor(static, let) {
    }
}

class F implements public.private.B { }
class H extends package.A { }
"#,
    );

    assert!(
        diagnostics
            .iter()
            .any(|(code, message)| *code == 1213 && message.contains("'public'")),
        "expected TS1213 for class type parameter `public`; got {diagnostics:#?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|(code, message)| *code == 1213 && message.contains("'private'")),
        "expected TS1213 for class type parameter `private`; got {diagnostics:#?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|(code, message)| *code == 1213 && message.contains("'package'")),
        "expected TS1213 for leftmost heritage identifier `package`; got {diagnostics:#?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|(code, message)| *code == 2702 && message.contains("'public'")),
        "expected TS2702 for type-only heritage left side `public`; got {diagnostics:#?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|(code, message)| *code == 7006 && message.contains("Parameter 'static'")),
        "expected TS7006, not TS7051, for class-context `static` parameter; got {diagnostics:#?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|(code, message)| *code == 7006 && message.contains("Parameter 'let'")),
        "expected TS7006, not TS7051, for class-context `let` parameter; got {diagnostics:#?}"
    );
    assert!(
        diagnostics.iter().all(|(code, _)| *code != 7051),
        "did not expect TS7051 for class-context reserved parameters; got {diagnostics:#?}"
    );
}
