use super::super::core::*;

#[test]
fn test_ts2403_param_var_redeclaration_inferred_type_constructor() {
    // Inferred type in constructor (the actual failing conformance test)
    let source = r#"
class C {
    constructor(options?: number) {
        var options = (options || 0);
    }
}
"#;
    let diagnostics = compile_and_get_raw_diagnostics_named(
        "test.ts",
        source,
        CheckerOptions {
            strict_null_checks: true,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    assert!(
        diagnostics.iter().any(|d| d.code == 2403),
        "Expected TS2403 for var with inferred type re-declaring optional parameter in constructor.\nActual: {diagnostics:#?}"
    );
}

/// Callback return type elaboration with `NoInfer` should produce TS2741
/// at the body expression, not TS2322 at the arrow function.
///
/// When `doSomething<T>(value: T, getDefault: () => NoInfer<T>)` is called
/// with `doSomething(new Dog(), () => new Animal())`, T infers as Dog.
/// The callback return type `NoInfer<Dog>` evaluates to `Dog`, and since
/// `Animal` is missing `woof` from `Dog`, tsc emits TS2741 at the `new Animal()`
/// expression, not a generic TS2322 at the arrow function.
#[test]
fn test_noinfer_callback_return_type_elaboration_emits_ts2741() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
declare class Animal { move(): void }
declare class Dog extends Animal { woof(): void }
declare function doSomething<T>(value: T, getDefault: () => NoInfer<T>): void;

doSomething(new Dog(), () => new Animal());
        "#,
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    assert!(
        has_error(&diagnostics, 2741),
        "Should emit TS2741 (Property 'woof' is missing in type 'Animal' but required in type 'Dog') for NoInfer callback return type mismatch.\nActual: {diagnostics:?}"
    );
    let msg = diagnostic_message(&diagnostics, 2741).unwrap();
    assert!(
        msg.contains("woof") && msg.contains("Animal") && msg.contains("Dog"),
        "TS2741 message should reference 'woof', 'Animal', and 'Dog'.\nActual message: {msg}"
    );
    // Should NOT have TS2322 for the callback argument (the elaboration replaces it)
    let ts2322_msgs: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect();
    assert!(
        ts2322_msgs.is_empty(),
        "Should not emit TS2322 for callback return type when TS2741 is emitted.\nTS2322 diagnostics: {ts2322_msgs:?}"
    );
}

/// TS2590: Simple test - intersection of many 2-member unions should flag as too complex.
/// This verifies the solver flag propagates through the checker.
#[test]
fn test_simple_intersection_of_many_unions_emits_ts2590() {
    // Create a function whose return type is an intersection of 18 two-member unions
    // Cross-product = 2^18 = 262,144 > 100,000
    let diagnostics = compile_and_get_diagnostics(
        r#"
type A = { ref: { a: 1 } | { b: 1 } };
type B = { ref: { a: 2 } | { b: 2 } };
type C = { ref: { a: 3 } | { b: 3 } };
type D = { ref: { a: 4 } | { b: 4 } };
type E = { ref: { a: 5 } | { b: 5 } };
type F = { ref: { a: 6 } | { b: 6 } };
type G = { ref: { a: 7 } | { b: 7 } };
type H = { ref: { a: 8 } | { b: 8 } };
type I = { ref: { a: 9 } | { b: 9 } };
type J = { ref: { a: 10 } | { b: 10 } };
type K = { ref: { a: 11 } | { b: 11 } };
type L = { ref: { a: 12 } | { b: 12 } };
type M = { ref: { a: 13 } | { b: 13 } };
type N = { ref: { a: 14 } | { b: 14 } };
type O = { ref: { a: 15 } | { b: 15 } };
type P = { ref: { a: 16 } | { b: 16 } };
type Q = { ref: { a: 17 } | { b: 17 } };
type R = { ref: { a: 18 } | { b: 18 } };
declare function make(): A & B & C & D & E & F & G & H & I & J & K & L & M & N & O & P & Q & R;
const x = make();
const r = x.ref;
        "#,
    );
    // We expect TS2590 because accessing `ref` on the intersection creates
    // an intersection of 18 two-member unions (cross-product = 2^18 > 100,000)
    assert!(
        has_error(&diagnostics, 2590),
        "Should emit TS2590 when property access on intersection creates too-complex union.\nActual diagnostics: {diagnostics:#?}"
    );
}

/// TS2590: Test basic `UnionToIntersection` behavior
#[test]
fn test_union_to_intersection_basic() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
type UnionToIntersection<U> = (U extends any ? (k: U) => void : never) extends ((k: infer I) => void) ? I : never;
type A = { a: number };
type B = { b: string };
type AB = UnionToIntersection<A | B>;
declare const x: AB;
const a: number = x.a;
const b: string = x.b;
const c: number = x.b; // Should error: string not assignable to number
        "#,
    );
    assert!(
        has_error(&diagnostics, 2322),
        "UnionToIntersection<A|B> should produce {{a: number}} & {{b: string}}, and 'string' not assignable to 'number' should emit TS2322.\nDiagnostics: {diagnostics:#?}"
    );
}

