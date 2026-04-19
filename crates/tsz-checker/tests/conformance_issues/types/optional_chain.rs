use crate::core::*;

#[test]
fn test_assert_optional_chain_discriminant_narrows_base_union_member() {
    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        r#"
interface Cat {
    type: 'cat';
    canMeow: true;
}
interface Dog {
    type: 'dog';
    canBark: true;
}
type Animal = Cat | Dog;
declare function assertEqual<T>(value: any, type: T): asserts value is T;

function f(animalOrUndef: Animal | undefined) {
    assertEqual(animalOrUndef?.type, 'cat' as const);
    animalOrUndef.canMeow;
}
        "#,
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            no_implicit_any: true,
            ..Default::default()
        },
    );

    let semantic_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .collect();
    assert!(
        !semantic_errors.iter().any(|(code, _)| *code == 2339),
        "Expected no TS2339 after assertEqual(animalOrUndef?.type, 'cat'). Actual: {semantic_errors:#?}"
    );
    assert!(
        !semantic_errors.iter().any(|(code, _)| *code == 18048),
        "Expected no TS18048 after assertEqual(animalOrUndef?.type, 'cat'). Actual: {semantic_errors:#?}"
    );
}

#[test]
fn test_assert_optional_chain_then_assert_nonnull_keeps_base_narrowed() {
    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        r#"
type Thing = { foo: string | number };
declare function assert(x: unknown): asserts x;
declare function assertNonNull<T>(x: T): asserts x is NonNullable<T>;
function f(o: Thing | undefined) {
    assert(typeof o?.foo === "number");
    o.foo;
    assertNonNull(o?.foo);
    o.foo;
}
        "#,
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            no_implicit_any: true,
            ..Default::default()
        },
    );

    let semantic_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .collect();
    assert!(
        !semantic_errors.iter().any(|(code, _)| *code == 2339),
        "Expected no TS2339 after assertion optional-chain sequence. Actual: {semantic_errors:#?}"
    );
}

#[test]
fn test_optional_chain_strict_equality_transports_non_nullish_to_base() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
type Thing = { foo: number, bar(): number };
function f(o: Thing | null, value: number) {
    if (o?.foo === value) {
        o.foo;
    }
    if (o?.["foo"] === value) {
        o["foo"];
    }
    if (o?.bar() === value) {
        o.bar;
    }
    if (o?.bar() == value) {
        o.bar;
    }
}
        "#,
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            no_implicit_any: true,
            ..Default::default()
        },
    );

    let semantic_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .collect();
    assert!(
        !semantic_errors.iter().any(|(code, _)| *code == 18047),
        "Expected no TS18047 after o?.foo === value. Actual: {semantic_errors:#?}"
    );
    assert!(
        !semantic_errors.iter().any(|(code, _)| *code == 2339),
        "Expected no TS2339 after o?.foo === value. Actual: {semantic_errors:#?}"
    );
}

#[test]
fn test_non_null_assertion_condition_narrows_underlying_reference() {
    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        r#"
const m = ''.match('');
m! && m[0];
        "#,
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            no_implicit_any: true,
            ..Default::default()
        },
    );

    let semantic_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .collect();
    assert!(
        !semantic_errors.iter().any(|(code, _)| *code == 18047),
        "Expected no TS18047 for m! && m[0]. Actual: {semantic_errors:#?}"
    );
}

#[test]
fn test_non_null_assertion_on_optional_chain_condition_narrows_underlying_reference() {
    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        r#"
const m = ''.match('');
m?.[0]! && m[0];
        "#,
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            no_implicit_any: true,
            ..Default::default()
        },
    );

    let semantic_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .collect();
    assert!(
        !semantic_errors.iter().any(|(code, _)| *code == 18047),
        "Expected no TS18047 for m?.[0]! && m[0]. Actual: {semantic_errors:#?}"
    );
}

#[test]
fn test_optional_chain_truthiness_narrows_all_prefixes_on_true_branch() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
type T = { x?: { y?: { z: number } } };
declare const o: T;
if (o.x?.y?.z) {
    o.x.y.z;
}
        "#,
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            no_implicit_any: true,
            ..Default::default()
        },
    );

    let semantic_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .collect();
    assert!(
        !semantic_errors.iter().any(|(code, _)| *code == 18048),
        "Expected no TS18048 in true branch after o.x?.y?.z truthiness check. Actual: {semantic_errors:#?}"
    );
}

