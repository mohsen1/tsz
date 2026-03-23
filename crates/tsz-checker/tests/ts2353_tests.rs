//! Tests for TS2353: Object literal may only specify known properties,
//! and '{prop}' does not exist in type '{Type}'.
//!
//! These tests cover:
//! - Discriminated union excess property checking (narrowed member)
//! - Type alias name display in error messages

use std::path::Path;
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_binder::lib_loader::LibFile;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn load_lib_files_for_test() -> Vec<Arc<LibFile>> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let lib_paths = [
        manifest_dir.join("scripts/node_modules/typescript/lib/lib.es5.d.ts"),
        manifest_dir.join("scripts/conformance/node_modules/typescript/lib/lib.es5.d.ts"),
        manifest_dir.join("scripts/emit/node_modules/typescript/lib/lib.es5.d.ts"),
        manifest_dir.join("TypeScript/src/lib/es5.d.ts"),
        manifest_dir.join("TypeScript/node_modules/typescript/lib/lib.es5.d.ts"),
        manifest_dir.join("../TypeScript/src/lib/es5.d.ts"),
        manifest_dir.join("../TypeScript/node_modules/typescript/lib/lib.es5.d.ts"),
        manifest_dir.join("../../scripts/node_modules/typescript/lib/lib.es5.d.ts"),
        manifest_dir.join("../../scripts/conformance/node_modules/typescript/lib/lib.es5.d.ts"),
        manifest_dir.join("../../scripts/emit/node_modules/typescript/lib/lib.es5.d.ts"),
        manifest_dir.join("../../TypeScript/src/lib/es5.d.ts"),
        manifest_dir.join("../../TypeScript/node_modules/typescript/lib/lib.es5.d.ts"),
    ];

    for lib_path in &lib_paths {
        if lib_path.exists()
            && let Ok(content) = std::fs::read_to_string(lib_path)
        {
            let lib_file = LibFile::from_source("lib.es5.d.ts".to_string(), content);
            return vec![Arc::new(lib_file)];
        }
    }

    Vec::new()
}

fn get_diagnostics(source: &str) -> Vec<(u32, String)> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let lib_files = load_lib_files_for_test();

    let mut binder = BinderState::new();
    if lib_files.is_empty() {
        binder.bind_source_file(parser.get_arena(), root);
    } else {
        binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);
    }

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );

    if !lib_files.is_empty() {
        let lib_contexts: Vec<tsz_checker::context::LibContext> = lib_files
            .iter()
            .map(|lib| tsz_checker::context::LibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        checker.ctx.set_lib_contexts(lib_contexts);
        checker.ctx.set_actual_lib_file_count(lib_files.len());
    }
    checker.check_source_file(root);

    checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code != 2318) // Filter missing global type errors
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
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
fn property_access_reads_preserve_union_presence_before_write_widening() {
    let source = r#"
function foo(options?: { a: string, b: number }) {
  let x1 = (options || {}).a;
  let x2 = (options || {})["a"];
  (options || {}).a = 1;
  (options || {})["a"] = 1;
}
"#;

    let diags = get_diagnostics(source);
    assert!(
        diags.iter().any(|d| d.0 == 2339),
        "Read access should still report TS2339 before widening, got: {diags:?}"
    );
    // Element access with literal string key "a" on a union type emits TS7053
    // (implicit any from index expression), not TS2339. TS2339 is reserved for
    // literal keys on non-union types; unions use TS7053 because partial index
    // signature presence across members causes the index failure.
    assert!(
        diags.iter().any(|d| d.0 == 7053),
        "Element read access on union should report TS7053 before widening, got: {diags:?}"
    );
    assert!(
        !diags.iter().any(|d| d.0 == 2322),
        "Write access through || should not emit TS2322 after write-target widening, got: {diags:?}"
    );
}

#[test]
fn nullish_coalescing_write_target_widens_without_changing_read_presence() {
    let source = r#"
function foo(options?: { a: string, b: number } | null) {
  let x1 = (options ?? {}).a;
  let x2 = (options ?? {})["a"];
  (options ?? {}).a = 1;
  (options ?? {})["a"] = 1;
}
"#;

    let diags = get_diagnostics(source);
    assert!(
        diags.iter().any(|d| d.0 == 2339),
        "Read access through ?? should still report TS2339 before widening, got: {diags:?}"
    );
    // Element access with literal string key "a" on a union type emits TS7053.
    assert!(
        diags.iter().any(|d| d.0 == 7053),
        "Element read access through ?? on union should report TS7053, got: {diags:?}"
    );
    assert!(
        !diags.iter().any(|d| d.0 == 2322),
        "Write access through ?? should not emit TS2322 after write-target widening, got: {diags:?}"
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