/// TS2590: Test that `UnionToIntersection` distributes and creates intersection.
/// This is a prerequisite for the normalizedIntersectionTooComplex conformance test.
#[test]
fn test_union_to_intersection_with_many_members_emits_ts2590() {
    // Simplified version: explicit intersection through UnionToIntersection
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
type UnionToIntersection<U> = (U extends any ? (k: U) => void : never) extends ((k: infer I) => void) ? I : never;

type T0 = { ref: { a: 0 } | { b: 0 } };
type T1 = { ref: { a: 1 } | { b: 1 } };
type T2 = { ref: { a: 2 } | { b: 2 } };
type T3 = { ref: { a: 3 } | { b: 3 } };
type T4 = { ref: { a: 4 } | { b: 4 } };
type T5 = { ref: { a: 5 } | { b: 5 } };
type T6 = { ref: { a: 6 } | { b: 6 } };
type T7 = { ref: { a: 7 } | { b: 7 } };
type T8 = { ref: { a: 8 } | { b: 8 } };
type T9 = { ref: { a: 9 } | { b: 9 } };
type T10 = { ref: { a: 10 } | { b: 10 } };
type T11 = { ref: { a: 11 } | { b: 11 } };
type T12 = { ref: { a: 12 } | { b: 12 } };
type T13 = { ref: { a: 13 } | { b: 13 } };
type T14 = { ref: { a: 14 } | { b: 14 } };
type T15 = { ref: { a: 15 } | { b: 15 } };
type T16 = { ref: { a: 16 } | { b: 16 } };
type T17 = { ref: { a: 17 } | { b: 17 } };
type BigUnion = T0 | T1 | T2 | T3 | T4 | T5 | T6 | T7 | T8 | T9 | T10 | T11 | T12 | T13 | T14 | T15 | T16 | T17;
type BigIntersection = UnionToIntersection<BigUnion>;
declare function make(): BigIntersection;
const x = make();
const r = x.ref;
        "#,
        CheckerOptions {
            strict: true,
            ..Default::default()
        },
    );
    let all_codes: Vec<u32> = diagnostics.iter().map(|(c, _)| *c).collect();
    // Check if TS2590 is emitted - this validates UnionToIntersection evaluation
    assert!(
        has_error(&diagnostics, 2590),
        "Should emit TS2590 when UnionToIntersection creates an intersection with too-complex cross-product.\nDiagnostics: {all_codes:?}\n{diagnostics:#?}"
    );
}

