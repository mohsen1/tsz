use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

#[test]
fn bare_alias_to_generic_class_default_keeps_alias_display() {
    let source = r#"
declare class TableClass<S = any> {
    _field: S;
}

type Table = TableClass;

declare const o: Table;
let value: boolean = o;
"#;

    let diagnostics = check_source(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            strict_function_types: true,
            ..CheckerOptions::default()
        },
    );
    let diag = diagnostics
        .iter()
        .find(|diag| diag.code == 2322)
        .expect("expected TS2322 for assigning Table to boolean");
    assert!(
        diag.message_text
            .contains("Type 'Table' is not assignable to type 'boolean'."),
        "alias declaration diagnostic should keep the source alias, got: {diag:?}"
    );
    assert!(
        !diag.message_text.contains("TableClass<any>"),
        "alias declaration diagnostic should not expand to the generic class default, got: {diag:?}"
    );
}

#[test]
fn anonymous_empty_object_target_is_not_repainted_as_mapped_alias_reduction() {
    let source = r#"
type T50<T> = { [P in keyof T]: number };
type T52 = T50<unknown>;

function f22(x: unknown) {
    let v: {} = x;
}

function f30<T, U extends unknown>(t: T, u: U) {
    let x: {} = t;
    let y: {} = u;
}

function oops<T extends unknown>(arg: T): {} {
    return arg;
}
"#;

    let diagnostics = check_source(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            strict_function_types: true,
            ..CheckerOptions::default()
        },
    );
    let ts2322_messages: Vec<_> = diagnostics
        .iter()
        .filter(|diag| diag.code == 2322)
        .map(|diag| diag.message_text.as_str())
        .collect();
    assert_eq!(
        ts2322_messages.len(),
        4,
        "expected four TS2322 diagnostics for unknown/type-param to {{}}, got: {diagnostics:#?}"
    );
    assert!(
        ts2322_messages
            .iter()
            .all(|message| message.contains("not assignable to type '{}'.")
                && !message.contains("T50<unknown>")),
        "anonymous {{}} targets must not inherit the display alias from T50<unknown>: {ts2322_messages:#?}"
    );
}

/// Shared options + TS2322 message extraction for the bare-nominal-alias
/// display fences below.
fn ts2322_messages_for(source: &str) -> Vec<String> {
    let diagnostics = check_source(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            strict_function_types: true,
            ..CheckerOptions::default()
        },
    );
    diagnostics
        .iter()
        .filter(|diag| diag.code == 2322)
        .map(|diag| diag.message_text.clone())
        .collect()
}

#[test]
fn bare_alias_to_interface_displays_interface_name() {
    let messages = ts2322_messages_for(
        r#"
interface Iface { a: string }
type IA = Iface;
declare const s1: string;
const x1: IA = s1;
"#,
    );
    assert_eq!(messages.len(), 1, "expected one TS2322: {messages:#?}");
    assert!(
        messages[0].contains("Type 'string' is not assignable to type 'Iface'."),
        "bare alias to a non-generic interface renders the interface name, got: {messages:#?}"
    );
}

#[test]
fn bare_alias_to_class_displays_class_name() {
    let messages = ts2322_messages_for(
        r#"
declare class Widget { m: string }
type CA = Widget;
declare const s1: string;
const x2: CA = s1;
"#,
    );
    assert_eq!(messages.len(), 1, "expected one TS2322: {messages:#?}");
    assert!(
        messages[0].contains("Type 'string' is not assignable to type 'Widget'."),
        "bare alias to a non-generic class renders the class name, got: {messages:#?}"
    );
}

#[test]
fn bare_alias_chain_to_interface_displays_interface_name() {
    let messages = ts2322_messages_for(
        r#"
interface Iface { a: string }
type IA = Iface;
type IB = IA;
declare const s1: string;
const x3: IB = s1;
"#,
    );
    assert_eq!(messages.len(), 1, "expected one TS2322: {messages:#?}");
    assert!(
        messages[0].contains("Type 'string' is not assignable to type 'Iface'."),
        "alias chain to a non-generic interface renders the interface name, got: {messages:#?}"
    );
}

#[test]
fn bare_alias_to_interface_displays_interface_name_in_nested_elaboration() {
    let messages = ts2322_messages_for(
        r#"
interface Iface { a: string }
interface Deep { inner: Iface }
type DA = Deep;
declare const s1: string;
const x6: DA = { inner: s1 };
"#,
    );
    assert_eq!(messages.len(), 1, "expected one TS2322: {messages:#?}");
    assert!(
        messages[0].contains("Type 'string' is not assignable to type 'Iface'."),
        "nested elaboration renders the interface name for the property target, got: {messages:#?}"
    );
}

