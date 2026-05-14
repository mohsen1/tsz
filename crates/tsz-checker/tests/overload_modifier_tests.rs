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

#[test]
fn ts2394_type_predicate_overloads_with_predicate_impl_no_error() {
    // #6177: type predicate overloads compatible with broader predicate implementation
    let source = r#"
function unionOverload(x: string | number): x is string;
function unionOverload(x: object): x is object & { id: number };
function unionOverload(x: unknown): x is unknown {
    return typeof x === "string";
}
"#;
    assert!(!has_error(source, 2394));
}

#[test]
fn ts2394_type_predicate_overloads_narrowing_variety() {
    // All overload predicates compatible with broader implementation predicate.
    let source = r#"
function check(val: string): val is string;
function check(val: number): val is number;
function check(val: unknown): val is unknown {
    return true;
}
"#;
    assert!(!has_error(source, 2394));
}

// TS2394: callback parameter contravariance in overload-implementation checking.
// Inline structural types: class-type resolution isn't needed to exercise contravariance.

#[test]
fn ts2394_callback_narrower_in_overload_than_impl_errors() {
    let source = r#"
function handle(cb: (x: { kind: string; bark(): void }) => void): void;
function handle(cb: (x: { kind: string }) => void): void {}
"#;
    assert!(has_error(source, 2394));
}

#[test]
fn ts2394_callback_narrower_in_overload_different_names_errors() {
    // Different property names — proves no hardcoding on specific identifiers.
    let source = r#"
function process(fn: (v: { id: number; name: string }) => void): void;
function process(fn: (v: { id: number }) => void): void {}
"#;
    assert!(has_error(source, 2394));
}

#[test]
fn ts2394_callback_wider_in_overload_than_impl_no_error() {
    let source = r#"
function handle(cb: (x: { kind: string }) => void): void;
function handle(cb: (x: { kind: string; bark(): void }) => void): void {}
"#;
    assert!(!has_error(source, 2394));
}

#[test]
fn ts2394_callback_same_type_no_error() {
    let source = r#"
function on(cb: (e: { ts: number }) => void): void;
function on(cb: (e: { ts: number }) => void): void {}
"#;
    assert!(!has_error(source, 2394));
}

#[test]
fn ts2394_multiple_overloads_one_incompatible_callback_errors() {
    let source = r#"
function listen(kind: "any", cb: (e: { ts: number }) => void): void;
function listen(kind: "click", cb: (e: { ts: number; x: number }) => void): void;
function listen(kind: string, cb: (e: { ts: number }) => void): void {}
"#;
    assert!(has_error(source, 2394));
}

#[test]
fn ts2394_multiple_overloads_all_compatible_callbacks_no_error() {
    let source = r#"
function listen(kind: "any", cb: (e: { ts: number }) => void): void;
function listen(kind: "click", cb: (e: { ts: number }) => void): void;
function listen(kind: string, cb: (e: { ts: number }) => void): void {}
"#;
    assert!(!has_error(source, 2394));
}