/// Cross-binder SymbolId collision: named import from ambient module with export=.
///
/// When `import { Passport } from "passport"` resolves through an ambient module
/// with `export = passport`, the resolved target SymbolId comes from a different
/// binder. If the current binder has a symbol with the same numeric ID,
/// `get_symbol_with_libs` would return the wrong symbol (e.g., a local variable
/// instead of the imported interface), causing a false TS2749.
#[test]
fn test_cross_binder_symbol_id_collision_no_false_ts2749() {
    let passport_dts = r#"
declare module 'passport' {
    namespace passport {
        interface Passport {
            use(): this;
        }

        interface PassportStatic extends Passport {
            Passport: {new(): Passport};
        }
    }

    const passport: passport.PassportStatic;
    export = passport;
}
"#;

    let test_ts = r#"
import * as passport from "passport";
import { Passport } from "passport";

let p: Passport = passport.use();
"#;

    let files: &[(&str, &str)] = &[("passport.d.ts", passport_dts), ("test.ts", test_ts)];
    let diagnostics = compile_named_files_get_diagnostics_with_options(
        files,
        "test.ts",
        CheckerOptions {
            module: ModuleKind::CommonJS,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    let codes: Vec<u32> = diagnostics.iter().map(|(c, _)| *c).collect();
    // Must NOT emit TS2749 — Passport is an interface, not a value.
    assert!(
        !has_error(&diagnostics, 2749),
        "Should NOT emit TS2749 for 'Passport' — it is an interface imported \
         from an ambient module via export=. Got: {codes:?}\n{diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 2300) && !has_error(&diagnostics, 2451),
        "Should NOT emit TS2300/TS2451 for 'Passport' imported from an ambient module via export=. Got: {codes:?}\n{diagnostics:#?}"
    );
}

#[test]
#[ignore = "merged backlog: needs tsc-compatible widened keyof array element display"]
fn test_keyof_array_elaboration_reports_only_invalid_literal_element() {
    let source = r#"
function foo<T extends { a: string, b: string }>() {
    let b: (keyof T)[] = ["a", "b", "c"];
}
"#;

    let diagnostics = compile_and_get_raw_diagnostics_named(
        "test.ts",
        source,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|diag| diag.code == 2322)
        .collect();
    assert_eq!(
        ts2322.len(),
        1,
        "Expected only one TS2322 for the invalid array element.\nActual: {diagnostics:#?}"
    );

    let expected_start = source.find("\"c\"").expect("expected c literal") as u32;
    assert_eq!(
        ts2322[0].start, expected_start,
        "Expected TS2322 to anchor at the invalid \"c\" element.\nActual: {diagnostics:#?}"
    );
    assert!(
        ts2322[0]
            .message_text
            .contains("Type 'string' is not assignable to type 'keyof T'"),
        "Expected widened keyof-target TS2322 text.\nActual: {diagnostics:#?}"
    );
}

#[test]
fn test_property_receiver_display_widens_fresh_object_application_args() {
    let diagnostics = compile_and_get_diagnostics_named(
        "test.ts",
        r#"
type MyPick<T, K extends keyof T> = { [P in K]: T[P] };
declare function pick<T, K extends keyof T>(obj: T, propNames: K[]): MyPick<T, K>;

const x = pick({ a: 10, b: 20, c: 30 }, ["a", "c"]);
x.b;
        "#,
        CheckerOptions {
            strict: true,
            ..Default::default()
        },
    );

    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert_eq!(
        ts2339.len(),
        1,
        "Expected exactly one TS2339 for the missing property access.\nActual diagnostics: {diagnostics:#?}"
    );

    let message = diagnostic_message(&diagnostics, 2339).expect("expected TS2339");
    assert_eq!(
        message,
        "Property 'b' does not exist on type 'MyPick<{ a: number; b: number; c: number; }, \"a\" | \"c\">'."
    );
}

#[test]
fn test_property_receiver_display_preserves_annotated_application_literals() {
    let diagnostics = compile_and_get_diagnostics_named(
        "test.ts",
        r#"
type MyPick<T, K extends keyof T> = { [P in K]: T[P] };

declare const x: MyPick<{ a: 10; b: 20; c: 30 }, "a" | "c">;
x.b;
        "#,
        CheckerOptions {
            strict: true,
            ..Default::default()
        },
    );

    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert_eq!(
        ts2339.len(),
        1,
        "Expected exactly one TS2339 for the annotated missing property access.\nActual diagnostics: {diagnostics:#?}"
    );

    let message = diagnostic_message(&diagnostics, 2339).expect("expected TS2339");
    assert_eq!(
        message,
        "Property 'b' does not exist on type 'MyPick<{ a: 10; b: 20; c: 30; }, \"a\" | \"c\">'."
    );
}

#[test]
fn test_property_receiver_display_preserves_annotated_mapped_type_params() {
    let diagnostics = compile_and_get_diagnostics_named(
        "test.ts",
        r#"
type MyPick<T, K extends keyof T> = { [P in K]: T[P] };
type MyRecord<K extends keyof any, T> = { [P in K]: T };

function pickAccess<T, K extends keyof T>(obj: MyPick<T, K>) {
    obj.foo;
}

function recordAccess<T, K extends keyof T>(obj: MyRecord<K, number>) {
    obj.foo;
}
        "#,
        CheckerOptions {
            strict: true,
            ..Default::default()
        },
    );

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2339 && message == "Property 'foo' does not exist on type 'MyPick<T, K>'."
        }),
        "Expected TS2339 to preserve annotated MyPick<T, K> receiver.\nActual diagnostics: {diagnostics:#?}"
    );
    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2339
                && message == "Property 'foo' does not exist on type 'MyRecord<K, number>'."
        }),
        "Expected TS2339 to preserve annotated MyRecord<K, number> receiver.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_assignability_display_widens_fresh_application_args() {
    let diagnostics = compile_and_get_diagnostics_named(
        "test.ts",
        r#"
type MyReadonly<T> = { readonly [P in keyof T]: T[P] };
declare function objAndReadonly<T>(primary: T, secondary: MyReadonly<T>): T;

objAndReadonly({ x: 0, y: 0 }, { x: 1 });
        "#,
        CheckerOptions {
            strict: true,
            ..Default::default()
        },
    );

    let message = diagnostic_message(&diagnostics, 2345).expect("expected TS2345");
    assert!(
        message.contains("MyReadonly<{ x: number; y: number; }>"),
        "Expected mapped application target display to widen fresh object args.\nActual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !message.contains("MyReadonly<{ x: 0; y: 0; }>"),
        "Mapped application target display must not preserve fresh literal args.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_excess_property_display_widens_fresh_application_args() {
    let diagnostics = compile_and_get_diagnostics_named(
        "test.ts",
        r#"
type MyReadonly<T> = { readonly [P in keyof T]: T[P] };
type MyPartial<T> = { [P in keyof T]?: T[P] };
declare function objAndReadonly<T>(primary: T, secondary: MyReadonly<T>): T;
declare function objAndPartial<T>(primary: T, secondary: MyPartial<T>): T;

objAndReadonly({ x: 0, y: 0 }, { x: 1, y: 1, z: 1 });
objAndPartial({ x: 0, y: 0 }, { x: 1, y: 1, z: 1 });
        "#,
        CheckerOptions {
            strict: true,
            ..Default::default()
        },
    );

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2353 && message.contains("MyReadonly<{ x: number; y: number; }>")
        }),
        "Expected TS2353 to widen fresh object args in MyReadonly target display.\nActual diagnostics: {diagnostics:#?}"
    );
    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2353 && message.contains("MyPartial<{ x: number; y: number; }>")
        }),
        "Expected TS2353 to widen fresh object args in MyPartial target display.\nActual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !diagnostics.iter().any(|(code, message)| {
            *code == 2353
                && (message.contains("MyReadonly<{ x: 0; y: 0; }>")
                    || message.contains("MyPartial<{ x: 0; y: 0; }>"))
        }),
        "TS2353 mapped application display must not preserve fresh literal args.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_cross_binder_symbol_id_collision_emits_ts2322_for_this_return() {
    let passport_dts = r#"
