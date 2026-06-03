//! Probe tests for index signature type checking with control flow.
//! Investigates controlFlowForIndexSignatures.ts failure.

use tsz_checker::test_utils::check_source_diagnostics;

fn codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn basic_index_signature_assignment_ok() {
    let c = codes(
        r#"
interface StringMap { [key: string]: string }
declare const m: StringMap;
m["x"] = "hello";
"#,
    );
    assert!(c.is_empty(), "expected no errors, got: {:?}", c);
}

#[test]
fn index_signature_wrong_type_assignment_ts2322() {
    let c = codes(
        r#"
interface StringMap { [key: string]: string }
declare const m: StringMap;
m["x"] = 42;
"#,
    );
    assert!(c.contains(&2322), "expected TS2322, got: {:?}", c);
}

#[test]
fn union_index_signature_access_ts2322() {
    // Narrowed index signature type union
    let c = codes(
        r#"
interface StringMap { [key: string]: string }
interface NumberMap { [key: string]: number }
declare const m: StringMap | NumberMap;
const v: string = m["x"];
"#,
    );
    assert!(c.contains(&2322), "expected TS2322, got: {:?}", c);
}

#[test]
fn control_flow_index_signature_narrowed_assignment() {
    let c = codes(
        r#"
interface StringMap { [key: string]: string }
interface NumberMap { [key: string]: number }
function isStringMap(x: StringMap | NumberMap): x is StringMap {
    return true;
}
declare const m: StringMap | NumberMap;
if (isStringMap(m)) {
    const v: string = m["x"];
}
const v: string = m["x"];
"#,
    );
    assert!(
        c.contains(&2322),
        "expected TS2322 for post-if access, got: {:?}",
        c
    );
}

#[test]
fn index_signature_in_generic_constraint_ts2322() {
    let c = codes(
        r#"
interface HasStrings { [key: string]: string }
function assignNumber<T extends HasStrings>(m: T, key: string) {
    m[key] = 42;
}
"#,
    );
    assert!(c.contains(&2322), "expected TS2322, got: {:?}", c);
}
