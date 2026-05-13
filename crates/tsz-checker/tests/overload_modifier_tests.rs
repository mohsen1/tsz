//! Tests for overload modifier agreement: TS2383, TS2385, TS2386, TS2394.

use crate::test_utils::check_source_code_messages as get_diagnostics;

fn has_error(source: &str, code: u32) -> bool {
    get_diagnostics(source).iter().any(|d| d.0 == code)
}

// TS2385: access modifier agreement on class method overloads

#[test]
fn ts2385_public_vs_private_method() {
    let source = r#"
class Foo {
    public bar(): void;
    private bar(x?: any) { }
}
"#;
    assert!(has_error(source, 2385));
}

#[test]
fn ts2385_consistent_access_no_error() {
    let source = r#"
class Foo {
    public bar(): void;
    public bar(x?: any) { }
}
"#;
    assert!(!has_error(source, 2385));
}

#[test]
fn ts2385_protected_vs_public() {
    let source = r#"
class Foo {
    protected bar(): void;
    public bar(x?: any) { }
}
"#;
    assert!(has_error(source, 2385));
}

// TS2383: export agreement on function overloads

#[test]
fn ts2383_export_vs_non_export() {
    let source = r#"
declare function baz(): void;
export function baz(s: string): void;
function baz(s?: string) { }
"#;
    assert!(has_error(source, 2383));
}

#[test]
fn ts2383_consistent_export_no_error() {
    let source = r#"
export function baz(): void;
export function baz(s: string): void;
export function baz(s?: string) { }
"#;
    assert!(!has_error(source, 2383));
}

// TS2386: optionality agreement on interface method overloads

#[test]
fn ts2386_optional_vs_required_interface() {
    let source = r#"
interface I {
    foo?(): void;
    foo(s: string): void;
}
"#;
    assert!(has_error(source, 2386));
}

#[test]
fn ts2386_consistent_optionality_no_error() {
    let source = r#"
interface I {
    foo(): void;
    foo(s: string): void;
}
"#;
    assert!(!has_error(source, 2386));
}

#[test]
fn ts2386_class_method_optional_vs_required() {
    let source = r#"
class C {
    foo?(): void;
    foo(x?: any) { }
}
"#;
    assert!(has_error(source, 2386));
}

// TS2394: overload signature must be compatible with implementation signature

#[test]
fn ts2394_type_predicate_overload_with_boolean_impl_no_error() {
    let source = r#"
function check(x: unknown): x is string;
function check(x: unknown, kind: string): x is number;
function check(x: unknown, kind?: string): boolean {
    if (kind === "number") return typeof x === "number";
    return typeof x === "string";
}
"#;
    assert!(!has_error(source, 2394));
}

#[test]
fn ts2394_asserts_predicate_overload_no_error() {
    let source = r#"
function assert(x: unknown): asserts x is string;
function assert(x: unknown, msg?: string): asserts x is string {
    if (typeof x !== "string") throw new Error(msg ?? "not a string");
}
"#;
    assert!(!has_error(source, 2394));
}

#[test]
fn ts2394_incompatible_param_types_still_errors() {
    let source = r#"
function bad(x: string): boolean;
function bad(x: number): boolean {
    return true;
}
"#;
    assert!(has_error(source, 2394));
}