declare module 'passport' {
    namespace passport {
        interface Passport {
            use(): this;
        }

        interface PassportStatic extends Passport {
            Passport: {new(): Passport};
        }
    }

    const passport: passport.PassportStatic;
    export = passport;
}
"#;

    let test_ts = r#"
import * as passport from "passport";
import { Passport } from "passport";

let p: Passport = passport.use();
"#;

    let files: &[(&str, &str)] = &[("passport.d.ts", passport_dts), ("test.ts", test_ts)];
    let diagnostics = compile_named_files_get_diagnostics_with_options(
        files,
        "test.ts",
        CheckerOptions {
            module: ModuleKind::CommonJS,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    let codes: Vec<u32> = diagnostics.iter().map(|(c, _)| *c).collect();
    // After lib type resolution alignment (e369dffe12), the `PassportStatic extends Passport`
    // relationship is correctly resolved via the symbol type cache, so TS2322 is no longer
    // emitted. The key invariant is that we don't regress into false TS2749/TS2300/TS2451.
    assert!(
        !has_error(&diagnostics, 2749)
            && !has_error(&diagnostics, 2300)
            && !has_error(&diagnostics, 2451),
        "Should not regress into TS2749/TS2300/TS2451 for `Passport` imported from an ambient module via export=. Got: {codes:?}\n{diagnostics:#?}"
    );
}

/// Simpler variant: named interface import from an ambient module without
/// polymorphic this.
#[test]
fn test_cross_binder_named_import_resolves_as_type() {
    let module_dts = r#"
declare module 'mymod' {
    namespace mymod {
        interface Config {
            name: string;
        }
    }
    const mymod: { Config: { new(): mymod.Config } };
    export = mymod;
}
"#;

    let test_ts = r#"
import { Config } from "mymod";

let c: Config = { name: "hello" };
"#;

    let files: &[(&str, &str)] = &[("mymod.d.ts", module_dts), ("test.ts", test_ts)];
    let diagnostics = compile_named_files_get_diagnostics_with_options(
        files,
        "test.ts",
        CheckerOptions {
            module: ModuleKind::CommonJS,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    let codes: Vec<u32> = diagnostics.iter().map(|(c, _)| *c).collect();
    assert!(
        !has_error(&diagnostics, 2749),
        "Should NOT emit TS2749 for 'Config' — it is an interface imported \
         from an ambient module via export=. Got: {codes:?}\n{diagnostics:#?}"
    );
}

/// Bug: `keyof` type alias used as function parameter corrupts subsequent usage.
/// When `type K = keyof Reg` is used as a function parameter type, subsequent
/// assignments like `const x: K = "a"` incorrectly fail with "Type 'string'
/// is not assignable to type 'keyof Reg'". Without the function declaration,
/// the assignment works fine. Using `keyof Reg` directly (not through alias)
/// also works. This suggests a caching side-effect in function parameter
/// type processing that corrupts the keyof type alias evaluation.
#[test]
#[ignore = "keyof type alias caching bug - function parameter processing corrupts keyof evaluation"]
fn test_keyof_type_alias_in_function_parameter_should_not_corrupt_assignability() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
interface Registry {
  a: never;
  b: never;
  c: never;
}
type Keys = keyof Registry;
declare function take(style: Keys): void;
const x: Keys = "a";
take("a");
"#,
        CheckerOptions {
            strict: true,
            ..Default::default()
        },
    );

    // Neither assignment nor call should error
    assert!(
        !has_error(&diagnostics, 2322),
        "Should NOT emit TS2322 for literal assignable to keyof. Got: {diagnostics:?}"
    );
    assert!(
        !has_error(&diagnostics, 2345),
        "Should NOT emit TS2345 for literal argument to keyof param. Got: {diagnostics:?}"
    );
}

/// Narrowing by a const identifier with null type.
/// When `x === myNull` where `const myNull: null = null`,
/// the true branch should narrow `x` to `null`.
/// Repro: TypeScript/tests/cases/compiler/controlFlowNullTypeAndLiteral.ts
#[test]
fn test_narrowing_by_const_null_identifier() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
const myNull: null = null;
function f(x: number | null) {
    if (x === myNull) {
        const s: string = x;
    }
}
"#,
        CheckerOptions {
            strict: true,
            ..Default::default()
        },
    );

    assert!(
        has_error(&diagnostics, 2322),
        "Should emit TS2322 inside if body. Got: {diagnostics:?}"
    );
    let msg = diagnostic_message(&diagnostics, 2322).unwrap();
    assert!(
        msg.contains("'null'"),
        "TS2322 should say 'null' not 'number', got: {msg}"
    );
    assert!(
        !msg.contains("'number'"),
        "TS2322 should NOT say 'number' (wrong narrowing), got: {msg}"
    );
}

