//! Tests for TS2428, TS2411, TS2413: interface declaration merging diagnostics.

use crate::CheckerState;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn get_diagnostics(source: &str) -> Vec<(u32, String)> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

fn has_error_with_code(source: &str, code: u32) -> bool {
    get_diagnostics(source).iter().any(|d| d.0 == code)
}

#[test]
fn generic_and_non_generic_interface_same_name_emits_ts2428() {
    let source = r#"
interface A {
    foo: string;
}
interface A<T> {
    bar: T;
}
"#;
    assert!(
        has_error_with_code(source, 2428),
        "Should emit TS2428 when generic and non-generic interfaces share a name"
    );
}

#[test]
fn same_interface_no_type_params_no_error() {
    let source = r#"
interface A {
    foo: string;
}
interface A {
    bar: number;
}
"#;
    assert!(
        !has_error_with_code(source, 2428),
        "Should NOT emit TS2428 when interfaces have identical (no) type params"
    );
}

#[test]
fn same_generic_interface_same_params_no_error() {
    let source = r#"
interface A<T> {
    foo: T;
}
interface A<T> {
    bar: T;
}
"#;
    assert!(
        !has_error_with_code(source, 2428),
        "Should NOT emit TS2428 when interfaces have identical type params"
    );
}

#[test]
fn different_arity_emits_ts2428() {
    let source = r#"
interface A<T> {
    x: T;
}
interface A<T, U> {
    y: T;
}
"#;
    assert!(
        has_error_with_code(source, 2428),
        "Should emit TS2428 when interface type parameter arity differs"
    );
}

#[test]
fn namespace_separate_blocks_emits_ts2428() {
    let source = r#"
namespace M3 {
    export interface A {
        foo: string;
    }
}

namespace M3 {
    export interface A<T> {
        bar: T;
    }
}
"#;
    assert!(
        has_error_with_code(source, 2428),
        "Should emit TS2428 for interfaces in separate namespace blocks with different type params"
    );
}

#[test]
fn namespace_same_block_emits_ts2428() {
    let source = r#"
namespace M {
    interface A<T> {
        bar: T;
    }
    interface A {
        foo: string;
    }
}
"#;
    assert!(
        has_error_with_code(source, 2428),
        "Should emit TS2428 for interfaces in same namespace block with different type params"
    );
}

#[test]
fn any_constraint_not_identical_to_concrete_constraint_emits_ts2428() {
    // TSC uses isTypeIdenticalTo, not assignability, for constraint comparison.
    // A<any> is mutually assignable to A<Date> but NOT identical.
    let source = r#"
interface Foo {}
namespace M {
    interface B<T extends Foo> {
        x: T;
    }
    interface B<T extends any> {
        y: T;
    }
}
"#;
    assert!(
        has_error_with_code(source, 2428),
        "Should emit TS2428 when one constraint is `any` and the other is a concrete type"
    );
}

#[test]
fn same_constraint_no_ts2428() {
    let source = r#"
interface Foo {}
namespace M {
    interface B<T extends Foo> {
        x: T;
    }
    interface B<T extends Foo> {
        y: T;
    }
}
"#;
    assert!(
        !has_error_with_code(source, 2428),
        "Should NOT emit TS2428 when constraints are identical"
    );
}

#[test]
fn one_constraint_missing_no_ts2428() {
    // TSC: if one has a constraint and the other doesn't, they're compatible.
    let source = r#"
interface B<T extends number> {
    u: T;
}
interface B<T> {
    x: T;
}
"#;
    assert!(
        !has_error_with_code(source, 2428),
        "Should NOT emit TS2428 when one has constraint and the other doesn't"
    );
}

#[test]
fn class_interface_merge_different_arity_no_ts2428() {
    let source = r#"
interface Component<P = {}, S = {}, SS = any> {}
class Component<P, S> {}
"#;
    assert!(
        !has_error_with_code(source, 2428),
        "Should NOT emit TS2428 for merged class/interface declarations with different arity"
    );
}

#[test]
fn class_interface_merge_uses_interface_type_parameter_arity_in_type_position() {
    let source = r#"
interface Component<P = {}, S = {}, SS = any> {}
class Component<P, S> {}
type X = Component<string, number, boolean>;
"#;
    assert!(
        !has_error_with_code(source, 2707),
        "Should NOT emit TS2707 when merged class/interface type position uses interface arity"
    );
}

#[test]
fn ts2717_separate_namespace_blocks_exported() {
    // Exported interfaces in separate namespace blocks should have
    // property types compared across blocks (TS2717).
    let source = r#"
namespace M3 {
    export interface A<T> {
        x: T;
    }
}
namespace M3 {
    export interface A<T> {
        x: number;
    }
}
"#;
    assert!(
        has_error_with_code(source, 2717),
        "Should emit TS2717 for conflicting property types across separate namespace blocks"
    );
}