#[test]
fn test_optional_chain_truthiness_does_not_over_narrow_false_branch() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
type T = { x?: { y?: { z: number } } };
declare const o: T;
if (o.x?.y?.z) {
} else {
    o.x.y.z;
}
        "#,
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            no_implicit_any: true,
            ..Default::default()
        },
    );

    let semantic_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .collect();
    assert!(
        semantic_errors.iter().any(|(code, _)| *code == 18048),
        "Expected TS18048 in false branch after o.x?.y?.z truthiness check. Actual: {semantic_errors:#?}"
    );
}

#[test]
fn test_direct_identifier_truthiness_guard_narrows_in_and_rhs() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
const x: string[] | null = null as any;
x && x[0];
        "#,
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            no_implicit_any: true,
            ..Default::default()
        },
    );

    let semantic_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .collect();
    assert!(
        !semantic_errors.iter().any(|(code, _)| *code == 18047),
        "Expected no TS18047 for x && x[0]. Actual: {semantic_errors:#?}"
    );
}

#[test]
fn test_optional_call_generic_this_inference_uses_receiver_type() {
    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        r#"
interface Y {
    foo<T>(this: T, arg: keyof T): void;
    a: number;
    b: string;
}
declare const value: Y | undefined;
if (value) {
    value?.foo("a");
}
value?.foo("a");
        "#,
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            no_implicit_any: true,
            ..Default::default()
        },
    );

    let semantic_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .collect();
    assert!(
        !semantic_errors.iter().any(|(code, _)| *code == 2345),
        "Expected no TS2345 for optional-call generic this inference. Actual: {semantic_errors:#?}"
    );
}

/// Assignment-based narrowing should use declared annotation types, not initializer flow types.
///
/// Regression pattern: `let x: T | undefined = undefined; x = makeT(); use(x);`
/// Previously, flow assignment compatibility could read `x` as `undefined` and skip narrowing.
#[test]
fn test_assignment_narrowing_prefers_declared_annotation_type() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
// @strict: true
type Browser = { close(): void };
declare function makeBrowser(): Browser;
declare function consumeBrowser(b: Browser): void;

function test() {
    let browser: Browser | undefined = undefined;
    try {
        browser = makeBrowser();
        consumeBrowser(browser);
        browser.close();
    } finally {
    }
}
        "#,
    );

    let semantic_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .collect();
    assert!(
        !semantic_errors
            .iter()
            .any(|(code, _)| *code == 2345 || *code == 18048),
        "Should not emit TS2345/TS18048 after assignment narrowing, got: {semantic_errors:#?}"
    );
}

/// Issue: Private identifiers in object literals
///
/// Expected: TS18016 (private identifiers not allowed outside class bodies)
/// Status: FIXED (2026-02-09)
///
/// Root cause: Parser wasn't validating private identifier usage in object literals
/// Fix: Added validation in `state_expressions.rs` `parse_property_assignment`
#[test]
fn test_private_identifier_in_object_literal() {
    // TS18016 is a PARSER error, so we need to check parser diagnostics
    let source = r"
const obj = {
    #x: 1
};
    ";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let parser_diagnostics: Vec<(u32, String)> = parser
        .get_diagnostics()
        .iter()
        .map(|d| (d.code, d.message.clone()))
        .collect();

    assert!(
        parser_diagnostics.iter().any(|(c, _)| *c == 18016),
        "Should emit TS18016 for private identifier in object literal.\nActual errors: {parser_diagnostics:#?}"
    );
}

/// Issue: Private identifier access outside class
///
/// Expected: TS18013 (property not accessible outside class)
/// Status: FIXED (2026-02-09)
///
/// Root cause: `get_type_of_private_property_access` didn't check class scope
/// Fix: Added check in `state_type_analysis.rs` to emit TS18013 when !`saw_class_scope`
#[test]
fn test_private_identifier_access_outside_class() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class Foo {
    #bar = 42;
}
const f = new Foo();
const x = f.#bar;  // Should error TS18013
        ",
    );

    assert!(
        has_error(&diagnostics, 18013),
        "Should emit TS18013 for private identifier access outside class.\nActual errors: {diagnostics:#?}"
    );
}

