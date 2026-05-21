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

// TS2383: only 2+ bodyless overload signatures must agree on export status.
// The implementation is not an overload signature; tsc does not check it.
// Single-overload + implementation export mismatches are silently accepted.

#[test]
fn ts2383_single_overload_exported_implementation_not_exported_no_error() {
    let source = r#"
export function compute(x: number): number;
function compute(x: any): number { return x; }
"#;
    assert!(!has_error(source, 2383));
}

#[test]
fn ts2383_single_overload_not_exported_implementation_exported_no_error() {
    let source = r#"
function transform(x: number): number;
export function transform(x: any): number { return x; }
"#;
    assert!(!has_error(source, 2383));
}

#[test]
fn ts2383_single_overload_both_exported_no_error() {
    let source = r#"
export function process(x: number): number;
export function process(x: any): number { return x; }
"#;
    assert!(!has_error(source, 2383));
}

#[test]
fn ts2383_single_overload_both_non_exported_no_error() {
    let source = r#"
function process(x: number): number;
function process(x: any): number { return x; }
"#;
    assert!(!has_error(source, 2383));
}

#[test]
fn ts2383_single_overload_impl_export_mismatch_different_name_no_error() {
    let source = r#"
export function handle(x: string): string;
function handle(x: any): string { return x; }
"#;
    assert!(!has_error(source, 2383));
}

#[test]
fn ts2383_single_overload_reversed_impl_export_mismatch_no_error() {
    let source = r#"
function serialize(x: number): string;
export function serialize(x: any): string { return String(x); }
"#;
    assert!(!has_error(source, 2383));
}

#[test]
fn ts2383_three_overloads_all_exported_no_error() {
    let source = r#"
export function dispatch(x: number): void;
export function dispatch(x: string): void;
export function dispatch(x: any): void {}
"#;
    assert!(!has_error(source, 2383));
}

#[test]
fn ts2383_two_overloads_one_not_exported_error() {
    let source = r#"
export function route(x: number): void;
function route(x: string): void;
export function route(x: any): void {}
"#;
    assert!(has_error(source, 2383));
}

#[test]
fn ts2383_two_exported_overloads_non_exported_impl_no_error() {
    // Implementation export status is not checked — only bodyless signatures matter.
    let source = r#"
export function compute(x: number): number;
export function compute(x: string): number;
function compute(x: any): number { return 0; }
"#;
    assert!(!has_error(source, 2383));
}

#[test]
fn ts2383_two_non_exported_overloads_exported_impl_no_error() {
    let source = r#"
function transform(x: number): string;
function transform(x: string): string;
export function transform(x: any): string { return ""; }
"#;
    assert!(!has_error(source, 2383));
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
fn ts2394_constructor_rest_overload_accepts_broader_impl_first_param() {
    let source = r#"
class RestConstruct {
  values: number[];

  constructor(...values: number[]);
  constructor(first: string, ...rest: number[]);
  constructor(firstOrNum: string | number, ...rest: number[]) {
    if (typeof firstOrNum === 'string') {
      this.values = rest;
    } else {
      this.values = [firstOrNum, ...rest];
    }
  }
}

const rc1 = new RestConstruct(1, 2, 3);
const rc2 = new RestConstruct('label', 1, 2, 3);
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
