use tsz_checker::test_utils::check_source_code_messages;

#[test]
fn primitive_type_reference_properties_keep_intrinsic_types() {
    let diagnostics = check_source_code_messages(
        r#"
interface I {
    n: number;
    s: string;
    b: boolean;
    tag: "ok";
}

const ok: I = { n: 1, s: "x", b: true, tag: "ok" };
const badNumber: I = { n: "x", s: "x", b: true, tag: "ok" };
const badTag: I = { n: 1, s: "x", b: true, tag: "no" };
"#,
    );

    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect();
    assert_eq!(
        ts2322.len(),
        2,
        "expected TS2322s for primitive and literal property mismatches, got {diagnostics:?}",
    );
    assert!(
        ts2322.iter().any(
            |(_, message)| message.contains("Type 'string' is not assignable to type 'number'")
        ),
        "expected primitive number target in TS2322, got {ts2322:?}",
    );
    assert!(
        ts2322.iter().any(
            |(_, message)| message.contains("Type '\"no\"' is not assignable to type '\"ok\"'")
        ),
        "expected string-literal target in TS2322, got {ts2322:?}",
    );
}

#[test]
fn composite_and_array_properties_keep_lowered_types() {
    let diagnostics = check_source_code_messages(
        r#"
interface I {
    choice: number | "x";
    list: number[];
    pair: [number, "ok"];
}

const ok: I = { choice: "x", list: [1], pair: [1, "ok"] };
const badChoice: I = { choice: true, list: [1], pair: [1, "ok"] };
const badList: I = { choice: 1, list: ["x"], pair: [1, "ok"] };
const badPair: I = { choice: 1, list: [1], pair: ["x", "ok"] };
"#,
    );

    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect();
    assert_eq!(
        ts2322.len(),
        3,
        "expected TS2322s for union, array, and tuple property mismatches, got {diagnostics:?}",
    );
    assert!(
        ts2322
            .iter()
            .any(|(_, message)| message.contains("Type 'true' is not assignable to type")),
        "expected union target in TS2322, got {ts2322:?}",
    );
    assert!(
        ts2322
            .iter()
            .filter(
                |(_, message)| message.contains("Type 'string' is not assignable to type 'number'")
            )
            .count()
            >= 2,
        "expected array and tuple number targets in TS2322, got {ts2322:?}",
    );
}