// ── TS2411 quoting tests ────────────────────────────────────────────────

fn get_diagnostic_messages(source: &str) -> Vec<(u32, String)> {
    get_diagnostics(source)
}

#[test]
fn ts2411_single_quoted_property_name_preserved_in_diagnostic() {
    // TSC preserves quote style: `'a': number` → Property ''a'' of type ...
    let source = r#"
interface A2 {
    [x: string]: { length: number };
    'a': number;
}
"#;
    let diags = get_diagnostic_messages(source);
    let ts2411_msgs: Vec<_> = diags.iter().filter(|d| d.0 == 2411).collect();
    assert!(
        !ts2411_msgs.is_empty(),
        "Should emit TS2411 for string literal property vs string index"
    );
    let msg = &ts2411_msgs[0].1;
    assert!(
        msg.contains("'a'"),
        "TS2411 message should include single-quoted property name 'a', got: {msg}"
    );
}

#[test]
fn ts2411_double_quoted_property_name_preserved_in_diagnostic() {
    // TSC preserves quote style: `"-Infinity": string` → Property '"-Infinity"' of type ...
    let source = r#"
interface A {
    [x: string]: number;
    "-Infinity": string;
}
"#;
    let diags = get_diagnostic_messages(source);
    let ts2411_msgs: Vec<_> = diags.iter().filter(|d| d.0 == 2411).collect();
    assert!(
        !ts2411_msgs.is_empty(),
        "Should emit TS2411 for double-quoted string literal property vs string index"
    );
    let msg = &ts2411_msgs[0].1;
    assert!(
        msg.contains("\"-Infinity\""),
        "TS2411 message should include double-quoted property name, got: {msg}"
    );
}

#[test]
fn ts2411_identifier_property_not_quoted() {
    // Identifier properties should NOT have quotes in TS2411 message
    let source = r#"
interface A {
    [x: string]: number;
    foo: string;
}
"#;
    let diags = get_diagnostic_messages(source);
    let ts2411_msgs: Vec<_> = diags.iter().filter(|d| d.0 == 2411).collect();
    assert!(
        !ts2411_msgs.is_empty(),
        "Should emit TS2411 for identifier property vs string index"
    );
    let msg = &ts2411_msgs[0].1;
    // The template wraps {0} in single quotes: Property 'foo' of type...
    // But the property name itself should NOT have extra quotes
    assert!(
        !msg.contains("'foo'") || !msg.contains("''foo''"),
        "TS2411 message for identifier should not double-quote, got: {msg}"
    );
}

// ── TS2413 location tests ───────────────────────────────────────────────

#[test]
fn ts2413_emitted_for_incompatible_number_and_string_index() {
    // TS2413: 'number' index type 'string' is not assignable to 'string' index type ...
    let source = r#"
interface A {
    [x: number]: string;
    [x: string]: { length: string };
}
"#;
    assert!(
        has_error_with_code(source, 2413),
        "Should emit TS2413 when number index type is not assignable to string index type"
    );
}

#[test]
fn ts2413_not_duplicated_across_merged_interface_bodies() {
    // When index signatures are in separate merged interface bodies,
    // TS2413 should only be emitted once (on the number index node).
    let source = r#"
interface A {
    [x: number]: string;
}
interface A {
    [x: string]: { length: string };
}
"#;
    let diags = get_diagnostic_messages(source);
    let ts2413_count = diags.iter().filter(|d| d.0 == 2413).count();
    assert!(
        ts2413_count <= 1,
        "TS2413 should not be duplicated across merged bodies, got {ts2413_count} emissions"
    );
    assert!(
        ts2413_count == 1,
        "Should still emit TS2413 once for incompatible indexes, got {ts2413_count} emissions"
    );
}

// ── TS2515 abstract member satisfaction via declaration merging ──────────

#[test]
fn ts2515_suppressed_when_merged_interface_satisfies_abstract_member() {
    // When a class merges with an interface that provides the abstract member,
    // TSC does NOT emit TS2515.
    let source = r#"
abstract class BaseClass {
    abstract bar: number;
}
class Broken extends BaseClass {}
interface IGetters {
    bar: number;
}
interface Broken extends IGetters {}
"#;
    assert!(
        !has_error_with_code(source, 2515),
        "Should NOT emit TS2515 when merged interface provides the abstract member"
    );
}

#[test]
fn ts2515_emitted_when_no_merged_interface_provides_abstract_member() {
    // Without declaration merging, TS2515 should fire.
    let source = r#"
abstract class BaseClass {
    abstract bar: number;
}
class Broken extends BaseClass {}
"#;
    assert!(
        has_error_with_code(source, 2515),
        "Should emit TS2515 when non-abstract class doesn't implement abstract member"
    );
}
