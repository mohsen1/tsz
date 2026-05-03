use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

#[test]
fn typeof_type_literal_call_signature_parameter_uses_declared_type() {
    let source = r#"
function test1(a: number | string) {
  if (typeof a === "number") {
    const fn = (arg: typeof a) => true;
    return fn;
  }
  return;
}

test1(0)?.(100);
test1(0)?.("");

function test2(a: number | string) {
  if (typeof a === "number") {
    const fn: { (arg: typeof a): boolean; } = () => true;
    return fn;
  }
  return;
}

test2(0)?.(100);
test2(0)?.("");

function test3(a: number | string) {
  if (typeof a === "number") {
    return (arg: typeof a) => {};
  }
  throw "";
}

test3(1)(100);
test3(1)("");
"#;

    let diags = check_source(source, "test.ts", CheckerOptions::default());
    let ts2345: Vec<_> = diags.iter().filter(|d| d.code == 2345).collect();
    assert_eq!(
        ts2345.len(),
        2,
        "Expected TS2345 only for test1/test3 string calls, got: {diags:#?}"
    );

    let test1_string_arg =
        source.find("test1(0)?.(\"\")").unwrap() as u32 + "test1(0)?.(".len() as u32;
    let test2_string_arg =
        source.find("test2(0)?.(\"\")").unwrap() as u32 + "test2(0)?.(".len() as u32;
    let test3_string_arg = source.find("test3(1)(\"\")").unwrap() as u32 + "test3(1)(".len() as u32;

    assert!(
        ts2345.iter().any(|d| d.start == test1_string_arg),
        "Expected TS2345 on test1 string argument, got: {ts2345:#?}"
    );
    assert!(
        !ts2345.iter().any(|d| d.start == test2_string_arg),
        "Did not expect TS2345 on test2 explicit call-signature argument, got: {ts2345:#?}"
    );
    assert!(
        ts2345.iter().any(|d| d.start == test3_string_arg),
        "Expected TS2345 on test3 string argument, got: {ts2345:#?}"
    );
}