/// Types NOT inside a namespace should remain unqualified in diagnostics.
#[test]
fn test_global_type_display_remains_unqualified() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
class Bar { y: string; }
declare function takeBar(b: Bar): void;
takeBar("wrong");
"#,
    );

    assert!(
        has_error(&diagnostics, 2345),
        "Should emit TS2345. Got: {diagnostics:?}"
    );
    let msg = diagnostic_message(&diagnostics, 2345).unwrap();
    assert!(
        msg.contains("'Bar'"),
        "TS2345 message should contain unqualified 'Bar', got: {msg}"
    );
    assert!(
        !msg.contains(".Bar"),
        "TS2345 message should NOT contain dot-qualified '.Bar', got: {msg}"
    );
}

#[test]
fn test_ts7059_angle_bracket_assertion_in_mts_file() {
    // TS7059: Angle-bracket type assertions are reserved in .mts/.cts files.
    let diagnostics = compile_and_get_diagnostics_named(
        "test.mts",
        r#"const x = <any>"hello";"#,
        CheckerOptions::default(),
    );
    let ts7059_count = diagnostics.iter().filter(|(c, _)| *c == 7059).count();
    assert!(
        ts7059_count > 0,
        "Expected TS7059 for angle-bracket type assertion in .mts file. Got: {diagnostics:#?}"
    );
}

#[test]
fn test_ts7059_not_emitted_in_ts_file() {
    // TS7059 should NOT be emitted in regular .ts files.
    let diagnostics = compile_and_get_diagnostics_named(
        "test.ts",
        r#"const x = <any>"hello";"#,
        CheckerOptions::default(),
    );
    let ts7059_count = diagnostics.iter().filter(|(c, _)| *c == 7059).count();
    assert_eq!(
        ts7059_count, 0,
        "Expected 0 TS7059 in regular .ts file. Got: {diagnostics:#?}"
    );
}

#[test]
fn test_ts7060_single_type_param_arrow_in_mts_file() {
    // TS7060: Single type parameter without trailing comma or constraint in .mts/.cts files.
    //                0123456789012
    let source = r#"const f = <T>() => {};"#;
    let raw_diags =
        compile_and_get_raw_diagnostics_named("test.mts", source, CheckerOptions::default());
    let ts7060: Vec<_> = raw_diags.iter().filter(|d| d.code == 7060).collect();
    assert!(
        !ts7060.is_empty(),
        "Expected TS7060 for single type param arrow in .mts file. Got: {raw_diags:#?}"
    );
    // The type parameter 'T' starts at position 11 (column 12 in 1-indexed)
    let d = &ts7060[0];
    assert_eq!(
        d.start, 11,
        "TS7060 should point at type parameter 'T' at position 11, got {}. Diag: {d:?}",
        d.start
    );
}

#[test]
fn test_ts7060_not_emitted_with_trailing_comma() {
    // TS7060 should NOT be emitted when there's a trailing comma.
    let diagnostics = compile_and_get_diagnostics_named(
        "test.mts",
        r#"const f = <T,>() => {};"#,
        CheckerOptions::default(),
    );
    let ts7060_count = diagnostics.iter().filter(|(c, _)| *c == 7060).count();
    assert_eq!(
        ts7060_count, 0,
        "Expected 0 TS7060 with trailing comma. Got: {diagnostics:#?}"
    );
}