#[test]
fn bare_alias_to_interface_displays_interface_name_as_source() {
    let messages = ts2322_messages_for(
        r#"
interface Iface { a: string }
type IA = Iface;
declare const src: IA;
const x8: number = src;
"#,
    );
    assert_eq!(messages.len(), 1, "expected one TS2322: {messages:#?}");
    assert!(
        messages[0].contains("Type 'Iface' is not assignable to type 'number'."),
        "source-position bare alias renders the interface name, got: {messages:#?}"
    );
}

#[test]
fn alias_to_generic_interface_instantiation_keeps_alias_display() {
    let messages = ts2322_messages_for(
        r#"
interface Box<T> { v: T }
type GA = Box<string>;
declare const s1: string;
const x4: GA = s1;
"#,
    );
    assert_eq!(messages.len(), 1, "expected one TS2322: {messages:#?}");
    assert!(
        messages[0].contains("Type 'string' is not assignable to type 'GA'."),
        "alias of a generic instantiation keeps the alias name, got: {messages:#?}"
    );
}

#[test]
fn generic_alias_of_generic_interface_keeps_alias_display() {
    let messages = ts2322_messages_for(
        r#"
interface Box<T> { v: T }
type GBox<T> = Box<T>;
declare const s1: string;
const x5: GBox<number> = s1;
"#,
    );
    assert_eq!(messages.len(), 1, "expected one TS2322: {messages:#?}");
    assert!(
        messages[0].contains("Type 'string' is not assignable to type 'GBox<number>'."),
        "generic alias forwarding a generic interface keeps its own applied name, got: {messages:#?}"
    );
}

#[test]
fn alias_to_inline_object_literal_keeps_alias_display() {
    let messages = ts2322_messages_for(
        r#"
type OA = { a: string };
declare const s1: string;
const x9: OA = s1;
"#,
    );
    assert_eq!(messages.len(), 1, "expected one TS2322: {messages:#?}");
    assert!(
        messages[0].contains("Type 'string' is not assignable to type 'OA'."),
        "alias of a fresh inline object literal keeps the alias name, got: {messages:#?}"
    );
}

#[test]
fn bare_alias_to_interface_displays_interface_name_renamed_binders() {
    let messages = ts2322_messages_for(
        r#"
interface Zq9 { qq: number }
type Aliased_0 = Zq9;
declare const nn: boolean;
const w1: Aliased_0 = nn;
"#,
    );
    assert_eq!(messages.len(), 1, "expected one TS2322: {messages:#?}");
    assert!(
        messages[0].contains("Type 'boolean' is not assignable to type 'Zq9'."),
        "renamed-binder bare interface alias renders the interface name, got: {messages:#?}"
    );
}

#[test]
fn bare_alias_source_display_matrix_matches_oracle() {
    // One-file matrix mirroring the tsc 6.0.2 oracle byte-for-byte: bare
    // aliases of non-generic interfaces/classes render the declaration name
    // (including chains); instantiations, inline literals, generic aliases,
    // and defaulted-generic bare refs keep the alias spelling; enum aliases
    // keep the #17705 behavior.
    let messages = ts2322_messages_for(
        r#"
interface Iface { a: string }
type IA = Iface;
declare const src1: IA;
const y1: number = src1;

declare class Widget { m: string }
type CA = Widget;
declare const src2: CA;
const y2: number = src2;

type IB = IA;
declare const src3: IB;
const y3: number = src3;

interface Box<T> { v: T }
type GA = Box<string>;
declare const src4: GA;
const y4: number = src4;

type GBox<T> = Box<T>;
declare const src5: GBox<number>;
const y5: number = src5;

type OA = { a: string };
declare const src6: OA;
const y6: number = src6;

class GC<T = string> { t!: T }
type GCA = GC;
declare const src7: GCA;
const y7: number = src7;

enum Mode { A = 1, B = 2 }
type MA = Mode;
declare const src8: MA;
const y8: string = src8;
"#,
    );
    let expected = [
        "Type 'Iface' is not assignable to type 'number'.",
        "Type 'Widget' is not assignable to type 'number'.",
        "Type 'Iface' is not assignable to type 'number'.",
        "Type 'GA' is not assignable to type 'number'.",
        "Type 'GBox<number>' is not assignable to type 'number'.",
        "Type 'OA' is not assignable to type 'number'.",
        "Type 'GCA' is not assignable to type 'number'.",
        "Type 'Mode' is not assignable to type 'string'.",
    ];
    assert_eq!(
        messages.len(),
        expected.len(),
        "expected {} TS2322 diagnostics, got: {messages:#?}",
        expected.len()
    );
    for (message, expected) in messages.iter().zip(expected) {
        assert!(
            message.contains(expected),
            "expected {expected:?} in {message:?} (full set: {messages:#?})"
        );
    }
}

