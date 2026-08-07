//! Tests for overload modifier agreement: TS2383, TS2385, TS2386, TS2394.

use tsz_checker::test_utils::check_source_code_messages as get_diagnostics;
use tsz_checker::test_utils::check_source_diagnostics;

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

// TS2383: overloads declared directly inside an ambient module/namespace body
// are exempt from the export-consistency check (tsc: `export` on a member of
// an ambient module body doesn't carry module-export meaning). `declare
// global` is a global augmentation, not an ambient module, and stays subject
// to the check, as does a bare top-level ambient overload set with no
// enclosing namespace.

#[test]
fn ts2383_ambient_namespace_mixed_export_no_error() {
    let source = r#"
declare namespace Widgets {
    function make(): void;
    export function make(): void;
    function make(): void;
}
"#;
    assert!(!has_error(source, 2383));
}

#[test]
fn ts2383_non_ambient_namespace_mixed_export_error() {
    let source = r#"
namespace Widgets {
    function make(): void;
    export function make(): void;
    function make(): void {}
}
"#;
    assert!(has_error(source, 2383));
}

#[test]
fn ts2383_ambient_module_string_literal_mixed_export_no_error() {
    let source = r#"
declare module "widgets" {
    function make(): void;
    export function make(): void;
}
"#;
    assert!(!has_error(source, 2383));
}

#[test]
fn ts2383_non_ambient_namespace_nested_inside_ambient_namespace_no_error() {
    let source = r#"
declare namespace Outer {
    namespace Inner {
        function make(): void;
        export function make(): void;
    }
}
"#;
    assert!(!has_error(source, 2383));
}

#[test]
fn ts2383_declare_global_mixed_export_still_errors() {
    let source = r#"
declare global {
    function make(): void;
    export function make(): void;
}
export {};
"#;
    assert!(has_error(source, 2383));
}

#[test]
fn ts2383_bare_top_level_ambient_overloads_mixed_export_still_errors() {
    let source = r#"
declare function make(): void;
export declare function make(): void;
"#;
    assert!(has_error(source, 2383));
}

