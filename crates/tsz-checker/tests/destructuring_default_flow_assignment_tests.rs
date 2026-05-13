use tsz_checker::context::CheckerOptions;

fn check_strict(source: &str) -> Vec<(u32, String)> {
    let options = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        no_implicit_any: true,
        ..Default::default()
    };
    tsz_checker::test_utils::check_source(source, "test.ts", options)
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

#[test]
fn defaulted_destructuring_assignments_update_property_receiver_flow() {
    let diagnostics = check_strict(
        r#"
export {};

let a: string | number = "before";
let b: string | number = "before";
declare const tupleSource: [number?];
let c: string | number = "before";
declare const objectSource: { y?: number };
let d: string | number = "before";

if (typeof a === "string") {
  [a = 1] = [];
  const aNumber: number = a;
  const aString: string = a;
}

if (typeof b === "string") {
  [b = 1] = [undefined];
  const bNumber: number = b;
  const bString: string = b;
}

if (typeof c === "string") {
  [c = 1] = tupleSource;
  const cNumber: number = c;
  const cString: string = c;
}

if (typeof d === "string") {
  ({ y: d = 1 } = objectSource);
  const dNumber: number = d;
  const dString: string = d;
}
"#,
    );

    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .map(|(_, message)| message.as_str())
        .collect();
    let ts18048: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 18048)
        .map(|(_, message)| message.as_str())
        .collect();

    assert!(
        ts18048.is_empty(),
        "defaulted destructuring should not leave receivers possibly undefined: {diagnostics:#?}"
    );

    assert_eq!(
        ts2322.len(),
        4,
        "expected only the string assignment errors after defaulted destructuring flow, got {diagnostics:#?}"
    );
    assert!(
        ts2322.iter().all(|message| {
            message.contains("string")
                && !message.contains("number' is not assignable to type 'number")
        }),
        "defaulted destructuring should narrow each receiver before assignment checks: {ts2322:#?}"
    );
}
