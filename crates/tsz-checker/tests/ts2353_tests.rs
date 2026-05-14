//! Tests for TS2353: Object literal may only specify known properties,
//! and '{prop}' does not exist in type '{Type}'.
//!
//! These tests cover:
//! - Discriminated union excess property checking (narrowed member)
//! - Type alias name display in error messages

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs, load_lib_files};

fn get_diagnostics(source: &str) -> Vec<(u32, String)> {
    let libs = load_lib_files(&["es5.d.ts"]);
    check_source_with_libs(source, "test.ts", CheckerOptions::default(), &libs)
        .iter()
        .filter(|d| d.code != 2318) // Filter missing global type errors
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

#[test]
fn noinfer_union_excess_property_display_orders_object_before_function() {
    let source = r#"
declare function test1<T extends { x: string }>(
  a: T,
  b: NoInfer<T> | (() => NoInfer<T>),
): void;
test1({ x: "foo" }, { x: "bar", y: 42 });

declare function test3<T extends { x: string }>(
  a: T,
  b: NoInfer<T | (() => T)>,
): void;
test3({ x: "foo" }, { x: "bar", y: 42 });
"#;
    let diags = get_diagnostics(source);
    let ts2353: Vec<_> = diags.iter().filter(|d| d.0 == 2353).collect();
    assert_eq!(
        ts2353.len(),
        2,
        "Expected two TS2353 diagnostics, got: {diags:?}",
    );
    assert!(
        ts2353
            .iter()
            .any(|(_, msg)| msg
                .contains("'NoInfer<{ x: string; }> | (() => NoInfer<{ x: string; }>)'")),
        "Expected NoInfer object branch before function branch, got: {ts2353:?}",
    );
    assert!(
        ts2353
            .iter()
            .any(|(_, msg)| msg.contains("'{ x: string; } | (() => { x: string; })'")),
        "Expected outer NoInfer union display to put object branch first, got: {ts2353:?}",
    );
}

#[test]
fn const_assertion_assignment_reports_excess_property() {
    let source = r#"
interface Point {
    x: number;
    y: number;
}

const point: Point = { x: 1, y: 2, z: 3 } as const;
"#;
    let diags = get_diagnostics(source);
    let ts2353: Vec<_> = diags.iter().filter(|d| d.0 == 2353).collect();
    assert_eq!(
        ts2353.len(),
        1,
        "Expected one TS2353 through as const, got: {diags:?}",
    );
    assert!(
        ts2353[0].1.contains("'z'") && ts2353[0].1.contains("'Point'"),
        "Expected TS2353 to mention excess property z and target Point, got: {ts2353:?}",
    );
}

#[test]
fn parenthesized_const_assertion_assignment_reports_excess_property() {
    let source = r#"
interface Point {
    x: number;
    y: number;
}

const point: Point = ({ x: 1, y: 2, z: 3 } as const);
"#;
    let diags = get_diagnostics(source);
    let ts2353: Vec<_> = diags.iter().filter(|d| d.0 == 2353).collect();
    assert_eq!(
        ts2353.len(),
        1,
        "Expected one TS2353 through parenthesized as const, got: {diags:?}",
    );
    assert!(
        ts2353[0].1.contains("'z'") && ts2353[0].1.contains("'Point'"),
        "Expected TS2353 to mention excess property z and target Point, got: {ts2353:?}",
    );
}

#[test]
fn plain_type_assertion_assignment_keeps_excess_property_opaque() {
    let source = r#"
interface Point {
    x: number;
    y: number;
}

const point: Point = { x: 1, y: 2, z: 3 } as Point;
"#;
    let diags = get_diagnostics(source);
    assert!(
        !diags.iter().any(|d| d.0 == 2353),
        "Did not expect TS2353 through plain type assertion, got: {diags:?}",
    );
}

// --- Discriminated union excess property checking ---

#[test]
fn discriminated_union_reports_excess_property_on_narrowed_member() {
    // When a fresh object literal with a discriminant is assigned to a
    // discriminated union, tsc narrows to the matching member and reports
    // excess properties against that member (TS2353), not a generic TS2322.
    let source = r#"
type Square = { kind: "sq", size: number }
type Rectangle = { kind: "rt", x: number, y: number }
type Shape = Square | Rectangle
let s: Shape = { kind: "sq", x: 12 }
"#;
    let diags = get_diagnostics(source);
    // Should emit TS2353, not TS2322
    assert!(
        diags.iter().any(|d| d.0 == 2353),
        "Expected TS2353, got: {diags:?}"
    );
    assert!(
        !diags.iter().any(|d| d.0 == 2322),
        "Should NOT emit TS2322 when TS2353 fires: {diags:?}"
    );
}

#[test]
fn discriminated_union_excess_reports_first_property_by_source_position() {
    // tsc reports the first excess property in source order.
    let source = r#"
type Square = { kind: "sq", size: number }
type Rectangle = { kind: "rt", x: number, y: number }
type Shape = Square | Rectangle
let s: Shape = { kind: "sq", x: 12, y: 13 }
"#;
    let diags = get_diagnostics(source);
    let ts2353 = diags.iter().find(|d| d.0 == 2353);
    assert!(ts2353.is_some(), "Expected TS2353, got: {diags:?}");
    // 'x' appears before 'y' in the source, so 'x' should be reported
    let msg = &ts2353.unwrap().1;
    assert!(
        msg.contains("'x'"),
        "Expected excess property 'x' (first in source), got: {msg}"
    );
}

#[test]
fn discriminated_union_excess_message_uses_type_alias_name() {
    // The error message should reference the type alias name (e.g., "Square")
    // instead of the structural type "{ size: number; kind: \"sq\" }".
    let source = r#"
type Square = { kind: "sq", size: number }
type Rectangle = { kind: "rt", x: number, y: number }
type Shape = Square | Rectangle
let s: Shape = { kind: "sq", x: 12 }
"#;
    let diags = get_diagnostics(source);
    let ts2353 = diags.iter().find(|d| d.0 == 2353);
    assert!(ts2353.is_some(), "Expected TS2353, got: {diags:?}");
    let msg = &ts2353.unwrap().1;
    assert!(
        msg.contains("'Square'"),
        "Expected type alias name 'Square' in message, got: {msg}"
    );
}

#[test]
fn discriminated_union_with_missing_required_and_excess_reports_ts2353() {
    // When a fresh object has a discriminant matching one member but is missing
    // a required property AND has an excess property, tsc reports TS2353 (excess)
    // against the narrowed member. The missing property is a secondary concern.
    let source = r#"
type Square = { kind: "sq", size: number }
type Rectangle = { kind: "rt", x: number, y: number }
type Shape = Square | Rectangle
let s: Shape = { kind: "sq", x: 12, y: 13 }
"#;
    let diags = get_diagnostics(source);
    assert!(
        diags.iter().any(|d| d.0 == 2353),
        "Expected TS2353 for excess 'x' on narrowed Square, got: {diags:?}"
    );
    // Exactly one TS2353 error (for the first excess property)
    let ts2353_count = diags.iter().filter(|d| d.0 == 2353).count();
    assert_eq!(
        ts2353_count, 1,
        "Expected exactly 1 TS2353 error, got {ts2353_count}"
    );
}

#[test]
fn non_discriminated_union_does_not_use_discriminant_narrowing() {
    // When the union has no unit-type discriminant, we shouldn't
    // use discriminant narrowing. This should fall through to normal checking.
    let source = r#"
type A = { x: number, y: number }
type B = { x: number, z: string }
type AB = A | B
let v: AB = { x: 1, w: true }
"#;
    // w is excess in both A and B, so some error should fire
    let diags = get_diagnostics(source);
    let has_any_error = !diags.is_empty();
    assert!(has_any_error, "Expected some error for excess property 'w'");
}

#[test]
fn discriminated_union_narrows_when_partial_member_lacks_discriminator_property() {
    // Regression test: when a target union has multi-discriminator overlap and
    // some members lack one of the discriminator properties, the source's
    // unit-typed discriminator values should still narrow to the unique member
    // that satisfies all discriminators. Mirrors tsc's
    // `discriminateTypeByDiscriminableItems`, which drops members where
    // `getTypeOfPropertyOfType` returns undefined for the discriminator.
    //
    // Conformance reference: `excessPropertyCheckWithUnions.ts` lines 47/48.
    let source = r#"
type Overlapping =
    | { a: 1, b: 1, first: string }
    | { a: 2, second: string }
    | { b: 3, third: string }
let over: Overlapping
over = { a: 1, b: 1, first: "ok", second: "error" }
over = { a: 1, b: 1, first: "ok", third: "error" }
"#;
    let diags = get_diagnostics(source);
    let ts2353: Vec<&(u32, String)> = diags.iter().filter(|d| d.0 == 2353).collect();
    assert_eq!(
        ts2353.len(),
        2,
        "Expected exactly two TS2353 (one per assignment), got: {diags:?}",
    );
    assert!(
        ts2353.iter().any(|(_, msg)| msg.contains("'second'")),
        "Expected TS2353 reporting excess 'second', got: {diags:?}",
    );
    assert!(
        ts2353.iter().any(|(_, msg)| msg.contains("'third'")),
        "Expected TS2353 reporting excess 'third', got: {diags:?}",
    );
    // Narrowed to the single matching member, so the message should mention
    // that member's properties (a, b, first) and not the constituent that
    // owns the excess key.
    for (_, msg) in &ts2353 {
        assert!(
            msg.contains("first"),
            "Excess message should mention narrowed member's 'first' property, got: {msg}",
        );
    }
    assert!(
        !diags.iter().any(|d| d.0 == 2322),
        "Should not emit TS2322 alongside TS2353 for narrowed members, got: {diags:?}",
    );
}

#[test]
fn discriminated_union_narrows_with_renamed_discriminators() {
    // The fix must not depend on the specific discriminator names. Same shape
    // with renamed properties should narrow identically. Locks the structural
    // (rather than name-based) interpretation of the discriminator rule.
    let source = r#"
type Overlap =
    | { p: 1, q: 1, first: string }
    | { p: 2, second: string }
    | { q: 3, third: string }
let v: Overlap
v = { p: 1, q: 1, first: "ok", second: "error" }
"#;
    let diags = get_diagnostics(source);
    assert!(
        diags
            .iter()
            .any(|d| d.0 == 2353 && d.1.contains("'second'")),
        "Expected TS2353 reporting excess 'second', got: {diags:?}",
    );
}

#[test]
fn indirect_discriminant_variable_does_not_trigger_excess_property_narrowing() {
    let source = r#"
type Blah =
    | { type: "foo", abc: string }
    | { type: "bar", xyz: number, extra: any };

declare function thing(blah: Blah): void;

let foo = "foo";
thing({
    type: foo,
    abc: "hello!",
    extra: 123,
});
"#;

    let diags = get_diagnostics(source);
    assert!(
        !diags.iter().any(|d| d.0 == 2353),
        "Indirect discriminants should not narrow EPC to a union member, got: {diags:?}"
    );
}

#[test]
fn mapped_keyof_intersection_prunes_impossible_discriminant_branch() {
    let source = r#"
type Gen = { v: 0 } & (
  { v: 0, a: string } |
  { v: 1, b: string }
);

type Gen2 = {
  [Property in keyof Gen]: string;
};

const ok: Gen2 = { v: "", a: "" };
"#;

    let diags = get_diagnostics(source);
    assert!(
        !diags.iter().any(|d| d.0 == 2353),
        "Mapped discriminant filtering should not report excess-property errors here, got: {diags:?}"
    );
    assert!(diags.is_empty(), "Expected no diagnostics, got: {diags:?}");
}

#[test]
fn mapped_keyof_intersection_keeps_enum_discriminant_member_keys() {
    let source = r#"
enum ABC { A, B }

type Gen<T extends ABC> = { v: T; } & (
  { v: ABC.A, a: string } |
  { v: ABC.B, b: string }
);

type Gen2<T extends ABC> = {
  [Property in keyof Gen<T>]: string;
};

const gen2TypeA: Gen2<ABC.A> = { v: "I am A", a: "" };
const gen2TypeB: Gen2<ABC.B> = { v: "I am B", b: "" };
"#;

    let diags = get_diagnostics(source);
    assert!(
        !diags.iter().any(|d| d.0 == 2353),
        "Enum discriminant mapped keys should not report excess-property errors, got: {diags:?}"
    );
    assert!(
        diags.is_empty(),
        "Expected no diagnostics for enum discriminant mapped keys, got: {diags:?}"
    );
}

#[test]
fn mapped_application_assignment_reports_missing_property_instead_of_ts2322() {
    let source = r#"
enum ABC { A, B }

type Gen<T extends ABC> = { v: T; } & (
  { v: ABC.A, a: string } |
  { v: ABC.B, b: string }
);

type Gen2<T extends ABC> = {
  [Property in keyof Gen<T>]: string;
};

declare let a: Gen2<ABC.A>;
declare let b: Gen2<ABC.B>;
a = b;
b = a;
"#;

    let diags = get_diagnostics(source);
    let ts2741: Vec<_> = diags.iter().filter(|d| d.0 == 2741).collect();
    assert_eq!(
        ts2741.len(),
        2,
        "Expected two TS2741 diagnostics, got: {diags:?}"
    );
    assert!(
        ts2741
            .iter()
            .any(|d| d.1.contains("Property 'a' is missing")),
        "Expected missing-property diagnostic for 'a', got: {diags:?}"
    );
    assert!(
        ts2741
            .iter()
            .any(|d| d.1.contains("Property 'b' is missing")),
        "Expected missing-property diagnostic for 'b', got: {diags:?}"
    );
    assert!(
        !diags.iter().any(|d| d.0 == 2322 || d.0 == 2353),
        "Mapped application assignment should classify as missing properties, got: {diags:?}"
    );
}

#[test]
fn mapped_array_as_clause_missing_named_property_beats_symbol_members() {
    let source = r#"
declare const Symbol: {
  readonly iterator: unique symbol;
  readonly unscopables: unique symbol;
};

type Target = {
  length: number;
  [Symbol.iterator]: unknown;
  [Symbol.unscopables]: unknown;
};

declare let src: {
  [Symbol.iterator]: unknown;
  [Symbol.unscopables]: unknown;
};

let tgt: Target = src;
"#;

    let diags = get_diagnostics(source);
    let ts2741: Vec<_> = diags.iter().filter(|d| d.0 == 2741).collect();
    assert_eq!(
        ts2741.len(),
        1,
        "Expected one TS2741 diagnostic, got: {diags:?}"
    );
    assert!(
        ts2741[0].1.contains("Property 'length' is missing"),
        "Expected missing-property diagnostic for 'length', got: {}",
        ts2741[0].1
    );
    assert!(
        !diags.iter().any(|d| d.0 == 2739),
        "Late-bound symbol members should not inflate this to TS2739, got: {diags:?}"
    );
}

#[test]
fn object_literal_union_normalization_avoids_ts2339_and_ts2353() {
    let source = r#"
let a1 = [{ a: 0 }, { a: 1, b: "x" }, { a: 2, b: "y", c: true }][0];
a1.b;
a1.c;
a1 = { a: 0, b: 0 };

let d1 = [{ kind: 'a', pos: { x: 0, y: 0 } }, { kind: 'b', pos: !true ? { a: "x" } : { b: 0 } }][0];
d1.pos.x;
d1.pos.y;
d1.pos.a;
d1.pos.b;
"#;

    let diags = get_diagnostics(source);
    assert!(
        !diags.iter().any(|d| d.0 == 2339),
        "Normalized object-literal unions should not report missing-property reads here, got: {diags:?}"
    );
    assert!(
        !diags.iter().any(|d| d.0 == 2353),
        "Normalized object-literal unions should not report excess properties here, got: {diags:?}"
    );
    assert!(
        diags.iter().any(|d| d.0 == 2322),
        "Expected the bad assignment to remain a TS2322, got: {diags:?}"
    );
}

#[test]
fn logical_or_fresh_empty_object_read_succeeds_write_errors() {
    // tsc allows reads on (options || {}).a because the {} is a fresh object
    // literal: tsc treats it as partial, contributing undefined for properties
    // that exist on other union members. Writes still error because the
    // property doesn't physically exist on the {} member, so they must not
    // silently fall through to assignability against `string | undefined`.
    let source = r#"
function foo(options?: { a: string, b: number }) {
  let x1 = (options || {}).a;
  let x2 = (options || {})["a"];
  (options || {}).a = 1;
  (options || {})["a"] = 1;
}
"#;

    let diags = get_diagnostics(source);
    let ts2339_count = diags.iter().filter(|d| d.0 == 2339).count();
    let ts7053_count = diags.iter().filter(|d| d.0 == 7053).count();
    assert!(
        ts2339_count == 1,
        "Dot write through || should report exactly one TS2339 for the fresh empty-object branch, got: {diags:?}"
    );
    assert!(
        ts7053_count == 1,
        "Element write through || should report exactly one TS7053 for the fresh empty-object branch, got: {diags:?}"
    );
    assert!(
        !diags.iter().any(|d| d.0 == 2322),
        "Write through || should not degrade to TS2322 from a synthetic undefined write type, got: {diags:?}"
    );
}

#[test]
fn nullish_coalescing_fresh_empty_object_read_succeeds_write_errors() {
    // tsc allows reads on (options ?? {}).a because the {} is a fresh object
    // literal: tsc treats it as partial, contributing undefined for properties
    // that exist on other union members. Writes still error because the
    // property doesn't physically exist on the {} member, so the write path
    // should surface missing-property diagnostics rather than TS2322.
    let source = r#"
function foo(options?: { a: string, b: number } | null) {
  let x1 = (options ?? {}).a;
  let x2 = (options ?? {})["a"];
  (options ?? {}).a = 1;
  (options ?? {})["a"] = 1;
}
"#;

    let diags = get_diagnostics(source);
    let ts2339_count = diags.iter().filter(|d| d.0 == 2339).count();
    let ts7053_count = diags.iter().filter(|d| d.0 == 7053).count();
    assert!(
        ts2339_count == 1,
        "Dot write through ?? should report exactly one TS2339 for the fresh empty-object branch, got: {diags:?}"
    );
    assert!(
        ts7053_count == 1,
        "Element write through ?? should report exactly one TS7053 for the fresh empty-object branch, got: {diags:?}"
    );
    assert!(
        !diags.iter().any(|d| d.0 == 2322),
        "Write through ?? should not degrade to TS2322 from a synthetic undefined write type, got: {diags:?}"
    );
}

#[test]
fn nested_generic_callee_does_not_preinstantiate_from_outer_call_target() {
    let source = r#"
interface Effect<out A> {
  readonly EffectTypeId: {
    readonly _A: (_: never) => A;
  };
}

declare function pipe<A, B>(a: A, ab: (a: A) => B): B;

declare const repeat: {
  <A>(
    options: {
      until?: (_: A) => boolean;
    },
  ): (self: Effect<A>) => Effect<A>;
};

pipe(
  {} as Effect<boolean>,
  repeat({
    until: (x) => {
      return x;
    },
  }),
);
"#;

    let diags = get_diagnostics(source);
    assert!(
        !diags
            .iter()
            .any(|d| d.0 == 2353 || d.0 == 7006 || d.0 == 2345),
        "Nested generic callee should infer from its own signature before outer call compatibility, got: {diags:?}"
    );
    assert!(diags.is_empty(), "Expected no diagnostics, got: {diags:?}");
}

#[test]
fn promise_resolve_argument_skips_epc_for_infer_placeholder_target() {
    let source = r#"
interface PromiseLike<T> {}
interface Obj { key: "value"; }
declare function withResolver<T>(
  cb: (resolve: (value: T | PromiseLike<T>) => void) => void,
): PromiseLike<T>;
declare function expectObj(value: PromiseLike<Obj>): void;

expectObj(
  withResolver(resolve => {
    resolve({ key: "value" });
  }),
);
"#;

    let diags = get_diagnostics(source);
    assert!(
        !diags.iter().any(|d| d.0 == 2353),
        "Promise resolve arguments should not run EPC against __infer targets, got: {diags:?}"
    );
    assert!(diags.is_empty(), "Expected no diagnostics, got: {diags:?}");
}

// --- Type alias name display in diagnostics ---

#[test]
fn type_alias_name_displayed_in_ts2322_message() {
    // Type alias names should appear in TS2322 messages.
    // Before the fix, this would show the structural type instead.
    let source = r#"
type Point = { x: number, y: number }
let p: Point = { x: 1, z: 3 }
"#;
    let diags = get_diagnostics(source);
    // We expect an error referencing 'Point'
    let has_point_name = diags.iter().any(|d| d.1.contains("'Point'"));
    assert!(
        has_point_name,
        "Expected type alias 'Point' in error message, got: {diags:?}"
    );
}

#[test]
fn interface_name_still_displayed_correctly() {
    // Interfaces already displayed their names correctly; ensure no regression.
    let source = r#"
interface Foo { a: number }
let f: Foo = { a: 1, b: 2 }
"#;
    let diags = get_diagnostics(source);
    let has_foo_name = diags.iter().any(|d| d.1.contains("'Foo'"));
    assert!(
        has_foo_name,
        "Expected interface name 'Foo' in error message, got: {diags:?}"
    );
}

#[test]
fn excess_property_intersection_annotation_preserves_declared_type_display() {
    let source = r#"
interface Book { foreword: string }
interface Cover { color?: string }
let book: Book & Cover = { foreword: "hi", colour: "blue" };
"#;

    let diags = get_diagnostics(source);
    let ts2561 = diags.iter().find(|d| d.0 == 2561).expect("expected TS2561");
    assert!(
        ts2561.1.contains("'Book & Cover'"),
        "Expected declared intersection display in TS2561, got: {}",
        ts2561.1
    );
}

#[test]
fn excess_property_object_intersection_display_keeps_object_member() {
    let source = r#"
const value: object & { x: string } = { z: "abc" };
"#;

    let diags = get_diagnostics(source);
    let ts2353 = diags.iter().find(|d| d.0 == 2353).expect("expected TS2353");
    assert!(
        ts2353.1.contains("'object & { x: string; }'"),
        "Expected object intersection display in TS2353, got: {}",
        ts2353.1
    );
}

#[test]
fn generic_intersection_target_skips_excess_property_check() {
    let source = r#"
interface IFoo {}
function test<T extends IFoo>() {
    const value: T & { prop: boolean } = { name: "test", prop: true };
}
"#;

    let diags = get_diagnostics(source);
    assert!(
        !diags.iter().any(|d| d.0 == 2353),
        "Did not expect TS2353 for generic intersection assignment, got: {diags:?}"
    );
}

#[test]
fn primitive_intersection_target_uses_ts2322_instead_of_epc() {
    let source = r#"
interface Book { foreword: string }
const value: Book & number = { foreword: "hi", price: 10.99 };
"#;

    let diags = get_diagnostics(source);
    assert!(
        diags.iter().any(|d| d.0 == 2322),
        "Expected TS2322 for primitive intersection assignment, got: {diags:?}"
    );
    assert!(
        !diags.iter().any(|d| d.0 == 2353),
        "Did not expect TS2353 for primitive intersection assignment, got: {diags:?}"
    );
}

#[test]
fn union_with_generic_member_still_checks_concrete_member_for_excess_property() {
    let source = r#"
interface IFoo {}
function test<T extends IFoo>() {
    const value: T | { prop: boolean } = { name: "test", prop: true };
}
"#;

    let diags = get_diagnostics(source);
    let ts2353 = diags.iter().find(|d| d.0 == 2353).expect("expected TS2353");
    assert!(
        ts2353.1.contains("'{ prop: boolean; }'"),
        "Expected TS2353 against the concrete union member, got: {}",
        ts2353.1
    );
}

#[test]
fn union_with_generic_intersection_member_reports_concrete_member_display() {
    let source = r#"
function test<T extends IFoo>() {
    const value: T & { prop: boolean } | { name: string } = { name: "test", prop: true };
}
"#;

    let diags = get_diagnostics(source);
    let ts2353 = diags.iter().find(|d| d.0 == 2353).expect("expected TS2353");
    assert!(
        ts2353.1.contains("'{ name: string; }'"),
        "Expected TS2353 against the concrete union member, got: {}",
        ts2353.1
    );
    assert!(
        !ts2353.1.contains("|"),
        "Expected TS2353 display to omit the generic intersection union member, got: {}",
        ts2353.1
    );
}

#[test]
fn non_generic_union_excess_property_keeps_union_display() {
    let source = r#"
interface Cover {
    color?: string;
}
const value: Cover | Cover[] = { couleur: "non" };
"#;

    let diags = get_diagnostics(source);
    let ts2353 = diags.iter().find(|d| d.0 == 2353).expect("expected TS2353");
    assert!(
        ts2353.1.contains("'Cover[] | Cover'") || ts2353.1.contains("'Cover | Cover[]'"),
        "Expected TS2353 to keep the full union target, got: {}",
        ts2353.1
    );
}

#[test]
fn recursive_array_union_excess_property_uses_outer_alias_display() {
    let source = r#"
type Style = StyleBase | StyleArray;
interface StyleArray extends Array<Style> { }
interface StyleBase { foo: string; }

const blah: Style = [
    [[{
        foo: "asdf",
        jj: 1
    }]]
];
"#;

    let diags = get_diagnostics(source);
    let ts2353 = diags.iter().find(|d| d.0 == 2353).expect("expected TS2353");
    assert!(
        ts2353.1.contains("'Style'"),
        "Expected TS2353 to mention the recursive alias Style, got: {}",
        ts2353.1
    );
    assert!(
        !ts2353.1.contains("'StyleBase'"),
        "Expected TS2353 not to collapse to StyleBase, got: {}",
        ts2353.1
    );
}

#[test]
fn overlapping_discriminant_optionals_report_later_excess_property() {
    let source = r#"
interface Common {
    type: "A" | "B" | "C" | "D";
    n: number;
}
interface A {
    type: "A";
    a?: number;
}
interface B {
    type: "B";
    b?: number;
}

type CommonWithOverlappingOptionals = Common | (Common & A) | (Common & B);

const c1: CommonWithOverlappingOptionals = {
    type: "A",
    n: 1,
    a: 1,
    b: 1,
};
"#;

    let diags = get_diagnostics(source);
    let ts2353: Vec<_> = diags.iter().filter(|d| d.0 == 2353).collect();
    assert_eq!(
        ts2353.len(),
        1,
        "expected one TS2353 for 'b', got: {diags:?}"
    );
    assert!(
        ts2353[0].1.contains("'b'"),
        "TS2353 should mention 'b', got: {}",
        ts2353[0].1
    );
    assert!(
        ts2353[0].1.contains("Common | (Common & A)"),
        "TS2353 should use narrowed Common | (Common & A), got: {}",
        ts2353[0].1
    );
}

#[test]
fn function_argument_contextual_typed_object_literal_reports_property_token_excesses() {
    let source = r#"
interface I {
    value: string;
    toString: (t: string) => string;
}

function f2(args: I) {}

f2({ hello: 1 });
f2({ value: "", what: 1 });
"#;

    let diags = get_diagnostics(source);
    let ts2353: Vec<_> = diags.iter().filter(|d| d.0 == 2353).collect();
    assert_eq!(
        ts2353.len(),
        2,
        "Expected two TS2353 errors, got: {diags:?}"
    );
    assert!(
        ts2353.iter().any(|d| d.1.contains("'hello'")),
        "Expected TS2353 for 'hello', got: {diags:?}"
    );
    assert!(
        ts2353.iter().any(|d| d.1.contains("'what'")),
        "Expected TS2353 for 'what', got: {diags:?}"
    );
}

// --- Intersection with index signatures ---

// --- Post-inference EPC for generic calls with mapped type parameters ---

#[test]
fn generic_call_mapped_type_emits_epc_after_inference() {
    // When a generic function's parameter is a mapped type like
    // {[K in keyof T & keyof X]: T[K]}, and inference resolves T from
    // the argument, post-inference EPC should catch excess properties
    // that don't exist in the intersection of keyof T & keyof X.
    //
    // Before the fix, generic_excess_skip would suppress EPC entirely
    // because the raw param type contained type parameters.
    let source = r#"
type XNumber = { x: number }
declare function foo<T extends XNumber>(props: {[K in keyof T & keyof XNumber]: T[K]}): T;
foo({x: 1, y: "foo"});
"#;
    let diags = get_diagnostics(source);
    assert!(
        diags.iter().any(|d| d.0 == 2353),
        "Expected TS2353 for excess property 'y' in generic call with mapped type, got: {diags:?}"
    );
    assert!(
        !diags.iter().any(|d| d.0 == 2322),
        "Expected post-inference EPC to suppress TS2322 in favor of TS2353, got: {diags:?}"
    );
}

#[test]
fn generic_call_mapped_type_no_excess_no_error() {
    // When the object literal matches exactly, no EPC error should fire.
    let source = r#"
type XNumber = { x: number }
declare function foo<T extends XNumber>(props: {[K in keyof T & keyof XNumber]: T[K]}): T;
foo({x: 1});
"#;
    let diags = get_diagnostics(source);
    assert!(
        !diags.iter().any(|d| d.0 == 2353),
        "No TS2353 expected when no excess properties, got: {diags:?}"
    );
}

/// Tests for multi-discriminant narrowing in excess property checks.
/// When an object literal has multiple discriminant properties, tsc applies
/// ALL of them sequentially to narrow the union before checking excess properties.
#[test]
fn multi_discriminant_excess_property_check_applies_all_discriminants() {
    // Repro from TypeScript#32657 / conformance test excessPropertyCheckWithMultipleDiscriminants
    // Two discriminants: p1 narrows to [member0, member2], then p2 further narrows to [member2].
    let source = r#"
type DisjointDiscriminants = { p1: 'left'; p2: true; p3: number } | { p1: 'right'; p2: false; p4: string } | { p1: 'left'; p2: boolean };

const a: DisjointDiscriminants = {
    p1: 'left',
    p2: false,
    p3: 42,
    p4: "hello"
};
"#;
    let diags = get_diagnostics(source);
    let ts2353: Vec<_> = diags.iter().filter(|d| d.0 == 2353).collect();
    assert!(!ts2353.is_empty(), "Expected TS2353, got: {diags:?}");
    // After multi-discriminant narrowing: p1='left' and p2=false narrows to
    // { p1: 'left'; p2: boolean } only. p3 is the first excess property in that member.
    let msg = &ts2353[0].1;
    assert!(
        msg.contains("'p3'"),
        "Expected excess property 'p3' (narrowed by both p1+p2 discriminants), got: {msg}"
    );
    // The display type should reference only the narrowed member
    assert!(
        msg.contains("p2: boolean") || msg.contains("boolean"),
        "Expected display type to reflect discriminant-narrowed member, got: {msg}"
    );
}

#[test]
fn multi_discriminant_first_discriminant_only_case() {
    // When only one discriminant applies (p1='right' narrows to a single member),
    // excess check uses that single member.
    let source = r#"
type DisjointDiscriminants = { p1: 'left'; p2: true; p3: number } | { p1: 'right'; p2: false; p4: string } | { p1: 'left'; p2: boolean };

const c: DisjointDiscriminants = {
    p1: 'right',
    p2: false,
    p3: 42,
    p4: "hello"
};
"#;
    let diags = get_diagnostics(source);
    let ts2353: Vec<_> = diags.iter().filter(|d| d.0 == 2353).collect();
    assert!(!ts2353.is_empty(), "Expected TS2353, got: {diags:?}");
    // p1='right' narrows to { p1: 'right'; p2: false; p4: string } only.
    // p3 is excess in that member.
    let msg = &ts2353[0].1;
    assert!(
        msg.contains("'p3'"),
        "Expected excess property 'p3' for the right-narrowed member, got: {msg}"
    );
}

#[test]
fn intersection_with_index_signatures_nested_excess_property() {
    // When target is an intersection of types with string index signatures,
    // the outer property names are all valid (covered by index sig), but
    // the nested property values must be checked against the intersection
    // of index signature value types.
    //
    let source = r#"
let x: { [x: string]: { a: 0 } } & { [x: string]: { b: 0 } };
x = { y: { a: 0, b: 0, c: 0 } };
"#;
    let diags = get_diagnostics(source);
    let relevant: Vec<_> = diags.iter().filter(|d| d.0 != 2318).collect();
    assert!(
        relevant.iter().any(|d| d.0 == 2353),
        "Expected TS2353 for excess property 'c' against {{a: 0}} & {{b: 0}}, got: {relevant:?}"
    );
}

#[test]
fn simple_object_literal_with_three_excess_properties_reports_only_first() {
    // tsc reports only the first excess property in source order per object
    // literal — even when several properties are excess. Repro from
    // conformance test destructuringParameterProperties5.ts:
    // `{ x1: 10, x2: "", x3: true }` against `{ x: number; y: string; z: boolean }`
    // should produce exactly one TS2353 (for 'x1'), not three.
    let source = r#"
type ObjType1 = { x: number; y: string; z: boolean };
let v: ObjType1 = { x1: 10, x2: "", x3: true };
"#;
    let diags = get_diagnostics(source);
    let ts2353: Vec<_> = diags.iter().filter(|d| d.0 == 2353).collect();
    assert_eq!(
        ts2353.len(),
        1,
        "Expected exactly one TS2353 for the first excess property, got {} ({:?})",
        ts2353.len(),
        diags
    );
    let msg = &ts2353[0].1;
    assert!(
        msg.contains("'x1'"),
        "Expected the first excess property 'x1' to be reported, got: {msg}"
    );
}

#[test]
fn union_target_multiple_excess_properties_reports_only_first() {
    // For a non-discriminated union target where multiple source properties
    // are missing from every union member, tsc still reports only the first.
    let source = r#"
type A = { x: number; y: number };
type B = { x: number; z: string };
type AB = A | B;
let v: AB = { x: 1, foo: 1, bar: 2, baz: 3 };
"#;
    let diags = get_diagnostics(source);
    let ts2353: Vec<_> = diags.iter().filter(|d| d.0 == 2353).collect();
    assert_eq!(
        ts2353.len(),
        1,
        "Expected exactly one TS2353 across union excess checks, got {} ({:?})",
        ts2353.len(),
        diags
    );
    let msg = &ts2353[0].1;
    assert!(
        msg.contains("'foo'"),
        "Expected the first excess property 'foo' (earliest in source) to be reported, got: {msg}"
    );
}

#[test]
fn intersection_target_multiple_excess_properties_reports_only_first() {
    // tsc emits a single TS2353 for the first excess property when several
    // properties miss every member of an intersection target.
    let source = r#"
interface A { a: number }
interface B { b: number }
let v: A & B = { a: 1, b: 2, foo: 1, bar: 2 };
"#;
    let diags = get_diagnostics(source);
    let ts2353: Vec<_> = diags.iter().filter(|d| d.0 == 2353).collect();
    assert_eq!(
        ts2353.len(),
        1,
        "Expected exactly one TS2353 for intersection excess, got {} ({:?})",
        ts2353.len(),
        diags
    );
    let msg = &ts2353[0].1;
    assert!(
        msg.contains("'foo'"),
        "Expected the first excess property 'foo' to be reported, got: {msg}"
    );
}

// ────────────────────────────────────────────────────────────────────────────
// Multiple template literal index signatures — interface & type literal
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn interface_with_two_template_literal_index_signatures_accepts_both_patterns() {
    // Structural rule: a property key is valid if it matches ANY index signature
    // pattern, not just the first one declared.
    let source = r#"
interface TemplateIndexed {
    [key: `data-${string}`]: string;
    [key: `aria-${string}`]: string;
}
const ti: TemplateIndexed = {
    "data-id": "123",
    "aria-label": "test",
};
"#;
    let diags = get_diagnostics(source);
    let ts2353: Vec<_> = diags.iter().filter(|d| d.0 == 2353).collect();
    assert!(
        ts2353.is_empty(),
        "Both 'data-*' and 'aria-*' properties should be accepted; got: {diags:?}"
    );
}

#[test]
fn interface_with_two_template_literal_index_signatures_rejects_non_matching_property() {
    let source = r#"
interface TemplateIndexed {
    [key: `data-${string}`]: string;
    [key: `aria-${string}`]: string;
}
const ti: TemplateIndexed = {
    "foo-bar": "test",
};
"#;
    let diags = get_diagnostics(source);
    let ts2353: Vec<_> = diags.iter().filter(|d| d.0 == 2353).collect();
    assert_eq!(
        ts2353.len(),
        1,
        "Property 'foo-bar' doesn't match either pattern; expected TS2353, got: {diags:?}"
    );
    assert!(
        ts2353[0].1.contains("'foo-bar'"),
        "Expected 'foo-bar' in error message, got: {:?}",
        ts2353[0].1
    );
}

#[test]
fn interface_with_three_template_literal_index_signatures_accepts_all_patterns() {
    // Verify the fix generalizes beyond two patterns (three or more).
    let source = r#"
interface MultiPattern {
    [key: `get${string}`]: () => unknown;
    [key: `set${string}`]: (v: unknown) => void;
    [key: `on${string}`]: (e: unknown) => void;
}
const handlers: MultiPattern = {
    getName: () => "test",
    setValue: (_v: unknown) => {},
    onClick: (_e: unknown) => {},
};
"#;
    let diags = get_diagnostics(source);
    let ts2353: Vec<_> = diags.iter().filter(|d| d.0 == 2353).collect();
    assert!(
        ts2353.is_empty(),
        "All three patterns should be accepted; got: {diags:?}"
    );
}

#[test]
fn type_literal_with_two_template_literal_index_signatures_accepts_both_patterns() {
    // Same fix applies to type aliases with object type literals.
    let source = r#"
type TemplateIndexed = {
    [key: `data-${string}`]: string;
    [key: `aria-${string}`]: string;
};
const ti: TemplateIndexed = {
    "data-id": "123",
    "aria-label": "test",
};
"#;
    let diags = get_diagnostics(source);
    let ts2353: Vec<_> = diags.iter().filter(|d| d.0 == 2353).collect();
    assert!(
        ts2353.is_empty(),
        "Both 'data-*' and 'aria-*' properties should be accepted in type literal; got: {diags:?}"
    );
}

#[test]
fn type_literal_with_two_template_literal_index_signatures_rejects_non_matching_property() {
    let source = r#"
type TemplateIndexed = {
    [key: `data-${string}`]: string;
    [key: `aria-${string}`]: string;
};
const ti: TemplateIndexed = {
    "foo-bar": "test",
};
"#;
    let diags = get_diagnostics(source);
    let ts2353: Vec<_> = diags.iter().filter(|d| d.0 == 2353).collect();
    assert_eq!(
        ts2353.len(),
        1,
        "Property 'foo-bar' doesn't match either pattern in type literal; expected TS2353, got: {diags:?}"
    );
}
