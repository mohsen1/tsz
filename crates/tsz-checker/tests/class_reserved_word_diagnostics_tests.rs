use tsz_checker::test_utils::{check_js_source_code_messages, check_source_code_messages};

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
            .any(|(code, message)| *code == 1213 && message.contains("'static'")),
        "expected TS1213 for constructor parameter `static`; got {diagnostics:#?}"
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

#[test]
fn js_module_let_named_lexical_declaration_uses_ts2480_not_ts1214() {
    let diagnostics = check_js_source_code_messages(
        r#"
export const marker = 0;
let let = 1;
const yield = 2;
"#,
    );

    assert!(
        diagnostics
            .iter()
            .any(|(code, message)| *code == 2480 && message.contains("'let'")),
        "expected TS2480 for `let` as a lexical binding name; got {diagnostics:#?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|(code, message)| *code == 1214 && message.contains("'yield'")),
        "expected adjacent JS module `yield` binding to keep TS1214; got {diagnostics:#?}"
    );
    assert!(
        diagnostics
            .iter()
            .all(|(code, message)| *code != 1214 || !message.contains("'let'")),
        "did not expect TS1214 for `let` as a lexical binding name; got {diagnostics:#?}"
    );
}