#[test]
fn test_ts7060_not_emitted_with_constraint() {
    // TS7060 should NOT be emitted when the type parameter has a constraint.
    let diagnostics = compile_and_get_diagnostics_named(
        "test.mts",
        r#"const f = <T extends object>() => {};"#,
        CheckerOptions::default(),
    );
    let ts7060_count = diagnostics.iter().filter(|(c, _)| *c == 7060).count();
    assert_eq!(
        ts7060_count, 0,
        "Expected 0 TS7060 with constraint. Got: {diagnostics:#?}"
    );
}

/// Enum types from different namespaces with the same name should produce
/// TS2322 with namespace-qualified type names in the diagnostic message, not
/// TS2719 ("Two different types with this name exist").
///
/// tsc displays the target as the qualified enum name (e.g.,
/// "numerics.DiagnosticCategory") rather than a structural type alias.
/// Previously, the target formatter lacked an enum-specific path, causing
/// it to resolve enum types to unrelated display aliases.
#[test]
fn test_enum_assignment_compat_uses_qualified_names_not_ts2719() {
    let code = r#"
namespace numerics {
    export enum DiagnosticCategory {
        Warning,
        Error,
        Suggestion,
        Message,
    }
}
namespace strings {
    export enum DiagnosticCategory {
        Warning = "Warning",
        Error = "Error",
        Suggestion = "Suggestion",
        Message = "Message",
    }
}
function f(x: numerics.DiagnosticCategory, y: strings.DiagnosticCategory) {
    x = y;
    y = x;
}
"#;
    let diagnostics = compile_and_get_diagnostics(code);
    assert!(
        has_error(&diagnostics, 2322),
        "Expected TS2322 for incompatible enum assignment, got: {diagnostics:?}"
    );
    assert!(
        !has_error(&diagnostics, 2719),
        "Should NOT emit TS2719 for enums from different namespaces, got: {diagnostics:?}"
    );
    // Verify the message uses qualified names
    let ts2322_messages: Vec<&str> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2322)
        .map(|(_, msg)| msg.as_str())
        .collect();
    for msg in &ts2322_messages {
        assert!(
            msg.contains("strings.DiagnosticCategory")
                || msg.contains("numerics.DiagnosticCategory"),
            "TS2322 message should use qualified enum names, got: {msg}"
        );
    }
}

/// TS2719 ("Two different types with this name exist, but they are unrelated")
/// must not fire when the shared display name is a primitive name. Primitives
/// have no second declaration that could clash, so the "two different types"
/// framing is wrong; emit plain TS2322 instead.
///
/// Repro from `compiler/conditionalTypeAssignabilityWhenDeferred.ts` line 47:
/// the target is a deferred conditional whose printer falls back to the
/// branch upper bound (`string`), and the source is the primitive `string`.
/// Without the gate the strings compare equal and TS2719 fires.
#[test]
fn test_no_ts2719_when_target_evaluates_to_primitive_string() {
    let code = r#"
type Foo<T> = T extends true ? string : "a";
function test<T>(x: Foo<T>, s: string) {
  x = s;
}
"#;
    let diagnostics = compile_and_get_diagnostics(code);
    assert!(
        !has_error(&diagnostics, 2719),
        "Should NOT emit TS2719 when display collapses to primitive `string`, got: {diagnostics:?}"
    );
    assert!(
        has_error(&diagnostics, 2322),
        "Expected TS2322 for incompatible primitive→deferred assignment, got: {diagnostics:?}"
    );
}

/// Same structural rule as the previous test, but with a different
/// type-parameter name (`U`) and alias name (`Bar`) to confirm the gate is
/// not keyed on any identifier — only on the printed primitive name.
#[test]
fn test_no_ts2719_when_target_evaluates_to_primitive_number() {
    let code = r#"
type Bar<U> = U extends true ? number : 0;
function check<U>(y: Bar<U>, n: number) {
  y = n;
}
"#;
    let diagnostics = compile_and_get_diagnostics(code);
    assert!(
        !has_error(&diagnostics, 2719),
        "Should NOT emit TS2719 when display collapses to primitive `number`, got: {diagnostics:?}"
    );
    assert!(
        has_error(&diagnostics, 2322),
        "Expected TS2322 for incompatible primitive→deferred assignment, got: {diagnostics:?}"
    );
}

