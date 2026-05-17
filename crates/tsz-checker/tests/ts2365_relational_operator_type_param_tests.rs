use tsz_checker::test_utils::check_source_code_messages;

/// For relational operator errors, when a parameter has a TYPE PARAMETER annotation
/// (e.g. `T` in `<T extends number>(x: T, ...)`), tsc shows the TYPE PARAMETER NAME
/// in the diagnostic, not the widened constraint type.
///
/// This verifies tsz matches tsc's behavior: `'T'` appears in the error, not `'number'`.
#[test]
fn ts2365_relational_shows_type_param_name_not_widened_type() {
    let diagnostics = check_source_code_messages(
        r#"
function f<T extends number>(x: T, y: boolean) {
    x < y;
}
"#,
    );

    let ts2365: Vec<String> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2365)
        .map(|(_, msg)| msg.clone())
        .collect();

    assert!(
        !ts2365.is_empty(),
        "expected TS2365 for T vs boolean; got none"
    );
    // tsc shows the type parameter name 'T', not the widened constraint 'number'
    assert!(
        ts2365.iter().any(|m| m.contains("'T'")),
        "expected type parameter name 'T' in error message; got: {ts2365:?}"
    );
}

/// Same as above with a renamed type parameter — proves the behavior is structural,
/// not hardcoded to `T`.
#[test]
fn ts2365_relational_shows_type_param_name_for_any_param_name() {
    let diagnostics_k = check_source_code_messages(
        r#"
function g<K extends boolean>(a: K, b: number) { a < b; }
"#,
    );
    let diagnostics_tnum = check_source_code_messages(
        r#"
function h<TNum extends number>(x: TNum, y: boolean) { x < y; }
"#,
    );

    for (label, diagnostics) in [
        ("K extends boolean", &diagnostics_k),
        ("TNum extends number", &diagnostics_tnum),
    ] {
        let ts2365: Vec<String> = diagnostics
            .iter()
            .filter(|(code, _)| *code == 2365)
            .map(|(_, msg)| msg.clone())
            .collect();
        assert!(!ts2365.is_empty(), "[{label}] expected TS2365; got none");
    }
    // Verify 'K' appears for the K case
    let k_msgs: Vec<String> = diagnostics_k
        .iter()
        .filter(|(c, _)| *c == 2365)
        .map(|(_, m)| m.clone())
        .collect();
    assert!(
        k_msgs.iter().any(|m| m.contains("'K'")),
        "expected 'K' in error for K extends boolean; got: {k_msgs:?}"
    );
}

/// When a parameter has a LITERAL TYPE annotation (e.g. `two: 2`), tsc displays
/// the WIDENED primitive type (`number`) in operator error messages, not the literal `2`.
///
/// This was the root cause of the `relationalOperatorComparable.ts` regression after
/// PR #7783 corrected parser type-node end positions: the annotation span for `2`
/// shrank from `"2,"` (failing alphanumeric check) to `"2"` (passing check), causing
/// the message to show `'2'` instead of `'number'`.
///
/// The fix: skip annotation text for `LITERAL_TYPE` nodes.
#[test]
fn ts2365_relational_literal_type_annotation_shows_widened_type() {
    let diagnostics = check_source_code_messages(
        r#"
function f(onethree: 1 | 3, two: 2) {
    let _a = false < two;
}
"#,
    );

    let ts2365: Vec<String> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2365)
        .map(|(_, msg)| msg.clone())
        .collect();

    assert!(
        !ts2365.is_empty(),
        "expected TS2365 for false < two (typed 2); got none"
    );
    for msg in &ts2365 {
        assert!(
            !msg.contains("'2'"),
            "literal annotation '2' must not appear in error; expected widened 'number'; got: {msg}"
        );
    }
    assert!(
        ts2365.iter().any(|m| m.contains("'number'")),
        "expected widened 'number' in error for literal-typed parameter; got: {ts2365:?}"
    );
}

/// Verify the full `relationalOperatorComparable.ts` pattern: the test function
/// uses `1 | 3` and `2` as literal parameter types and compares them with booleans.
/// All error messages must show widened types (`number`, `boolean`), not literals.
#[test]
fn ts2365_relational_operator_comparable_pattern() {
    let diagnostics = check_source_code_messages(
        r#"
function f(onethree: 1 | 3, two: 2) {
    const t = true;
    const ff = false;
    let _a2 = onethree < true;
    let _a3 = onethree <= ff;
    let _a4 = onethree >= t;
    let _a5 = onethree > ff;
    let _a6 = true < onethree;
    let _a7 = false < two;
    let _a8 = "foo" < onethree;
}
"#,
    );

    let ts2365: Vec<String> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2365)
        .map(|(_, msg)| msg.clone())
        .collect();

    assert!(!ts2365.is_empty(), "expected TS2365 errors; got none");
    // No literal values should appear in the error messages
    for msg in &ts2365 {
        assert!(
            !msg.contains("'2'") && !msg.contains("'1 | 3'"),
            "literal type must not appear in error; got: {msg}"
        );
    }
    // Widened types must be shown
    assert!(
        ts2365
            .iter()
            .any(|m| m.contains("'boolean'") || m.contains("'number'") || m.contains("'string'")),
        "expected widened primitive types in errors; got: {ts2365:?}"
    );
}
