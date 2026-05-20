//! Tests for TS2428, TS2411, TS2413: interface declaration merging diagnostics.

use crate::test_utils::check_source_code_messages as get_diagnostics;

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
fn different_arity_with_defaults_no_ts2428() {
    let source = r#"
interface i04 {}
interface i04<T> {}
interface i04<T = number> {}
interface i04<T = number, U = string> {}
"#;
    assert!(
        !has_error_with_code(source, 2428),
        "Should NOT emit TS2428 when extra type parameters have defaults in the merge group"
    );
}

#[test]
fn different_arity_with_defaults_pairwise_no_ts2428() {
    let source = r#"
interface A {}
interface A<T = number> {}
"#;
    assert!(
        !has_error_with_code(source, 2428),
        "Should NOT emit TS2428 when extra type parameter has default"
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
fn self_referential_constraint_repeated_no_ts2428() {
    // `<T extends Foo<T>>` declared twice. Each declaration's `T` resolves
    // to a distinct underlying `TypeId` because the binder gives each
    // declaration its own type-parameter symbol; without canonical-
    // substitution merge compatibility would report TS2428 spuriously.
    let source = r#"
class Foo<T> {}
interface I<T extends Foo<T>> {}
interface I<T extends Foo<T>> {}
"#;
    assert!(
        !has_error_with_code(source, 2428),
        "Should NOT emit TS2428 for repeated `<T extends Foo<T>>` declarations"
    );
}

#[test]
fn self_referential_constraint_three_decls_no_ts2428() {
    // Group merge: three declarations with self-referential constraints
    // should be accepted, since each pair canonicalizes identically.
    let source = r#"
class Foo<T> {}
interface I<T extends Foo<T>> {}
interface I<T extends Foo<T>> {}
interface I<T extends Foo<T>> {}
"#;
    assert!(
        !has_error_with_code(source, 2428),
        "Should NOT emit TS2428 for three self-referential merge group declarations"
    );
}

#[test]
fn two_param_self_referential_constraint_no_ts2428() {
    // `<T, U extends T>` declared twice: U's constraint references position 0's
    // parameter. Both declarations should canonicalize to the same shape.
    let source = r#"
interface I<T, U extends T> {}
interface I<T, U extends T> {}
"#;
    assert!(
        !has_error_with_code(source, 2428),
        "Should NOT emit TS2428 when constraint references an earlier positional type parameter"
    );
}

#[test]
fn class_interface_merge_self_referential_constraint_no_ts2428() {
    // Mixed class+interface merge with self-referential constraint should
    // accept identical shapes despite distinct underlying TypeIds.
    let source = r#"
class Foo<T> {}
interface I<T extends Foo<T>> {}
class I<T extends Foo<T>> {}
"#;
    assert!(
        !has_error_with_code(source, 2428),
        "Should NOT emit TS2428 for class+interface merge with self-referential constraint"
    );
}

#[test]
fn renamed_self_referential_constraint_emits_ts2428() {
    // Names must still match positionally. Renaming the iteration variable
    // is a real divergence under tsc's interface-merge rule and must fire.
    let source = r#"
class Foo<T> {}
interface I<T extends Foo<T>> {}
interface I<S extends Foo<S>> {}
"#;
    assert!(
        has_error_with_code(source, 2428),
        "Should emit TS2428 when positional names differ across declarations"
    );
}

#[test]
fn divergent_self_referential_constraint_emits_ts2428() {
    // Constraints with different shapes must still fire TS2428 — the
    // canonicalization only rewrites self-references, not the constraint
    // structure itself.
    let source = r#"
class Foo<T> {}
class Bar<T> {}
interface I<T extends Foo<T>> {}
interface I<T extends Bar<T>> {}
"#;
    assert!(
        has_error_with_code(source, 2428),
        "Should emit TS2428 when constraint base differs (Foo vs Bar)"
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

#[test]
fn ts2411_single_quoted_property_name_preserved_in_diagnostic() {
    // TSC preserves quote style: `'a': number` → Property ''a'' of type ...
    let source = r#"
interface A2 {
    [x: string]: { length: number };
    'a': number;
}
"#;
    let diags = get_diagnostics(source);
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
    let diags = get_diagnostics(source);
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
    let diags = get_diagnostics(source);
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
    let diags = get_diagnostics(source);
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

#[test]
fn ts2374_expands_union_index_signature_keys_for_duplicate_checks() {
    let source = r#"
type Duplicates = {
    [key: string | number]: any;
    [key: number | symbol]: any;
    [key: symbol | `foo${string}`]: any;
    [key: `foo${string}`]: any;
}
"#;
    let diags = get_diagnostics(source);
    let duplicate_msgs: Vec<_> = diags
        .iter()
        .filter_map(|(code, message)| (*code == 2374).then_some(message.as_str()))
        .collect();

    assert_eq!(
        duplicate_msgs
            .iter()
            .filter(|message| message.contains("'number'"))
            .count(),
        2,
        "expected both number index signatures to be flagged, got {duplicate_msgs:#?}"
    );
    assert_eq!(
        duplicate_msgs
            .iter()
            .filter(|message| message.contains("'symbol'"))
            .count(),
        2,
        "expected both symbol index signatures to be flagged, got {duplicate_msgs:#?}"
    );
    assert_eq!(
        duplicate_msgs
            .iter()
            .filter(|message| message.contains("'`foo${string}`'"))
            .count(),
        2,
        "expected both template index signatures to be flagged, got {duplicate_msgs:#?}"
    );
}

#[test]
fn ts2374_reports_duplicate_generic_index_signature_key_components() {
    let source = r#"
type Invalid<T extends string> = {
    [key: T | number]: string;
    [key: T & string]: string;
}
"#;
    let diags = get_diagnostics(source);
    let duplicate_t_count = diags
        .iter()
        .filter(|(code, message)| *code == 2374 && message.contains("'T'"))
        .count();
    assert_eq!(
        duplicate_t_count, 2,
        "expected both generic-key index signatures to be flagged, got {diags:#?}"
    );
}

#[test]
fn ts2413_checks_template_index_signature_subpattern_values() {
    let source = r#"
type Conflicting = {
    [key: `a${string}`]: 'a';
    [key: `${string}a`]: 'b';
    [key: `a${string}a`]: 'c';
}
"#;
    let diags = get_diagnostics(source);
    let ts2413_msgs: Vec<_> = diags
        .iter()
        .filter_map(|(code, message)| (*code == 2413).then_some(message.as_str()))
        .collect();

    assert_eq!(
        ts2413_msgs.len(),
        2,
        "expected conflicts against both wider template patterns, got {diags:#?}"
    );
    assert!(
        ts2413_msgs.iter().any(
            |message| message.contains("'`a${string}a`'") && message.contains("'`a${string}`'")
        ),
        "expected conflict against the prefix template index, got {ts2413_msgs:#?}"
    );
    assert!(
        ts2413_msgs.iter().any(
            |message| message.contains("'`a${string}a`'") && message.contains("'`${string}a`'")
        ),
        "expected conflict against the suffix template index, got {ts2413_msgs:#?}"
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

#[test]
fn ts2395_for_mixed_export_local_type_aliases() {
    // TSC emits TS2395 (not TS2300) when a type alias has both exported and
    // non-exported declarations in the same scope.
    let source = r#"export type A = {}
type A = {}"#;
    let diags = get_diagnostics(source);
    let codes: Vec<u32> = diags.iter().map(|d| d.0).collect();
    assert!(
        codes.contains(&2395),
        "Should emit TS2395 (not TS2300) when type alias has mixed export/local: got {codes:?}"
    );
    assert!(
        !codes.contains(&2300),
        "Should NOT emit TS2300 when TS2395 is emitted: got {codes:?}"
    );
}