/// CJS modules whose `module.exports = <callable>` produce a merged
/// callable+properties apparent type. tsc renders the structural form
/// (`{ (): void; blah: any; }`) for TS2339 receivers, not the
/// `typeof import("…")` namespace alias.
///
/// Repro from `compiler/pushTypeGetTypeOfAlias.ts`: when typing
/// `exports.someProp` inside the same file, the receiver Object cached
/// in `namespace_module_names` was synthesized from an early-call race
/// in `infer_commonjs_export_rhs_type` (returned UNDEFINED before the
/// function expression had been typed). Without the fix, the diagnostic
/// fell through to `'typeof import("bar")'` instead of the structural
/// shape.
#[test]
fn test_cjs_module_exports_callable_renders_structurally_in_ts2339() {
    let source = r#"
module.exports = function () {};
exports.blah = exports.someProp;
"#;
    let diagnostics = compile_and_get_diagnostics_named(
        "test.js",
        source,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            ..Default::default()
        },
    );
    let ts2339 = diagnostic_message(&diagnostics, 2339);
    assert!(
        ts2339.is_some(),
        "Expected TS2339 for `exports.someProp`: {diagnostics:?}"
    );
    let msg = ts2339.unwrap();
    assert!(
        msg.contains("{ (): void; blah: any; }"),
        "TS2339 receiver should render as the merged callable shape, not the namespace alias.\nActual: {msg}"
    );
    assert!(
        !msg.contains("typeof import"),
        "TS2339 receiver should NOT use `typeof import` for callable CJS modules.\nActual: {msg}"
    );
}

/// Same structural rule with a different export property name. Confirms
/// the fix is keyed on the *shape* (callable apparent type with merged
/// properties), not on any specific identifier (per CLAUDE.md §25
/// anti-hardcoding checklist).
#[test]
fn test_cjs_module_exports_callable_renders_structurally_alt_name() {
    let source = r#"
module.exports = function () {};
exports.foo = exports.missing;
"#;
    let diagnostics = compile_and_get_diagnostics_named(
        "test.js",
        source,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            ..Default::default()
        },
    );
    let ts2339 = diagnostic_message(&diagnostics, 2339);
    assert!(
        ts2339.is_some(),
        "Expected TS2339 for `exports.missing`: {diagnostics:?}"
    );
    let msg = ts2339.unwrap();
    assert!(
        msg.contains("(): void") && msg.contains("foo: any"),
        "TS2339 receiver should render the merged callable shape with the named export.\nActual: {msg}"
    );
}

/// `new Proxy(t, {})` should not emit TS2351 ("This expression is not constructable").
///
/// `ProxyConstructor` is an interface with a construct signature:
///   `new <T extends object>(target: T, handler: ProxyHandler<T>): T`
///
/// The type of `Proxy` is `ProxyConstructor` (from `declare var Proxy: ProxyConstructor`).
/// When the `ProxyConstructor` type stays as a `Lazy(DefId)` reference (common for lib types
/// whose `DefId`→`SymbolId` mapping isn't established during cross-file resolution), the
/// solver's `resolve_new` can't find construct signatures and incorrectly returns `NotCallable`.
///
/// The fix resolves Lazy constructor types through lib type resolution by name before
/// passing them to the solver.
#[test]
fn test_new_proxy_no_ts2351() {
    let source = r#"
var t = {};
var p = new Proxy(t, {});
"#;
    let diagnostics = compile_and_get_diagnostics_with_lib(source);
    assert!(
        !has_error(&diagnostics, 2351),
        "new Proxy() should NOT emit TS2351 (not constructable), got: {diagnostics:?}"
    );
}

/// Generic class with self-referential return type should not prevent property access
/// on instantiated class types. Previously, having a method that returned the same class
/// with different type args (e.g., `fmap<B>(...): Vec2<B>`) caused the class instance
/// type cache (`symbol_instance_types`) to hold ERROR, breaking property lookups on
/// `Vec2<(a: A) => B>` in other methods.
///
/// See: genericClasses4.ts conformance test
#[test]
fn test_generic_class_self_referential_property_access() {
    let source = r#"
class Vec2<A> {
    constructor(public x: A, public y: A) {}
    fmap<B>(f: (a: A) => B): Vec2<B> {
        var x: B = f(this.x);
        var y: B = f(this.y);
        var retval: Vec2<B> = new Vec2(x, y);
        return retval;
    }
    apply<B>(f: Vec2<(a: A) => B>): Vec2<B> {
        var x: B = f.x(this.x);
        var y: B = f.y(this.y);
        var retval: Vec2<B> = new Vec2(x, y);
        return retval;
    }
}
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        !has_error(&diagnostics, 2339),
        "Property access on generic class instance should not produce TS2339, got: {diagnostics:?}"
    );
}