#[test]
fn bare_alias_to_class_displays_class_name_as_source() {
    let messages = ts2322_messages_for(
        r#"
declare class Widget { m: string }
type CA = Widget;
declare const src2: CA;
const y2: number = src2;
"#,
    );
    assert_eq!(messages.len(), 1, "expected one TS2322: {messages:#?}");
    assert!(
        messages[0].contains("Type 'Widget' is not assignable to type 'number'."),
        "source-position bare class alias renders the class name, got: {messages:#?}"
    );
}

#[test]
fn bare_alias_chain_displays_interface_name_as_source() {
    let messages = ts2322_messages_for(
        r#"
interface Iface { a: string }
type IA = Iface;
type IB = IA;
declare const src3: IB;
const y3: number = src3;
"#,
    );
    assert_eq!(messages.len(), 1, "expected one TS2322: {messages:#?}");
    assert!(
        messages[0].contains("Type 'Iface' is not assignable to type 'number'."),
        "source-position alias chain renders the interface name, got: {messages:#?}"
    );
}

#[test]
fn bare_alias_declared_before_interface_displays_interface_name_as_source() {
    let messages = ts2322_messages_for(
        r#"
type Pre = Later;
interface Later { b: number }
declare const p: Pre;
const z1: number = p;
"#,
    );
    assert_eq!(messages.len(), 1, "expected one TS2322: {messages:#?}");
    assert!(
        messages[0].contains("Type 'Later' is not assignable to type 'number'."),
        "alias declared before its interface renders the interface name, got: {messages:#?}"
    );
}

#[test]
fn bare_alias_to_namespace_qualified_interface_displays_interface_name() {
    let messages = ts2322_messages_for(
        r#"
namespace NS { export interface QI { q: string } }
type NA = NS.QI;
declare const na: NA;
const z2: number = na;
"#,
    );
    assert_eq!(messages.len(), 1, "expected one TS2322: {messages:#?}");
    assert!(
        messages[0].contains("Type 'QI' is not assignable to type 'number'."),
        "namespace-qualified bare interface alias renders the bare interface name, got: {messages:#?}"
    );
}

#[test]
fn bare_alias_to_call_signature_interface_displays_interface_name_as_source() {
    let messages = ts2322_messages_for(
        r#"
interface Fn { (): void }
type FA = Fn;
declare const fa: FA;
const z4: number = fa;
"#,
    );
    assert_eq!(messages.len(), 1, "expected one TS2322: {messages:#?}");
    assert!(
        messages[0].contains("Type 'Fn' is not assignable to type 'number'."),
        "call-signature-only bare interface alias renders the interface name, got: {messages:#?}"
    );
}

#[test]
fn bare_alias_argument_positions_display_interface_name() {
    let source = r#"
interface Iface { a: string }
type IA = Iface;
declare const src1: IA;
declare function wantsNum(n: number): void;
wantsNum(src1);
declare function wantsIA(v: IA): void;
wantsIA(123);
"#;
    let diagnostics = check_source(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            strict_function_types: true,
            ..CheckerOptions::default()
        },
    );
    let messages: Vec<_> = diagnostics
        .iter()
        .filter(|diag| diag.code == 2345)
        .map(|diag| diag.message_text.clone())
        .collect();
    assert_eq!(messages.len(), 2, "expected two TS2345: {messages:#?}");
    assert!(
        messages[0]
            .contains("Argument of type 'Iface' is not assignable to parameter of type 'number'."),
        "TS2345 source position renders the interface name, got: {messages:#?}"
    );
    assert!(
        messages[1]
            .contains("Argument of type 'number' is not assignable to parameter of type 'Iface'."),
        "TS2345 parameter position renders the interface name, got: {messages:#?}"
    );
}
