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

#[test]
fn interface_extends_generic_intersection_argument_preserves_nested_generic_close() {
    let source = r#"
type T1 = { a: number };
type Box<T> = { value: T };
type Identifiable<T> = { _id: string } & T;
interface I24 extends Identifiable<T1 & Box<string>> {
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
                "Interface 'I24' incorrectly extends interface 'Identifiable<T1 & Box<string>>'",
            )
        }),
        "TS2430 should preserve the nested generic close in intersection type arguments. Got: {messages:?}"
    );
}

#[test]
fn interface_extends_union_alias_reports_ts2312() {
    let source = r#"
type U = { a: number } | { b: string };
interface I30 extends U { x: string }
"#;
    let diagnostics = diagnostics(source);

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2312
                && message.contains(
                    "An interface can only extend an object type or intersection of object types",
                )
        }),
        "interface heritage over a union alias should emit TS2312. Got: {diagnostics:?}"
    );
}

#[test]
fn interface_extends_tuple_alias_checks_fixed_numeric_member() {
    let source = r#"
type T4 = [string, number];
interface I4 extends T4 { 0: number }
"#;
    let messages: Vec<_> = diagnostics(source)
        .into_iter()
        .filter(|(code, _)| *code == 2430)
        .map(|(_, message)| message)
        .collect();

    assert!(
        messages
            .iter()
            .any(|msg| msg
                .contains("Interface 'I4' incorrectly extends interface '[string, number]'")),
        "tuple fixed element incompatibility should emit TS2430. Got: {messages:?}"
    );
}

#[test]
fn type_query_base_properties_are_checked_against_own_string_index() {
    let source = r#"
declare class CX { static a: string }
type TCX = typeof CX;
interface I14 extends TCX { [x: string]: number }
"#;
    let messages: Vec<_> = diagnostics(source)
        .into_iter()
        .filter(|(code, _)| *code == 2411)
        .map(|(_, message)| message)
        .collect();

    assert!(
        messages.iter().any(|msg| {
            msg.contains(
                "Property 'a' of type 'string' is not assignable to 'string' index type 'number'",
            )
        }),
        "static property from typeof base should be checked against derived string index. Got: {messages:?}"
    );
    assert!(
        messages.iter().any(|msg| {
            msg.contains("Property 'prototype' of type 'CX' is not assignable to 'string' index type 'number'")
        }),
        "prototype property from typeof base should be checked against derived string index. Got: {messages:?}"
    );
}

#[test]
fn mixin_constructor_array_and_tuple_members_emit_ts2416() {
    let source = r#"
interface Array<T> { length: number }
type T3 = number[];
type T4 = [string, number];
type Constructor<T> = new () => T;
declare function Constructor<T>(): Constructor<T>;
class C3 extends Constructor<T3>() { length: string }
class C4 extends Constructor<T4>() { 0: number }
"#;
    let diagnostics = diagnostics(source);
    let messages: Vec<_> = diagnostics
        .into_iter()
        .filter(|(code, _)| *code == 2416)
        .map(|(_, message)| message)
        .collect();

    assert!(
        messages.iter().any(|msg| {
            msg.contains(
                "Property '0' in type 'C4' is not assignable to the same property in base type '[string, number]'",
            )
        }),
        "tuple fixed element override should emit TS2416. Got: {messages:?}"
    );
}
