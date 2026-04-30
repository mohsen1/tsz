use super::super::core::*;

#[test]
fn test_indexed_access_constrained_type_param_no_ts2536() {
    let diagnostics = compile_and_get_diagnostics(
        r"
type PropertyType<T extends object, K extends keyof T> = T[K];
        ",
    );

    assert!(
        !has_error(&diagnostics, 2536),
        "Should not emit TS2536 when index type parameter is constrained by keyof.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_indexed_access_constrained_type_param_no_false_ts2304() {
    let diagnostics = compile_and_get_diagnostics(
        r"
type PropertyType<T extends object, K extends keyof T> = T[K];
        ",
    );

    assert!(
        !has_error(&diagnostics, 2304),
        "Should not emit TS2304 for in-scope type parameters in indexed access.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_indexed_access_unconstrained_type_param_emits_ts2536() {
    let diagnostics = compile_and_get_diagnostics_with_lib(
        r"
type BadPropertyType<T extends object, K> = T[K];
        ",
    );

    assert!(
        has_error(&diagnostics, 2536),
        "Should emit TS2536 when type parameter is unconstrained for indexed access.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_indexed_access_array_element_through_constrained_union_no_ts2536() {
    let diagnostics = compile_and_get_diagnostics(
        r"
type Node =
    | { name: 'a'; children: Node[] }
    | { name: 'b'; children: Node[] };

type ChildrenOf<T extends Node> = T['children'][number];
        ",
    );

    assert!(
        !has_error(&diagnostics, 2536),
        "Should not emit TS2536 for element access through constrained array property.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_indexed_access_scalar_property_then_number_index_emits_ts2536() {
    let diagnostics = compile_and_get_diagnostics_with_lib(
        r"
type Boxed = { value: number };
type Bad<T extends Boxed> = T['value'][number];
        ",
    );

    assert!(
        has_error(&diagnostics, 2536),
        "Should emit TS2536 when indexing a constrained scalar property with number.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_indexed_access_type_param_in_mapped_intersection_no_ts2536() {
    // Repro from conditionalTypes1.ts (#21862): type param T indexes an intersection
    // whose keyof includes T itself (from mapped types).
    let diagnostics = compile_and_get_diagnostics(
        r"
type OldDiff<T extends keyof any, U extends keyof any> = (
    & { [P in T]: P; }
    & { [P in U]: never; }
    & { [x: string]: never; }
)[T];
        ",
    );

    assert!(
        !has_error(&diagnostics, 2536),
        "Should not emit TS2536 when type param T indexes an intersection containing mapped type over T.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_mapped_type_direct_circular_constraint_reports_ts2313() {
    let diagnostics = compile_and_get_diagnostics(
        r"
type T00 = { [P in P]: string };
",
    );

    assert!(
        has_error(&diagnostics, 2313),
        "Expected TS2313 for direct mapped type parameter self reference.\nActual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 2304),
        "Should not emit TS2304 for self-reference constraint.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_self_indexed_property_annotations_emit_ts2502() {
    let diagnostics = compile_and_get_diagnostics_with_lib(
        r#"
type T1 = {
    x: T1["x"];
};

interface I1 {
    x: I1["x"];
}

class C1 {
    x: C1["x"];
}
"#,
    );

    let ts2502_count = diagnostics.iter().filter(|d| d.0 == 2502).count();
    assert_eq!(
        ts2502_count, 3,
        "Expected TS2502 for self-indexed type literal, interface, and class properties.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_mapped_type_invalid_key_constraint_emits_ts2536() {
    let diagnostics = compile_and_get_diagnostics(
        r"
type Foo2<T, F extends keyof T> = {
    pf: { [P in F]?: T[P] },
    pt: { [P in T]?: T[P] },
};

type O = { x: number; y: boolean; };
let o: O = { x: 5, y: false };
    let f: Foo2<O, 'x'> = {
        pf: { x: 7 },
        pt: { x: 7, y: false },
    };
        ",
    );

    assert!(
        has_error(&diagnostics, 2536),
        "Expected TS2536 for `T[P]` when mapped key is constrained as `P in T`.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_mapped_type_key_index_access_constraint_emits_ts2536() {
    let diagnostics = compile_and_get_diagnostics(
        r"
type AB = { a: 'a'; b: 'a' };
type T1<K extends keyof AB> = { [key in AB[K]]: true };
type T2<K extends 'a'|'b'> = T1<K>[K];
        ",
    );

    assert!(
        has_error(&diagnostics, 2536),
        "Expected TS2536 for indexing mapped result with unconstrained key subset (`AB[K]` values).\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_element_access_mismatched_keyof_source_emits_ts2536() {
    let diagnostics = compile_and_get_diagnostics(
        r"
function f<T, U extends T>(x: T, y: U, k: keyof U) {
    x[k] = y[k];
    y[k] = x[k];
}

function g<T, U extends T, K extends keyof U>(x: T, y: U, k: K) {
    x[k] = y[k];
    y[k] = x[k];
}
        ",
    );

    let ts2536_count = diagnostics.iter().filter(|(code, _)| *code == 2536).count();
    assert!(
        ts2536_count >= 4,
        "Expected TS2536 for mismatched generic key source in element access.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_element_access_union_receiver_with_noncommon_generic_keys_emits_ts2536() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
function f<T, U>(
    x: T | U,
    k1: keyof (T | U),
    k2: keyof T & keyof U,
    k3: keyof (T & U),
    k4: keyof T | keyof U,
) {
    x[k1];
    x[k2];
    x[k3];
    x[k4];
}
        "#,
    );

    let ts2536_count = diagnostics.iter().filter(|(code, _)| *code == 2536).count();
    assert!(
        ts2536_count >= 2,
        "Expected TS2536 for indexing a union receiver with non-common generic key spaces.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_record_constraint_checked_with_lib_param_prewarm_filtering() {
    if !lib_files_available() {
        return;
    }
    let diagnostics =
        compile_and_get_diagnostics_with_lib(r#"type ValidRecord = Record<string, number>;"#);
    assert!(
        diagnostics.is_empty(),
        "Expected no diagnostics for valid Record<K, V> usage.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_primitive_property_access_works_with_conditional_boxed_registration() {
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_and_get_diagnostics_with_lib(
        r#"
const upper = "hello".toUpperCase();
        "#,
    );
    assert!(
        diagnostics.is_empty(),
        "Expected no diagnostics for primitive string property access.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_global_array_augmentation_uses_lib_resolution_without_diagnostics() {
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        r#"
export {};

declare global {
    interface Array<T> {
        firstOrUndefined(): T | undefined;
    }
}

const xs = [1, 2, 3];
const first = xs.firstOrUndefined();
"#,
        CheckerOptions {
            module: tsz_common::common::ModuleKind::CommonJS,
            ..CheckerOptions::default()
        },
    );
    assert!(
        diagnostics.is_empty(),
        "Expected no diagnostics for Array global augmentation merged with lib declarations.\nActual diagnostics: {diagnostics:#?}"
    );
}

/// Issue: Flow analysis applies narrowing from invalid assignments
///
/// From: derivedClassTransitivity3.ts
/// Expected: TS2322 only (assignment incompatibility)
/// Actual: TS2322 + TS2345 (also reports wrong parameter type on subsequent call)
///
/// Root cause: Flow analyzer treats invalid assignment as if it succeeded,
/// narrowing the variable type to the assigned type.
///
/// Complexity: HIGH - requires binder/checker coordination
/// See: docs/conformance-work-session-summary.md
#[test]
fn test_flow_narrowing_from_invalid_assignment() {
    let diagnostics: Vec<_> = compile_and_get_diagnostics(
        r"
class C<T> {
    foo(x: T, y: T) { }
}

class D<T> extends C<T> {
    foo(x: T) { } // ok to drop parameters
}

class E<T> extends D<T> {
    foo(x: T, y?: number) { } // ok to add optional parameters
}

declare var c: C<string>;
declare var e: E<string>;
c = e;                      // Should error: TS2322
var r = c.foo('', '');      // Should NOT error (c is still C<string>)
        ",
    )
    .into_iter()
    .filter(|(code, _)| *code != 2318)
    .collect();

    // Should have TS2322 on the assignment
    assert!(
        has_error(&diagnostics, 2322),
        "Should emit TS2322 for assignment incompatibility"
    );
    // Flow narrowing no longer narrows c's type through the invalid assignment.
    assert!(
        !has_error(&diagnostics, 2345),
        "Should NOT emit false TS2345 after invalid assignment\nActual errors: {diagnostics:#?}"
    );
}

/// Issue: Parser emitting cascading error after syntax error
///
/// From: classWithPredefinedTypesAsNames2.ts
/// Expected: TS1005 only
/// Status: FIXED (2026-02-09)
///
/// Root cause: Parser didn't consume the invalid token after emitting error
/// Fix: Added `next_token()` call in `state_statements.rs` after reserved word error
#[test]
fn test_parser_cascading_error_suppression() {
    let source = r"
// classes cannot use predefined types as names
class void {}
        ";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let parser_diagnostics: Vec<(u32, String)> = parser
        .get_diagnostics()
        .iter()
        .map(|d| (d.code, d.message.clone()))
        .collect();

    // Should only emit TS1005 '{' expected
    let ts1005_count = parser_diagnostics
        .iter()
        .filter(|(c, _)| *c == 1005)
        .count();

    assert!(
        has_error(&parser_diagnostics, 1005),
        "Should emit TS1005 for syntax error.\nActual errors: {parser_diagnostics:#?}"
    );
    assert_eq!(
        ts1005_count, 1,
        "Should only emit one TS1005, got {ts1005_count}"
    );
    assert!(
        !has_error(&parser_diagnostics, 1068),
        "Should NOT emit cascading TS1068 error.\nActual errors: {parser_diagnostics:#?}"
    );
}

#[test]
fn test_method_implementation_name_formatting_probe() {
    let diagnostics = compile_and_get_diagnostics(
        r#"class C {
"foo"();
"bar"() { }
}"#,
    );
    println!("ClassDeclaration22 diag: {diagnostics:?}");

    let mut parser = ParserState::new(
        "test.ts".to_string(),
        r#"class C {
"foo"();
"bar"() { }
}"#
        .to_string(),
    );
    let root = parser.parse_source_file();
    let source_file = parser.get_arena().get_source_file_at(root).unwrap();
    if let Some(first_stmt) = source_file.statements.nodes.first() {
        let class_node = parser.get_arena().get(*first_stmt).unwrap();
        let class_data = parser.get_arena().get_class(class_node).unwrap();
        for member_idx in &class_data.members.nodes {
            let member_node = parser.get_arena().get(*member_idx).unwrap();
            let kind = member_node.kind;
            if let Some(method) = parser.get_arena().get_method_decl(member_node) {
                let name_node = parser.get_arena().get(method.name).unwrap();
                let text = parser
                    .get_arena()
                    .get_literal(name_node)
                    .map(|lit| lit.text.clone())
                    .unwrap_or_else(|| "<non-literal>".to_string());
                println!(
                    "member kind={kind} method body={body:?} name={name_node:?} text={text}",
                    body = method.body,
                    name_node = method.name
                );
            }
        }
    }

    let diagnostics = compile_and_get_diagnostics(
        r#"class C {
["foo"](): void
["bar"](): void;
["foo"]() {
    return 0;
}
}"#,
    );
    println!("Overload computed diag: {diagnostics:?}");
}

/// Issue: Interface with reserved word name
///
/// Expected: TS1005 only (no cascading errors)
/// Status: FIXED (2026-02-09)
///
/// Root cause: Parser must consume invalid reserved-word names to avoid cascades.
/// Fix: Reserved-word interface names emit TS1005 and recover.
#[test]
fn test_interface_reserved_word_error_suppression() {
    let source = r"
interface class {}
    ";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let parser_diagnostics: Vec<(u32, String)> = parser
        .get_diagnostics()
        .iter()
        .map(|d| (d.code, d.message.clone()))
        .collect();

    // Should only emit TS1005 '{' expected
    let ts1005_count = parser_diagnostics
        .iter()
        .filter(|(c, _)| *c == 1005)
        .count();

    assert!(
        has_error(&parser_diagnostics, 1005),
        "Should emit TS1005 for syntax error.\nActual errors: {parser_diagnostics:#?}"
    );
    assert_eq!(
        ts1005_count, 1,
        "Should only emit one TS1005, got {ts1005_count}"
    );
    // Check for common cascading errors
    assert!(
        !has_error(&parser_diagnostics, 1068),
        "Should NOT emit cascading TS1068 error.\nActual errors: {parser_diagnostics:#?}"
    );
}

#[test]
fn test_class_extends_primitive_reports_ts2863() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class C extends number {}
        ",
    );

    assert!(
        has_error(&diagnostics, 2863),
        "Expected TS2863 when class extends primitive type. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_class_implements_primitive_reports_ts2864() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class C implements number {}
        ",
    );

    assert!(
        has_error(&diagnostics, 2864),
        "Expected TS2864 when class implements primitive type. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_indirect_class_cycle_reports_all_ts2506_errors() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class C extends E { foo: string; }
class D extends C { bar: string; }
class E extends D { baz: number; }

class C2<T> extends E2<T> { foo: T; }
class D2<T> extends C2<T> { bar: T; }
class E2<T> extends D2<T> { baz: T; }
        ",
    );

    let ts2506_count = diagnostics.iter().filter(|(code, _)| *code == 2506).count();
    assert_eq!(
        ts2506_count, 6,
        "Expected TS2506 on all six classes in the two cycles. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_class_extends_export_default_base_resolves_instance_members() {
    let diagnostics = compile_and_get_diagnostics(
        r"
export default class Base {
    value: number = 1;
}

class Derived extends Base {
    read(): number {
        return this.value;
    }
}
        ",
    );

    let unexpected: Vec<(u32, String)> = diagnostics
        .into_iter()
        .filter(|(code, _)| matches!(*code, 2339 | 2506 | 2449))
        .collect();

    assert!(
        unexpected.is_empty(),
        "Expected extends/default-base instance resolution without TS2339/TS2506/TS2449. Actual diagnostics: {unexpected:#?}"
    );
}

#[test]
fn test_class_interface_merge_preserves_callable_and_properties() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class Merged {
    value: number = 1;
}

interface Merged {
    (x: number): string;
    extra: boolean;
}

declare const merged: Merged;
const okCall: string = merged(1);
const okProp: boolean = merged.extra;
const badCall: number = merged(1);
        ",
    );

    assert!(
        has_error(&diagnostics, 2322),
        "Expected TS2322 for assigning merged callable string result to number.\nActual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 2349),
        "Did not expect TS2349; merged class/interface type should remain callable.\nActual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 2339),
        "Did not expect TS2339; merged interface property should remain visible.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_generic_multi_level_extends_resolves_base_instance_member_without_cycle_noise() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class Box<T> {
    value!: T;
}

class Mid<U> extends Box<U> {}

class Final extends Mid<string> {
    read(): string {
        return this.value;
    }
}

const ok: string = new Final().value;
const bad: number = new Final().value;
        ",
    );

    assert!(
        has_error(&diagnostics, 2322),
        "Expected TS2322 for assigning inherited string member to number.\nActual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 2339),
        "Did not expect TS2339 for inherited base member lookup.\nActual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 2506),
        "Did not expect TS2506 in non-cyclic generic inheritance.\nActual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 2449),
        "Did not expect TS2449 for this linear declaration order.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_class_used_before_declaration_does_not_also_report_cycle_error() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class A extends B {}
class B extends C {}
class C {}
        ",
    );

    let has_ts2449 = diagnostics.iter().any(|(code, _)| *code == 2449);
    let has_ts2506 = diagnostics.iter().any(|(code, _)| *code == 2506);

    assert!(
        has_ts2449,
        "Expected TS2449 for class used before declaration. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !has_ts2506,
        "Did not expect TS2506 for non-cyclic before-declaration extends. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_new_expression_class_used_before_declaration() {
    // `new C()` before `class C` must emit TS2449.
    // The fast path for `new` expressions with identifier targets was
    // previously bypassing the TDZ check in get_type_of_identifier.
    let diagnostics = compile_and_get_diagnostics(
        r"
let a = new C();
class C { id: string = ''; }
        ",
    );
    assert!(
        has_error(&diagnostics, 2449),
        "Expected TS2449 for `new C()` before class declaration. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_new_expression_class_after_declaration_no_tdz() {
    // `new C()` after `class C` must NOT emit TS2449.
    let diagnostics = compile_and_get_diagnostics(
        r"
class C { id: string = ''; }
let a = new C();
        ",
    );
    assert!(
        !has_error(&diagnostics, 2449),
        "Did not expect TS2449 for `new C()` after class declaration. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_new_expression_merged_namespace_class_tdz() {
    // `new A()` inside a namespace body that merges with a class declared
    // after the namespace must emit TS2449.
    let diagnostics = compile_and_get_diagnostics(
        r"
namespace A {
    export var Instance = new A();
}
class A { id: string = ''; }
        ",
    );
    assert!(
        has_error(&diagnostics, 2449),
        "Expected TS2449 for `new A()` inside namespace before class declaration. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_duplicate_extends_clause_does_not_create_false_base_cycle() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class C extends A implements B extends C {
}
        ",
    );

    assert!(
        !has_error(&diagnostics, 2506),
        "Did not expect TS2506 from recovery-only duplicate extends clause. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_static_block_break_continue_cannot_target_outer_labels() {
    let diagnostics = compile_and_get_diagnostics(
        r"
function foo(v: number) {
    label: while (v) {
        class C {
            static {
                break label;
            }
        }
    }
}
        ",
    );

    assert!(
        has_error(&diagnostics, 1107),
        "Expected TS1107 for jump from static block to outer label. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_static_block_assignment_target_before_declaration_emits_ts2448() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class C {
    static {
        getY = () => 1;
    }
}

let getY: () => number;
        ",
    );

    assert!(
        has_error(&diagnostics, 2448),
        "Expected TS2448 for assignment target before declaration in static block. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_return_in_static_block_emits_ts18041_even_with_other_grammar_errors() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class C {
    static {
        await 1;
        return 1;
    }
}
        ",
    );

    assert!(
        has_error(&diagnostics, 18041),
        "Expected TS18041 for return inside class static block. Actual diagnostics: {diagnostics:#?}"
    );
}

/// Forward-reference class relationships should not trigger TS2506.
/// Derived extends Base, where Base is declared after Derived.
/// The `class_instance_resolution_set` recursion guard should not be
/// confused with a real circular inheritance cycle.
#[test]
fn test_complex_class_relationships_no_ts2506() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class Derived extends Base {
    public static createEmpty(): Derived {
        var item = new Derived();
        return item;
    }
}
class Base {
    ownerCollection: any;
}
        ",
    );
    assert!(
        !has_error(&diagnostics, 2506),
        "Did not expect TS2506 for forward-reference class extends. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_circular_base_type_alias_instantiation_reports_ts2310_and_ts2313() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
type M<T> = { value: T };
interface M2 extends M<M3> {}
type M3 = M2[keyof M2];

type X<T> = { [K in keyof T]: string } & { b: string };
interface Y extends X<Y> {
    a: "";
}
        "#,
    );

    assert!(
        has_error(&diagnostics, 2310),
        "Expected TS2310 for recursive base type instantiation. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        has_error(&diagnostics, 2313),
        "Expected TS2313 for mapped type constraint cycle through instantiated base alias. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_class_base_default_type_arg_cycle_reports_ts2310_without_ts2506() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class BaseType<T> {
    bar: T
}

class NextType<C extends { someProp: any }, T = C['someProp']> extends BaseType<T> {
    baz: string;
}

class Foo extends NextType<Foo> {
    someProp: {
        test: true
    }
}
        ",
    );

    assert!(
        has_error(&diagnostics, 2310),
        "Expected TS2310 for recursive instantiated class base type. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 2506),
        "Did not expect TS2506 for instantiated-base recursion. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_interface_extends_readonly_array_through_conditional_alias_has_no_ts2310() {
    let diagnostics = compile_and_get_diagnostics(
        r"
type Primitive = string | number | boolean | bigint | symbol | null | undefined;

type DeepReadonly<T> = T extends ((...args: any[]) => any) | Primitive
  ? T
  : T extends _DeepReadonlyArray<infer U>
  ? _DeepReadonlyArray<U>
  : T extends _DeepReadonlyObject<infer V>
  ? _DeepReadonlyObject<V>
  : T;

interface _DeepReadonlyArray<T> extends ReadonlyArray<DeepReadonly<T>> {}

type _DeepReadonlyObject<T> = {
  readonly [P in keyof T]: DeepReadonly<T[P]>;
};
        ",
    );

    assert!(
        !has_error(&diagnostics, 2310),
        "ReadonlyArray heritage should not report TS2310 through conditional element aliases. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_homomorphic_mapped_type_union_constraint_with_readonly_member() {
    let options = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        no_implicit_any: true,
        ..Default::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
type HomomorphicMappedType<T> = { [P in keyof T]: T[P] extends string ? boolean : null }

function test1<T extends [number] | [string]>(args: T) {
  const arr: any[] = [] as HomomorphicMappedType<T>
  const arr2: readonly any[] = [] as HomomorphicMappedType<T>
}

function test2<T extends [number] | readonly [string]>(args: T) {
  const arr: any[] = [] as HomomorphicMappedType<T>
  const arr2: readonly any[] = [] as HomomorphicMappedType<T>
}
",
        options,
    );
    assert_eq!(
        diagnostics.len(),
        1,
        "Expected exactly 1 diagnostic (test2 arr assignment to any[]), got: {diagnostics:#?}"
    );
    assert_eq!(
        diagnostics[0].0, 2322,
        "Expected TS2322, got: {diagnostics:#?}"
    );
}