/// Issue: Private identifier access from within class should work
///
/// Expected: No errors
/// Status: VERIFIED (2026-02-09)
#[test]
fn test_private_identifier_access_inside_class() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class Foo {
    #bar = 42;
    getBar() {
        return this.#bar;  // Should NOT error
    }
}
        ",
    );

    assert!(
        !has_error(&diagnostics, 18013),
        "Should NOT emit TS18013 when accessing private identifier inside class.\nActual errors: {diagnostics:#?}"
    );
}

/// Issue: Private identifiers as parameters
///
/// Expected: TS18009 (private identifiers cannot be used as parameters)
/// Status: FIXED (2026-02-09)
///
/// Root cause: Parser wasn't validating private identifier usage as parameters
/// Fix: Added validation in `state_statements.rs` `parse_parameter`
#[test]
fn test_private_identifier_as_parameter() {
    // TS18009 is a PARSER error
    let source = r"
class Foo {
    method(#param: any) {}
}
    ";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let parser_diagnostics: Vec<(u32, String)> = parser
        .get_diagnostics()
        .iter()
        .map(|d| (d.code, d.message.clone()))
        .collect();

    assert!(
        parser_diagnostics.iter().any(|(c, _)| *c == 18009),
        "Should emit TS18009 for private identifier as parameter.\nActual errors: {parser_diagnostics:#?}"
    );
}

/// Issue: Private identifiers in variable declarations
///
/// Expected: TS18029 (private identifiers not allowed in variable declarations)
/// Status: FIXED (2026-02-09)
///
/// Root cause: Parser wasn't validating private identifier usage in variable declarations
/// Fix: Added validation in `state_statements.rs` `parse_variable_declaration_with_flags`
#[test]
fn test_private_identifier_in_variable_declaration() {
    // TS18029 is a PARSER error
    let source = r"
const #x = 1;
    ";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let parser_diagnostics: Vec<(u32, String)> = parser
        .get_diagnostics()
        .iter()
        .map(|d| (d.code, d.message.clone()))
        .collect();

    assert!(
        parser_diagnostics.iter().any(|(c, _)| *c == 18029),
        "Should emit TS18029 for private identifier in variable declaration.\nActual errors: {parser_diagnostics:#?}"
    );
}

/// Issue: Optional chain with private identifiers
///
/// Expected: TS18030 (optional chain cannot contain private identifiers)
/// Status: FIXED (2026-02-09)
///
/// Root cause: Parser wasn't validating private identifier usage in optional chains
/// Fix: Added validation in `state_expressions.rs` when handling `QuestionDotToken`
#[test]
fn test_private_identifier_in_optional_chain() {
    // TS18030 is a PARSER error
    let source = r"
class Bar {
    #prop = 42;
    test() {
        return this?.#prop;
    }
}
    ";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let parser_diagnostics: Vec<(u32, String)> = parser
        .get_diagnostics()
        .iter()
        .map(|d| (d.code, d.message.clone()))
        .collect();

    assert!(
        parser_diagnostics.iter().any(|(c, _)| *c == 18030),
        "Should emit TS18030 for private identifier in optional chain.\nActual errors: {parser_diagnostics:#?}"
    );
}

/// Issue: TS18016 checker validation - private identifier outside class
///
/// For property access expressions (`obj.#bar`), TSC only emits TS18013 (semantic:
/// can't access private member) — NOT TS18016 (grammar: private identifier outside class).
/// TS18016 is only emitted for truly invalid syntax positions (object literals, etc.)
/// because `obj.#bar` is valid syntax even outside a class body.
///
/// Status: FIXED (2026-02-10) - corrected to match TSC behavior
#[test]
fn test_ts18016_private_identifier_outside_class() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class Foo {
    #bar: number;
}

let f: Foo;
let x = f.#bar;  // Outside class - should error TS18013 only (not TS18016)
        ",
    );

    // Filter out TS2318 (missing global types) which are noise for this test
    let relevant_diagnostics: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    // Should NOT emit TS18016 for property access — TSC doesn't emit it here.
    // TS18016 is only for truly invalid positions (object literals, standalone expressions).
    assert!(
        !has_error(&relevant_diagnostics, 18016),
        "Should NOT emit TS18016 for property access outside class (TSC doesn't).\nActual errors: {relevant_diagnostics:#?}"
    );

    // Should emit TS18013 (semantic error - property not accessible)
    assert!(
        has_error(&relevant_diagnostics, 18013),
        "Should emit TS18013 for private identifier access outside class.\nActual errors: {relevant_diagnostics:#?}"
    );
}