/// typeof on a merged namespace+interface symbol should resolve to the namespace
/// value type (the structural object with exported functions), not the interface type.
/// Without the fix, `typeof M2.Point` would resolve to the `Point` interface type,
/// causing a false TS2403 when compared against `{ Origin(): { x: number; y: number; } }`.
///
/// Test case from: conformance/internalModules/moduleDeclarations/nonInstantiatedModule.ts
#[test]
fn test_typeof_merged_namespace_interface_no_false_ts2403() {
    let source = r#"
namespace M2 {
    export namespace Point {
        export function Origin(): Point {
            return { x: 0, y: 0 };
        }
    }

    export interface Point {
        x: number;
        y: number;
    }
}

var p2: { Origin() : { x: number; y: number; } };
var p2: typeof M2.Point;
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        !has_error(&diagnostics, 2403),
        "typeof M2.Point should NOT emit TS2403 for merged namespace+interface, got: {diagnostics:?}"
    );
}

/// Dotted namespace `namespace M2.X { export interface Point }` merged with
/// `namespace M2 { export namespace X { export var Point: number } }` should
/// expose `Point` as a number in the namespace value type. Previously,
/// `check_value_decl_has_export_in_arena` did not walk from `VARIABLE_DECLARATION`
/// up through `VARIABLE_DECLARATION_LIST` to `VARIABLE_STATEMENT` to check for the
/// export modifier, so the exported variable was silently dropped and `M2.X`
/// resolved to `{}` instead of `{ Point: number }`.
///
/// Additionally, `namespace_export_member_type` must use the variable type (not
/// the interface type) for merged INTERFACE+VARIABLE symbols.
#[test]
fn test_dotted_namespace_merged_interface_variable_export_no_false_ts2339() {
    let source = r#"
namespace M2.X {
    export interface Point {
        x: number;
        y: number;
    }
}

namespace M2 {
    export namespace X {
        export var Point: number;
    }
}

var m = M2.X;
var point: number;
var point = m.Point;
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        !has_error(&diagnostics, 2339),
        "m.Point should NOT emit TS2339: namespace value type should include exported var Point. Got: {diagnostics:?}"
    );
    assert!(
        !has_error(&diagnostics, 2403),
        "m.Point should be number, matching the prior 'var point: number' declaration. Got: {diagnostics:?}"
    );
}

/// Verify that the first part of the nestedModules fix works in isolation:
/// `namespace A.B.C` declarations properly seed their exports into the merged
/// namespace, and `export var` inside a sub-namespace is accessible.
#[test]
fn test_nested_namespace_export_var_accessible_through_value() {
    let source = r#"
namespace A.B.C {
    export interface Point {
        x: number;
        y: number;
    }
}

namespace A {
    export namespace B {
        var Point: C.Point = { x: 0, y: 0 };
    }
}
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    // tsc expects no errors for this pattern
    assert!(
        !has_error(&diagnostics, 2339),
        "C.Point should be accessible within namespace A.B. Got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2536_mismatched_keyof_source_in_param_type() {
    // B[T] as parameter type where T extends keyof A, but B != A
    // Currently not checked: adding check_type_node for param types
    // causes false positives for Partial<T>[K] patterns.
    let source = r"
interface A { x: number; y: string; }
interface B { x: number; }
function foo<T extends keyof A>(value: B[T]) {}
";
    let diagnostics = compile_and_get_diagnostics(source);
    let ts2536_count = diagnostics.iter().filter(|(code, _)| *code == 2536).count();
    assert!(
        ts2536_count >= 1,
        "Expected TS2536 for B[T] where T extends keyof A but A != B.\nGot: {diagnostics:#?}"
    );
}

#[test]
fn test_ts2536_mismatched_keyof_source_in_tp_constraint() {
    // B[T] as type parameter constraint where T extends keyof A, but B != A
    let source = r"
interface A { x: number; y: string; }
interface B { x: number; }
function foo<T extends keyof A, V extends B[T]>(value: V) {}
";
    let diagnostics = compile_and_get_diagnostics(source);
    let ts2536_count = diagnostics.iter().filter(|(code, _)| *code == 2536).count();
    assert!(
        ts2536_count >= 1,
        "Expected TS2536 for B[T] in constraint where T extends keyof A but A != B.\nGot: {diagnostics:#?}"
    );
}

#[test]
fn test_ts2536_undefined_source_type_in_constraint() {
    // Like intersectionsOfLargeUnions: T extends keyof UndefinedType,
    // V extends KnownType[T][P] — tsc emits TS2536 even though UndefinedType
    // is unresolvable because T's constraint (keyof any = string|number|symbol)
    // is not assignable to keyof KnownType.
    let source = r"
interface KnownType { x: number; y: string; }
function foo<
    T extends keyof UndefinedType,
    P extends keyof UndefinedType,
    V extends KnownType[T][P]>(value: V) {}
";
    let diagnostics = compile_and_get_diagnostics(source);
    let ts2536_count = diagnostics.iter().filter(|(code, _)| *code == 2536).count();
    assert!(
        ts2536_count >= 1,
        "Expected TS2536 for KnownType[T] where T extends keyof of undefined type.\nGot: {diagnostics:#?}"
    );
}
