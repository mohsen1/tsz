use tsz_checker::test_utils::check_source_code_messages;

fn diagnostics(source: &str) -> Vec<(u32, String)> {
    check_source_code_messages(source)
}

#[test]
fn interface_extends_typeof_alias_displays_target_type() {
    let source = r#"
declare class CX { static a: string }
type TCX = typeof CX;
interface I10 extends TCX {
    a: number;
}
"#;
    let messages: Vec<_> = diagnostics(source)
        .into_iter()
        .filter(|(code, _)| *code == 2430)
        .map(|(_, message)| message)
        .collect();

    assert!(
        messages
            .iter()
            .any(|msg| msg.contains("Interface 'I10' incorrectly extends interface 'typeof CX'")),
        "TS2430 should display the typeof alias target, not TCX. Got: {messages:?}"
    );
    assert!(
        !messages.iter().any(|msg| msg.contains("interface 'TCX'")),
        "TS2430 should not display the local alias name. Got: {messages:?}"
    );
}

#[test]
fn interface_extends_generic_intersection_argument_preserves_surface_text() {
    let source = r#"
type T1 = { a: number };
type Identifiable<T> = { _id: string } & T;
interface I23 extends Identifiable<T1 & { b: number}> {
    a: string;
}
"#;
    let messages: Vec<_> = diagnostics(source)
        .into_iter()
        .filter(|(code, _)| *code == 2430)
        .map(|(_, message)| message)
        .collect();

    assert!(
        messages.iter().any(|msg| {
            msg.contains(
                "Interface 'I23' incorrectly extends interface 'Identifiable<T1 & { b: number; }>'",
            )
        }),
        "TS2430 should preserve the explicit intersection type argument. Got: {messages:?}"
    );
}