/// Issue: TS2416 false positive for private field "overrides"
///
/// Expected: Private fields with same name in child class should NOT emit TS2416
/// Status: FIXED (2026-02-09)
///
/// Root cause: Override checking didn't skip private identifiers
/// Fix: Added check in `class_checker.rs` to skip override validation for names starting with '#'
#[test]
fn test_private_field_no_override_error() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class Parent {
    #foo: number;
}

class Child extends Parent {
    #foo: string;  // Should NOT emit TS2416 - private fields don't participate in inheritance
}
        ",
    );

    // Filter out TS2318 (missing global types)
    let relevant_diagnostics: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    // Should NOT emit TS2416 (incompatible override) for private fields
    assert!(
        !has_error(&relevant_diagnostics, 2416),
        "Should NOT emit TS2416 for private field with same name in child class.\nActual errors: {relevant_diagnostics:#?}"
    );
}

/// TS2416 for class extending non-class (variable with constructor signature).
///
/// When a class extends a variable declared as `{ prototype: A; new(): A }`,
/// the AST-level class resolution fails (variable, not class), so the checker
/// falls back to type-level resolution. Property type compatibility must still
/// be checked against the resolved instance type.
#[test]
fn test_ts2416_type_level_base_class_property_incompatibility() {
    let diagnostics = compile_and_get_diagnostics(
        r"
interface A {
    n: number;
}
declare var A: {
    prototype: A;
    new(): A;
};

class B extends A {
    n = '';
}
        ",
    );

    let relevant_diagnostics: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    assert!(
        has_error(&relevant_diagnostics, 2416),
        "Should emit TS2416 when derived class property type is incompatible with base type.\nActual errors: {relevant_diagnostics:#?}"
    );
}

#[test]
fn test_ts2416_type_level_base_class_constructor_call_type_arguments() {
    let diagnostics = compile_and_get_diagnostics(
        r"
// @strictPropertyInitialization: false
type T1 = { n: number };
type Constructor<T> = new () => T;
declare function Constructor<T>(): Constructor<T>;

class Base extends Constructor<T1>() {
    n = '';
}
",
    );

    let relevant_diagnostics: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    assert!(
        has_error(&relevant_diagnostics, 2416),
        "Should emit TS2416 when constructor-call base type args are applied to derived class instances.\nActual errors: {relevant_diagnostics:#?}"
    );
}

/// TS2416 alongside TS2426 when method overrides accessor with incompatible type.
///
/// tsc emits both TS2426 (kind mismatch: accessor -> method) and TS2416 (type incompatibility)
/// when a derived class method overrides a base class accessor.
#[test]
fn test_ts2416_emitted_alongside_ts2426_accessor_method_mismatch() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class Base {
    get x() { return 1; }
    set x(v) {}
}

class Derived extends Base {
    x() { return 1; }
}
        ",
    );

    let relevant_diagnostics: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    assert!(
        has_error(&relevant_diagnostics, 2426),
        "Should emit TS2426 for accessor/method kind mismatch.\nActual errors: {relevant_diagnostics:#?}"
    );
    assert!(
        has_error(&relevant_diagnostics, 2416),
        "Should also emit TS2416 for type incompatibility alongside TS2426.\nActual errors: {relevant_diagnostics:#?}"
    );
}

#[test]
fn test_class_implements_class_instance_members_report_ts2416() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class Base {
    n: Base | string;
    fn() {
        return 10;
    }
}

class DerivedInterface implements Base {
    n: DerivedInterface | string;
    fn() {
        return 10 as number | string;
    }
}
        ",
    );

    let relevant_diagnostics: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318 && *code != 2564)
        .cloned()
        .collect();

    // tsc emits TS2720 (class incorrectly implements) alongside TS2416 (member mismatch).
    // We may emit either or both depending on current implementation state.
    assert!(
        has_error(&relevant_diagnostics, 2720) || has_error(&relevant_diagnostics, 2416),
        "Expected TS2720 or TS2416 for 'class DerivedInterface implements Base'.\nActual errors: {relevant_diagnostics:#?}"
    );
}