#[test]
fn ts2383_ambient_namespace_mixed_export_generic_overloads_no_error() {
    let source = r#"
declare namespace Registry {
    function get<T>(key: string): T;
    export function get<T>(key: string, fallback: T): T;
}
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

// TS2383: every bodyless overload signature's export status is compared
// against the canonical declaration — the implementation, when it shares a
// container with the first overload, otherwise the first overload itself
// (tsc's `getCanonicalOverload`). This holds even for a single bodyless
// signature: the implementation is not itself flagged, but it can still be
// the canonical whose export status the lone signature must match. Verified
// against pinned `typescript@7.0.2` (`--noEmit --strict false --module
// commonjs --target es2015`).

#[test]
fn ts2383_single_overload_exported_implementation_not_exported_error() {
    let source = r#"
export function compute(x: number): number;
function compute(x: any): number { return x; }
"#;
    assert!(has_error(source, 2383));
}

#[test]
fn ts2383_single_overload_not_exported_implementation_exported_error() {
    let source = r#"
function transform(x: number): number;
export function transform(x: any): number { return x; }
"#;
    assert!(has_error(source, 2383));
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
fn ts2383_single_overload_impl_export_mismatch_renamed_binder_error() {
    let source = r#"
export function handle(x: string): string;
function handle(x: any): string { return x; }
"#;
    assert!(has_error(source, 2383));
}

#[test]
fn ts2383_single_overload_reversed_impl_export_mismatch_error() {
    let source = r#"
function serialize(x: number): string;
export function serialize(x: any): string { return String(x); }
"#;
    assert!(has_error(source, 2383));
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
fn ts2383_two_exported_overloads_non_exported_impl_error() {
    // The implementation shares the source file container with the first
    // overload, so it is canonical; both agreeing-with-each-other overloads
    // still deviate from it and are each flagged.
    let source = r#"
export function compute(x: number): number;
export function compute(x: string): number;
function compute(x: any): number { return 0; }
"#;
    assert!(has_error(source, 2383));
}

#[test]
fn ts2383_two_non_exported_overloads_exported_impl_error() {
    let source = r#"
function transform(x: number): string;
function transform(x: string): string;
export function transform(x: any): string { return ""; }
"#;
    assert!(has_error(source, 2383));
}

#[test]
fn ts2383_single_overload_generic_export_mismatch_error() {
    let source = r#"
export function identity<T>(x: T): T;
function identity<T>(x: T): T { return x; }
"#;
    assert!(has_error(source, 2383));
}

#[test]
fn ts2383_non_ambient_namespace_single_overload_export_mismatch_error() {
    let source = r#"
namespace Widgets {
    export function make(): void;
    function make(): void {}
}
"#;
    assert!(has_error(source, 2383));
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

// TS2394: generic class method overloads with class-level type parameters
// These are false positives in tsz (tsc accepts them). See issue #10670.

#[test]
fn ts2394_generic_class_method_overload_keyof_no_false_positive() {
    let source = r#"
class Builder<T> {
    get<K extends keyof T>(key: K): T[K];
    get(key: any): any { return (this as any)[key]; }
}
"#;
    assert!(!has_error(source, 2394));
}

#[test]
fn ts2394_generic_class_method_overload_varied_names_no_false_positive() {
    // Varied type param names — proves the fix is not name-specific
    let source = r#"
class Store<X> {
    fetch<Q extends keyof X>(prop: Q): X[Q];
    fetch(prop: any): any { return (this as any)[prop]; }
}
"#;
    assert!(!has_error(source, 2394));
}

#[test]
fn ts2394_generic_class_method_overload_record_return_no_false_positive() {
    let source = r#"
class Inserter<O> {
    returning<SE extends keyof O & string>(col: SE): Inserter<O & Record<SE, O[SE]>>;
    returning(col: any): Inserter<any> { return this as any; }
}
"#;
    assert!(!has_error(source, 2394));
}

#[test]
fn ts2394_generic_class_method_overload_multi_type_param_no_false_positive() {
    let source = r#"
class QueryBuilder<DB, TB extends keyof DB, O> {
    returning<SE extends keyof O & string>(columns: SE[]): QueryBuilder<DB, TB, O & Record<SE, O[SE & keyof O]>>;
    returning(columns: any): QueryBuilder<DB, TB, any> { return this as any; }
}
"#;
    assert!(!has_error(source, 2394));
}

#[test]
fn ts2394_generic_class_method_overload_pick_return_no_false_positive() {
    let source = r#"
class Container<T> {
    select<K extends keyof T & string>(keys: K[]): Container<Pick<T, K>>;
    select(keys: any): Container<any> { return this as any; }
}
"#;
    assert!(!has_error(source, 2394));
}

// TS2394: Multiple overloads (not just one overload + impl)

#[test]
fn ts2394_multiple_generic_class_overloads_no_false_positive() {
    let source = r#"
class QueryBuilder<DB, TB extends keyof DB, O> {
    returning<SE extends keyof O & string>(col: SE): QueryBuilder<DB, TB, O & Pick<O, SE>>;
    returning<SE extends keyof O & string>(cols: SE[]): QueryBuilder<DB, TB, O & Pick<O, SE>>;
    returning(selection: any): QueryBuilder<DB, TB, any> { return this as any; }
}
"#;
    assert!(!has_error(source, 2394));
}

#[test]
fn ts2394_generic_method_with_conditional_return_no_false_positive() {
    let source = r#"
type ExtractRow<O, SE> = SE extends keyof O ? Pick<O, SE & keyof O> : never;

class Builder<O> {
    select<SE extends keyof O & string>(col: SE): Builder<ExtractRow<O, SE>>;
    select<SE extends keyof O & string>(cols: SE[]): Builder<ExtractRow<O, SE>>;
    select(cols: any): Builder<any> { return this as any; }
}
"#;
    assert!(!has_error(source, 2394));
}

#[test]
fn ts2394_generic_method_overload_with_union_param_no_false_positive() {
    let source = r#"
class Container<T> {
    select<K extends keyof T & string>(key: K | K[]): Container<Pick<T, K>>;
    select(key: any): Container<any> { return this as any; }
}
"#;
    assert!(!has_error(source, 2394));
}

// TS2394: self-referential class builder pattern (kysely-like)
// The return type of the overload contains the class itself.

#[test]
fn ts2394_self_referential_builder_overload_no_false_positive() {
    let source = r#"
class QueryBuilder<DB, TB extends keyof DB, O> {
    returning<SE extends keyof O & string>(
        col: SE
    ): QueryBuilder<DB, TB, O & Record<SE, O[SE & keyof O]>>;
    returning(col: any): QueryBuilder<DB, TB, any> { return this as any; }
}
"#;
    assert!(!has_error(source, 2394));
}

#[test]
fn ts2394_self_referential_builder_multiple_overloads_no_false_positive() {
    let source = r#"
class InsertBuilder<DB, TB extends keyof DB, O> {
    returning<SE extends string & keyof O>(col: SE): InsertBuilder<DB, TB, O & Pick<O, SE>>;
    returning<SE extends string & keyof O>(cols: ReadonlyArray<SE>): InsertBuilder<DB, TB, O & Pick<O, SE>>;
    returning(col: any): InsertBuilder<DB, TB, any> { return this as any; }
}
"#;
    assert!(!has_error(source, 2394));
}

#[test]
fn ts2394_method_returning_any_impl_with_self_ref_overload_no_false_positive() {
    // Implementation returns any, overload returns self-referential complex type
    let source = r#"
type SelectAll = '*';
type RowType<T, K extends keyof T> = Pick<T, K>;

class Builder<T> {
    select<K extends keyof T & string>(col: K): Builder<RowType<T, K>>;
    select<K extends keyof T & string>(cols: K[]): Builder<RowType<T, K>>;
    select(col: SelectAll): Builder<T>;
    select(col: any): Builder<any> { return this as any; }
}
"#;
    assert!(!has_error(source, 2394));
}

// TS2394: same-name shadowing — class-level `T` shadowed by method-level `T`.
// The class binding must not leak into the method's own type param scope.

#[test]
fn ts2394_method_type_param_shadows_class_type_param_same_name_no_false_positive() {
    // Class has `T`; overload method declares its own `T extends keyof U`.
    // tsc accepts: method `T` shadows class `T` — no TS2394.
    let source = r#"
class Container<T, U> {
    get<T extends keyof U>(key: T): U[T];
    get(key: any): any { return (this as any)[key]; }
}
"#;
    assert!(!has_error(source, 2394));
}

#[test]
fn ts2394_method_type_param_same_name_as_class_varied_names_no_false_positive() {
    // Varied class/method type param names to prove the fix is not spelling-specific.
    let source = r#"
class Store<X, Y> {
    fetch<X extends keyof Y>(prop: X): Y[X];
    fetch(prop: any): any { return (this as any)[prop]; }
}
"#;
    assert!(!has_error(source, 2394));
}

#[test]
fn overload_assignment_missing_literal_overload_still_ts2322() {
    let source = r#"
function f(x: "foo"): number;
function f(x: string): number;
function f(x: string): number {
    return 0;
}

function g(x: "foo"): number;
function g(x: string): number {
    return 0;
}

let a = f;
let b = g;

a = b;
b = a;
"#;
    assert!(has_error(source, 2322));
}

// TS2394: error must be anchored at the incompatible overload, not the implementation.
//
// tsc always places TS2394 on the overload signature's name, never on the implementation.
// tsz had a fallback that would anchor the error at impl_node_idx when the cross-file
// span for an overload could not be determined; that fallback is now suppressed.
//
// Structural rule: when `cross_file_span` is `None` in the overload compatibility path
// (overload_compatibility.rs, the `else { ... }` branch at lines ~741-754), the
// diagnostic is suppressed entirely rather than misanchored at the implementation.
//
// NOTE on path reachability: the direct `cross_file_span = None` suppression path
// requires declaration-arena injection, so it is covered in the overload compatibility
// module tests. The tests below verify the public observable invariant.
//
// The tests below verify the observable invariant that covers BOTH paths: TS2394 is never
// anchored at or after the implementation's start position, regardless of how many
// incompatible overloads exist or what parameter shapes they use.

#[test]
fn ts2394_error_anchors_at_overload_name_not_implementation() {
    // "function f(x: string): void;" — overload name "f" is at byte 9
    // "function f(x: number): void {}" — impl name "f" is at byte 39 (after newline at 29)
    let source = "function f(x: string): void;\nfunction f(x: number): void {}\n";
    let diags = check_source_diagnostics(source);
    let errors: Vec<_> = diags.iter().filter(|d| d.code == 2394).collect();
    assert!(
        !errors.is_empty(),
        "Expected TS2394 for incompatible overload/impl pair"
    );
    for e in &errors {
        assert!(
            e.start < 30,
            "TS2394 at start={}: should be anchored at the overload (line 1, before byte 30), not the implementation",
            e.start
        );
    }
}

#[test]
fn ts2394_error_anchors_at_overload_varied_param_names() {
    // Varied param names prove the fix is not tied to a specific spelling.
    // "function process(input: string): void;" — overload name at byte 9, line ends at byte 38
    // "function process(input: number): void {}" — impl starts at byte 39
    let source =
        "function process(input: string): void;\nfunction process(input: number): void {}\n";
    let diags = check_source_diagnostics(source);
    let errors: Vec<_> = diags.iter().filter(|d| d.code == 2394).collect();
    assert!(
        !errors.is_empty(),
        "Expected TS2394 for incompatible overload/impl pair"
    );
    for e in &errors {
        assert!(
            e.start < 39,
            "TS2394 at start={}: should be anchored at the overload (line 1, before byte 39), not the implementation",
            e.start
        );
    }
}

#[test]
fn ts2394_method_overload_error_anchors_at_overload_not_impl() {
    // Class method overload: same invariant — error at the overload name, not the impl.
    let source = r#"class C {
    m(x: string): void;
    m(x: number): void {}
}"#;
    // "m" overload is on line 2; impl is on line 3. Overload "m" position < impl "m" position.
    let diags = check_source_diagnostics(source);
    let errors: Vec<_> = diags.iter().filter(|d| d.code == 2394).collect();
    assert!(
        !errors.is_empty(),
        "Expected TS2394 for incompatible class method overload/impl pair"
    );
    // Overload "m" is at "    m(x: string): void;\n" — starts around byte 14.
    // Impl "m" is after that. Just verify the error isn't past the overload's line.
    // Source: "class C {\n    m(x: string): void;\n    m(x: number): void {}\n}"
    //   Line 1 ends at: 10 + 24 = 34 chars (with newline).
    //   Line 2 starts at: 35. Impl "m" is near byte 39.
    // Check that error is on the overload line (< 35).
    for e in &errors {
        assert!(
            e.start < 35,
            "TS2394 at start={}: should be anchored at the class method overload, not the impl",
            e.start
        );
    }
}

// This test exercises the suppression invariant across multiple overload shapes to prove
// that TS2394 is never anchored at or after the implementation's start position.
//
// Structural rule covered: "when tsc emits TS2394 it anchors at the overload name; tsz
// must do the same and must never fall back to anchoring at the implementation."
//
// Note: the direct `cross_file_span = None` suppression branch is covered in the
// overload compatibility module tests. This integration test covers the public invariant:
// TS2394 must not appear at the implementation position.
#[test]
fn ts2394_impl_position_never_anchored_across_overload_shapes() {
    // Each tuple: (source, impl_name_start_byte).
    // The impl start byte is the position of the implementation function name.
    let cases: &[(&str, u32)] = &[
        // Function overloads — param types differ
        (
            "function f(x: string): void;\nfunction f(x: number): void {}\n",
            39, // second "f"
        ),
        // Different function name to prove fix isn't spelled-name-specific
        (
            "function transform(x: boolean): void;\nfunction transform(x: number): void {}\n",
            48, // second "transform"
        ),
        // Multiple overloads, only one incompatible
        (
            "function h(x: string): void;\nfunction h(x: string | number): void;\nfunction h(x: boolean): void {}\n",
            68, // third "h"
        ),
    ];

    for (source, impl_start) in cases {
        let diags = check_source_diagnostics(source);
        let at_impl: Vec<_> = diags
            .iter()
            .filter(|d| d.code == 2394 && d.start >= *impl_start)
            .collect();
        assert!(
            at_impl.is_empty(),
            "source={source:?}: TS2394 must not be anchored at or after the implementation \
             (impl_start={impl_start}), but got: {at_impl:?}"
        );
    }
}
