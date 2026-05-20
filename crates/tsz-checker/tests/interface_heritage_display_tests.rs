use tsz_checker::test_utils::check_source_code_messages as diagnostics;

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

/// When an interface extends another, tsc displays the derived (own)
/// members first in declaration order, followed by inherited base members.
/// Test the structural rule by varying the iteration variable name in the
/// mapped type — if the rule is hardcoded against a name spelling, switching
/// it would break the assertion.
#[test]
fn interface_extends_displays_own_members_before_inherited() {
    fn check_derived_first(iter_var: &str) {
        let source = format!(
            r#"
interface Base {{
    inherited_one: number;
    inherited_two: number;
}}
interface Mid extends Base {{
    mid_one: number;
    mid_two: number;
}}
interface Derived extends Mid {{
    own_one: number;
    own_two: number;
}}
type Spread<T> = {{ [{iter_var} in keyof T]: T[{iter_var}] }};
declare let d: Derived;
declare function consume<T>(spread: Spread<T>): T;
const out = consume(d);
const fail: string = out;
"#,
        );
        let diags = diagnostics(&source);
        let display = diags
            .into_iter()
            .find(|(code, _)| *code == 2322)
            .map(|(_, msg)| msg)
            .unwrap_or_default();
        assert!(
            !display.is_empty(),
            "Expected TS2322 diagnostic showing inferred T (iter_var='{iter_var}'). Got nothing."
        );

        let positions: Vec<_> = [
            "own_one",
            "own_two",
            "mid_one",
            "mid_two",
            "inherited_one",
            "inherited_two",
        ]
        .iter()
        .map(|name| {
            display.find(name).unwrap_or_else(|| {
                panic!(
                    "Property '{name}' missing from TS2322 message (iter_var='{iter_var}'). Got: {display}"
                )
            })
        })
        .collect();

        for window in positions.windows(2) {
            assert!(
                window[0] < window[1],
                "Expected derived-first member order in inferred T (iter_var='{iter_var}'). \
                 Got: {display}"
            );
        }
    }

    // Run with two different iteration-variable names so the test would catch
    // any printer/solver heuristic that happened to be keyed on a specific name.
    check_derived_first("K");
    check_derived_first("P");
}

/// Method overrides (where a derived interface re-declares a method that the
/// base also declares with extra signatures) must keep the derived position.
/// Ensures `merge_properties` keeps the override at the derived (low)
/// `declaration_order` rather than collapsing it back into the inherited slot.
#[test]
fn interface_method_override_keeps_derived_position() {
    let source = r#"
interface Base {
    addEv(t: string): void;
    other: number;
}
interface Derived extends Base {
    own_first: number;
    addEv<K>(t: K): void;
}
type Spread<T> = { [K in keyof T]: T[K] };
declare let d: Derived;
declare function consume<T>(spread: Spread<T>): T;
const out = consume(d);
const fail: string = out;
"#;
    let display = diagnostics(source)
        .into_iter()
        .find(|(code, _)| *code == 2322)
        .map(|(_, msg)| msg)
        .unwrap_or_default();
    assert!(!display.is_empty(), "Expected TS2322 diagnostic.");

    // The override `addEv` is declared in Derived after `own_first`, so it
    // should appear after `own_first` and before any base-only members.
    let pos_own = display
        .find("own_first")
        .expect("own_first should be in display");
    let pos_addev = display.find("addEv").expect("addEv should be in display");
    let pos_other = display.find("other").expect("other should be in display");

    assert!(
        pos_own < pos_addev,
        "own_first should precede addEv override. Got: {display}"
    );
    assert!(
        pos_addev < pos_other,
        "addEv (override of base method, kept at derived position) should precede base-only 'other'. Got: {display}"
    );
}