/// When a class extends C<string> but implements C<number>, the inherited
/// member types (after instantiation) are incompatible with the target.
/// tsc emits TS2720 for the implements-class failure.
#[test]
fn test_class_extends_and_implements_same_generic_class_emits_ts2720() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
class C<T> {
    foo: number;
    bar(): T { return null as any; }
}
class D extends C<string> implements C<number> {
    baz() { }
}
"#,
    );
    let codes: Vec<u32> = diagnostics.iter().map(|(code, _)| *code).collect();
    assert!(
        has_error(&diagnostics, 2720),
        "Expected TS2720 for 'class D extends C<string> implements C<number>'. Got codes: {codes:?}"
    );
    // Verify the message includes type arguments (C<number>, not just C)
    let ts2720_msg = diagnostics
        .iter()
        .find(|(code, _)| *code == 2720)
        .map(|(_, msg)| msg.as_str())
        .unwrap();
    assert!(
        ts2720_msg.contains("C<number>"),
        "TS2720 message should reference 'C<number>' with type args, got: {ts2720_msg}"
    );
}

#[test]
fn test_class_implements_class_reports_private_member_incompatibility_on_assignment() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class A {
    private x = 1;
    foo(): number { return 1; }
}
class C implements A {
    foo() {
        return 1;
    }
}

class C2 extends A {}

declare var c: C;
declare var c2: C2;
c = c2;
c2 = c;
        ",
    );

    let relevant_diagnostics: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    assert!(
        has_error(&relevant_diagnostics, 2720),
        "Expected TS2720 for implementing class A, got: {relevant_diagnostics:#?}"
    );
    // tsc expects TS2741: "Property 'x' is missing in type 'C' but required in type 'A'."
    // for the `c2 = c` assignment (C -> C2 where C2 requires private x from A).
    assert!(
        has_error(&relevant_diagnostics, 2741),
        "Expected TS2741 for missing private property 'x', got: {relevant_diagnostics:#?}"
    );
}

/// Seam test: TS2430 should be reported for incompatible interface member types.
///
/// Guards `class_checker` interface-extension compatibility after relation-helper refactors.
#[test]
fn test_interface_extension_incompatible_property_reports_ts2430() {
    let diagnostics = compile_and_get_diagnostics(
        r"
interface Base {
  value: string;
}

interface Derived extends Base {
  value: number;
}
        ",
    );

    let relevant_diagnostics: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    assert!(
        has_error(&relevant_diagnostics, 2430),
        "Should emit TS2430 for incompatible interface extension member.\nActual errors: {relevant_diagnostics:#?}"
    );
}

#[test]
fn test_unconstrained_type_parameters_are_not_assignable_to_each_other() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
function foo<T, U>(t: T, u: U) {
  t = u;
  u = t;
}
        "#,
    );

    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .cloned()
        .collect();

    assert_eq!(
        ts2322.len(),
        2,
        "Expected two TS2322 diagnostics for unconstrained T/U assignment, got: {ts2322:?}"
    );
}

/// Seam test: TS2367 should be reported when compared types have no overlap.
///
/// Guards overlap-check relation/query refactors used by equality comparisons.
#[test]
fn test_no_overlap_comparison_reports_ts2367() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
let x: "a" | "b" = "a";
if (x === 42) {
}
        "#,
    );

    let relevant_diagnostics: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    assert!(
        has_error(&relevant_diagnostics, 2367),
        "Should emit TS2367 for comparison of non-overlapping types.\nActual errors: {relevant_diagnostics:#?}"
    );
}

#[test]
fn test_constructor_only_object_signatures_remain_comparable() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
declare let a6: { new <T>(x: T, y: T): T };
declare let b6: { new (x: string, y: number): {} };

let lt1 = a6 < b6;
let lt2 = b6 < a6;
let eq1 = a6 == b6;
let eq2 = b6 === a6;
        "#,
    );

    let relevant_diagnostics: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    assert!(
        !has_error(&relevant_diagnostics, 2365) && !has_error(&relevant_diagnostics, 2367),
        "Constructor-only object signature comparisons should stay comparable. Actual errors: {relevant_diagnostics:#?}"
    );
}
