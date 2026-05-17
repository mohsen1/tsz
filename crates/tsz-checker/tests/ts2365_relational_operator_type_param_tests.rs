use tsz_checker::test_utils::check_source_code_messages;

/// When a relational operator is applied to a generic type parameter, tsc shows
/// the widened/constrained type in the error message, not the type parameter name.
/// This applies to all short parameter names (T, K, U, etc.) that would otherwise
/// pass the annotation-text length gate.
#[test]
fn ts2365_relational_shows_widened_not_type_param_name() {
    let diagnostics = check_source_code_messages(
        r#"
function f<T extends number>(x: T, y: boolean) {
    x < y;
    x <= y;
}
function g<K extends boolean>(a: K, b: number) {
    a < b;
}
"#,
    );

    let ts2365: Vec<String> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2365)
        .map(|(_, msg)| msg.clone())
        .collect();

    assert!(!ts2365.is_empty(), "expected TS2365 errors; got none");
    for msg in &ts2365 {
        assert!(
            !msg.contains("'T'") && !msg.contains("'K'"),
            "error should show widened constraint type, not type parameter name; got: {msg}"
        );
    }
    assert!(
        ts2365
            .iter()
            .any(|m| m.contains("'number'") && m.contains("'boolean'")),
        "expected message with 'number' and 'boolean'; got: {ts2365:?}"
    );
}

/// The fix must work regardless of the type parameter name, including multi-character names
/// that already returned None via the length check. Single-letter names (T, K) are the
/// key regression path fixed by the type-parameter detection.
#[test]
fn ts2365_relational_shows_widened_for_any_type_param_name() {
    let single_letter = check_source_code_messages(
        r#"
function f<T extends number>(x: T, y: boolean) { x < y; }
"#,
    );
    let multi_letter = check_source_code_messages(
        r#"
function f<TNum extends number>(x: TNum, y: boolean) { x < y; }
"#,
    );

    for (label, diagnostics) in [
        ("single-letter T", &single_letter),
        ("multi-letter TNum", &multi_letter),
    ] {
        let msgs: Vec<String> = diagnostics
            .iter()
            .filter(|(code, _)| *code == 2365)
            .map(|(_, msg)| msg.clone())
            .collect();
        assert!(
            !msgs.is_empty(),
            "[{label}] expected TS2365 errors; got none"
        );
        for msg in &msgs {
            assert!(
                !msg.contains("'T'") && !msg.contains("'TNum'"),
                "[{label}] error should show constraint type, not parameter name; got: {msg}"
            );
            assert!(
                msg.contains("'number'"),
                "[{label}] expected 'number' in error; got: {msg}"
            );
        }
    }
}

/// Non-generic parameters with concrete type annotations should be unaffected:
/// a parameter `(x: number)` still triggers TS2365 with 'number' in the message.
#[test]
fn ts2365_relational_concrete_param_type_still_works() {
    let diagnostics = check_source_code_messages(
        r#"
function f(x: number, y: boolean) { x < y; }
"#,
    );

    let ts2365: Vec<String> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2365)
        .map(|(_, msg)| msg.clone())
        .collect();

    assert!(
        !ts2365.is_empty(),
        "expected TS2365 for concrete number vs boolean; got none"
    );
    assert!(
        ts2365
            .iter()
            .any(|m| m.contains("'number'") && m.contains("'boolean'")),
        "expected 'number' and 'boolean' in error; got: {ts2365:?}"
    );
}
