use super::super::core::*;

#[test]
fn test_constructor_only_object_signatures_with_mixed_subtype_direction_do_not_overlap() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
class Base {
    a!: string;
}

class Derived extends Base {
    b!: string;
}

declare let a6: { new (a: Derived, b: Base): Base };
declare let b6: { new (a: Base, b: Derived): Base };

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

    let ts2365 = relevant_diagnostics
        .iter()
        .filter(|(code, _)| *code == 2365)
        .count();
    let ts2367 = relevant_diagnostics
        .iter()
        .filter(|(code, _)| *code == 2367)
        .count();

    assert_eq!(
        ts2365, 2,
        "Mixed-direction constructor subtype relations should emit TS2365 for relational comparisons. Actual errors: {relevant_diagnostics:#?}"
    );
    assert_eq!(
        ts2367, 2,
        "Mixed-direction constructor subtype relations should emit TS2367 for equality comparisons. Actual errors: {relevant_diagnostics:#?}"
    );
}

/// Issue: Computed property destructuring produces false TS2349
///
/// From: computed-property-destructuring.md
/// Expected: No TS2349 errors
/// Actual: TS2349 "This expression is not callable" errors
///
/// Root cause: Computed property name expression in destructuring binding
/// may be incorrectly treated or the type resolution fails.
#[test]
fn test_computed_property_destructuring_no_false_ts2349() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
let foo = "bar";
let {[foo]: bar} = {bar: "baz"};
        "#,
    );

    // Filter out TS2318 (missing global types)
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    assert!(
        !has_error(&relevant, 2349),
        "Should NOT emit TS2349 for computed property destructuring.\nActual errors: {relevant:#?}"
    );
}

/// TS2538 for computed property keys with `any` type in destructuring assignments.
///
/// When a computed key in a destructuring ASSIGNMENT pattern has type `any`
/// (e.g., from calling a non-callable or invalid arithmetic), tsc emits TS2538
/// "Type 'any' cannot be used as an index type." Previously we only emitted
/// TS2538 for variable declaration destructuring, not assignment destructuring.
#[test]
fn test_ts2538_computed_key_any_type_in_destructuring_assignment() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
let foo = "bar";
let bar4: any;
[{[foo()]: bar4}] = [{bar: "bar"}];
        "#,
    );

    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    assert!(
        has_error(&relevant, 2538),
        "Should emit TS2538 for `any`-typed computed key in destructuring assignment.\nActual errors: {relevant:#?}"
    );
}

/// Binary arithmetic with invalid operand types should produce `any` result type.
///
/// When `+` is applied to incompatible types (e.g., `1 + {}`), tsc returns
/// `any` as the expression result type. This ensures downstream checks like
/// TS2538 see `any` rather than a misleading `number`.
#[test]
fn test_binary_plus_invalid_operand_produces_any_result() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
let bar4: any;
[{[(1 + {})]: bar4}] = [{bar: "bar"}];
        "#,
    );

    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    assert!(
        has_error(&relevant, 2365),
        "Should emit TS2365 for invalid `1 + {{}}`.\nActual errors: {relevant:#?}"
    );
    assert!(
        has_error(&relevant, 2538),
        "Should emit TS2538 for the any index result of invalid `1 + {{}}`.\nActual errors: {relevant:#?}"
    );
}

/// Issue: Contextual typing for generic function parameters
///
/// From: contextual-typing-generics.md
/// Expected: No TS7006 errors (parameter gets contextual type from generic function type)
/// Actual: TS7006 "Parameter implicitly has 'any' type"
///
/// Root cause: When a function expression/arrow is assigned to a generic function type
/// like `<T>(x: T) => void`, the parameter should get its type from contextual typing.
/// Currently, the parameter type is not inferred from the contextual type.
#[test]
fn test_contextual_typing_generic_function_param() {
    // Enable noImplicitAny to trigger TS7006
    let source = r"
// @noImplicitAny: true
const fn2: <T>(x: T) => void = function test(t) { };
    ";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions {
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );

    checker.check_source_file(root);

    let diagnostics: Vec<(u32, String)> = checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect();

    // Filter out TS2318 (missing global types)
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    assert!(
        !has_error(&relevant, 7006),
        "Should NOT emit TS7006 - parameter 't' should be contextually typed as T.\nActual errors: {relevant:#?}"
    );
}

/// Issue: Contextual typing for arrow function assigned to generic type
#[test]
fn test_contextual_typing_generic_arrow_param() {
    let source = r"
// @noImplicitAny: true
declare function f(fun: <T>(t: T) => void): void;
f(t => { });
    ";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions {
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );

    checker.check_source_file(root);

    let diagnostics: Vec<(u32, String)> = checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect();

    // Filter out TS2318 (missing global types)
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    assert!(
        !has_error(&relevant, 7006),
        "Should NOT emit TS7006 - parameter 't' should be contextually typed from generic.\nActual errors: {relevant:#?}"
    );
}

/// Issue: nested tuple callbacks under object-literal properties can leak provisional TS7006
///
/// From: mappedTypeRecursiveInference2.ts
/// Expected: No TS7006 errors
/// Actual: TS7006 on the nested tuple callback inside the object-literal property
#[test]
fn test_contextual_typing_nested_object_literal_tuple_callback() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
type MorphTuple = [string, "|>", any]

type validateMorph<def extends MorphTuple> = def[1] extends "|>"
    ? [validateDefinition<def[0]>, "|>", (In: def[0]) => unknown]
    : def

type validateDefinition<def> = def extends MorphTuple
    ? validateMorph<def>
    : {
          [k in keyof def]: validateDefinition<def[k]>
      }

declare function type<def>(def: validateDefinition<def>): def

const objectLiteral = type({ a: ["ark", "|>", (x) => x.length] })
        "#,
        CheckerOptions {
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
    );

    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    assert!(
        !has_error(&relevant, 7006),
        "Should NOT emit TS7006 for the nested tuple callback inside the object-literal property.\
         \nActual errors: {relevant:#?}"
    );
}

/// Issue: false-positive assignability errors with contextual generic outer type parameters.
///
/// Mirrors: contextualOuterTypeParameters.ts
/// Expected: no TS2322/TS2345 errors
#[test]
fn test_contextual_outer_type_parameters_no_false_assignability_errors() {
    let source = r"
declare function f(fun: <T>(t: T) => void): void

f(t => {
    type isArray = (typeof t)[] extends string[] ? true : false;
    type IsObject = { x: typeof t } extends { x: string } ? true : false;
});

const fn1: <T>(x: T) => void = t => {
    type isArray = (typeof t)[] extends string[] ? true : false;
    type IsObject = { x: typeof t } extends { x: string } ? true : false;
};

const fn2: <T>(x: T) => void = function test(t) {
    type isArray = (typeof t)[] extends string[] ? true : false;
    type IsObject = { x: typeof t } extends { x: string } ? true : false;
};
";

    let options = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(source, options);

    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    assert!(
        !has_error(&relevant, 2322),
        "Should NOT emit TS2322 for contextual generic outer type parameters.\nActual errors: {relevant:#?}"
    );
    assert!(
        !has_error(&relevant, 2345),
        "Should NOT emit TS2345 for contextual generic outer type parameters.\nActual errors: {relevant:#?}"
    );
}

/// Issue: false-positive TS2345 in contextual signature instantiation chain.
///
/// Mirrors: contextualSignatureInstantiation2.ts
/// Expected: no TS2345
#[test]
fn test_contextual_signature_instantiation_chain_no_false_ts2345() {
    let diagnostics = compile_and_get_diagnostics(
        r"
var dot: <T, S>(f: (_: T) => S) => <U>(g: (_: U) => T) => (_: U) => S;
dot = <T, S>(f: (_: T) => S) => <U>(g: (_: U) => T): (r:U) => S => (x) => f(g(x));
var id: <T>(x:T) => T;
var r23 = dot(id)(id);
        ",
    );

    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    assert!(
        !has_error(&relevant, 2345),
        "Should NOT emit TS2345 for contextual signature instantiation chain.\nActual errors: {relevant:#?}"
    );
}

#[test]
fn test_recursive_type_relations_object_keys_reduce_reports_ts2345() {
    let source = r#"
type ClassNameObject = { [key: string]: boolean | undefined };
declare function reduceClassNameObject(
    cb: (obj: ClassNameObject, key: string) => ClassNameObject,
): void;

export function css<S extends { [K in keyof S]: string }>(styles: S): string {
  reduceClassNameObject((obj: ClassNameObject, key: keyof S) => {
    const exportedClassName = styles[key];
    obj[exportedClassName] = true;
    return obj;
  });
  return "";
}
"#;
    let diagnostics = compile_and_get_diagnostics(source);

    let ts2345 = diagnostics
        .iter()
        .find(|(code, message)| {
            *code == 2345
                && message.contains(
                    "Argument of type '(obj: ClassNameObject, key: keyof S) => ClassNameObject'",
                )
        })
        .cloned();

    assert!(
        ts2345.is_some(),
        "Expected TS2345 for generic callback parameter mismatch with keyof S parameter. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_settimeout_callback_assignable_to_function_union() {
    let diagnostics = compile_and_get_diagnostics(
        r"
setTimeout(() => 1, 0);
        ",
    );

    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    assert!(
        !has_error(&relevant, 2345),
        "Should NOT emit TS2345 for setTimeout callback assignability.\nActual errors: {relevant:#?}"
    );
}

#[test]
fn test_typed_array_constructor_accepts_number_array() {
    let diagnostics = compile_and_get_diagnostics(
        r"
function makeTyped(obj: number[]) {
    var typedArrays = [];
    typedArrays[0] = new Int8Array(obj);
    return typedArrays;
}
        ",
    );

    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    assert!(
        !has_error(&relevant, 2769),
        "Should NOT emit TS2769 for Int8Array(number[]).\nActual errors: {relevant:#?}"
    );
}

/// Regression test: TS7006 SHOULD still fire for closures without any contextual type
#[test]
fn test_ts7006_still_fires_without_contextual_type() {
    let source = r"
// @noImplicitAny: true
var f = function(x) { };
    ";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions {
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );

    checker.check_source_file(root);

    let diagnostics: Vec<(u32, String)> = checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect();

    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    assert!(
        has_error(&relevant, 7006),
        "SHOULD emit TS7006 - parameter 'x' has no contextual type.\nActual errors: {relevant:#?}"
    );
}

/// Issue: Contextual typing for mapped type generic parameters
///
/// When a generic function has a mapped type parameter like `{ [K in keyof P]: P[K] }`,
/// and P has a constraint (e.g. `P extends Props`), the lambda parameters inside the
/// object literal argument should be contextually typed from the constraint.
///
/// For example:
/// ```typescript
/// interface Props { when: (value: string) => boolean; }
/// function good2<P extends Props>(attrs: { [K in keyof P]: P[K] }) { }
/// good2({ when: value => false }); // `value` should be typed as `string`
/// ```
///
/// Root cause was two-fold:
/// 1. During two-pass generic inference, when all args are context-sensitive,
///    type parameters had no candidates. Fixed by using upper bounds (constraints)
///    in `get_current_substitution` instead of UNKNOWN.
/// 2. The instantiated mapped type contained Lazy references that the solver's
///    `NoopResolver` couldn't resolve. Fixed by evaluating the contextual type
///    with the checker's Judge (which has the full `TypeEnvironment` resolver)
///    before extracting property types.
#[test]
fn test_contextual_typing_mapped_type_generic_param() {
    let source = r"
// @noImplicitAny: true
interface Props {
    when: (value: string) => boolean;
}
function good2<P extends Props>(attrs: { [K in keyof P]: P[K] }) { }
good2({ when: value => false });
    ";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions {
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );

    checker.check_source_file(root);

    let diagnostics: Vec<(u32, String)> = checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect();

    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    // tsc does not emit TS7006 here — the callback parameter `value` gets its type
    // from contextual typing through the mapped type generic param. Our fix to
    // track implicit-any-checked closures prevents the false positive on re-entry.
    assert!(
        !has_error(&relevant, 7006),
        "Should NOT emit TS7006 for 'value' — contextual typing resolves it.\
         \nActual errors: {relevant:#?}"
    );
}

/// Issue: TS2344 reported twice for the same type argument
///
/// When `get_type_from_type_node` re-resolves a type reference (e.g., because
/// `type_parameter_scope` changes between type environment building and statement
/// checking), `validate_type_reference_type_arguments` was called twice for the
/// same node, producing duplicate TS2344 errors.
///
/// Fix: Use `emitted_diagnostics` deduplication in `error_type_constraint_not_satisfied`
/// to prevent emitting the same TS2344 at the same source position twice.
#[test]
fn test_ts2344_no_duplicate_errors() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
interface Array<T> {}
interface Boolean {}
interface Function {}
interface IArguments {}
interface Number {}
interface Object {}
interface RegExp {}
interface String {}

function one<T extends string>() {}
one<number>();

function two<T extends object>() {}
two<string>();

function three<T extends { value: string }>() {}
three<number>();
        ",
        CheckerOptions {
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
    );

    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    // Count TS2344 errors - each should appear exactly once
    let ts2344_count = relevant.iter().filter(|(code, _)| *code == 2344).count();
    assert_eq!(
        ts2344_count, 3,
        "Should emit exactly 3 TS2344 errors (one per bad type arg), not duplicates.\nActual errors: {relevant:#?}"
    );
}

/// TS2339: Property access on `this` in static methods should use constructor type
///
/// In static methods, `this` refers to `typeof C` (the constructor type), not an
/// instance of C. Accessing instance properties on `this` in a static method should
/// emit TS2339 because instance properties don't exist on the constructor type.
#[test]
fn test_ts2339_this_in_static_method() {
    let diagnostics = compile_and_get_diagnostics(
        r"
class C {
    public p = 0;
    static s = 0;
    static b() {
        this.p = 1; // TS2339 - 'p' is instance, doesn't exist on typeof C
        this.s = 2; // OK - 's' is static
    }
}
        ",
    );

    let ts2339_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert_eq!(
        ts2339_errors.len(),
        1,
        "Should emit exactly 1 TS2339 for 'this.p' in static method.\nActual errors: {diagnostics:#?}"
    );
    assert!(
        ts2339_errors[0].1.contains("'p'") || ts2339_errors[0].1.contains("\"p\""),
        "TS2339 should mention property 'p'. Got: {}",
        ts2339_errors[0].1
    );
}

#[test]
fn test_interface_accessor_declarations() {
    // Interface accessor declarations (get/set) should be recognized as properties
    let diagnostics = compile_and_get_diagnostics(
        r"
interface Test {
    get foo(): string;
    set foo(s: string | number);
}
const t = {} as Test;
let m: string = t.foo;   // OK - getter returns string
        ",
    );

    let ts2339_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert_eq!(
        ts2339_errors.len(),
        0,
        "Interface accessors should be recognized as properties. Got TS2339 errors: {ts2339_errors:#?}"
    );
}

#[test]
fn test_type_literal_accessor_declarations() {
    // Type literal accessor declarations (get/set) should be recognized as properties
    let diagnostics = compile_and_get_diagnostics(
        r"
type Test = {
    get foo(): string;
    set foo(s: number);
};
const t = {} as Test;
let m: string = t.foo;   // OK - getter returns string
        ",
    );

    let ts2339_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert_eq!(
        ts2339_errors.len(),
        0,
        "Type literal accessors should be recognized as properties. Got TS2339 errors: {ts2339_errors:#?}"
    );
}

/// Issue: False-positive TS2345 when interface extends another and adds call signatures
///
/// From: addMoreCallSignaturesToBaseSignature2.ts
/// Expected: No errors - `a(1)` should match inherited `(bar: number): string` signature
/// Actual: TS2345 (falsely claims argument type mismatch)
///
/// When interface Bar extends Foo (which has `(bar: number): string`),
/// and Bar adds `(key: string): string`, calling `a(1)` with a numeric
/// argument should match the inherited signature without error.
#[test]
fn test_interface_inherited_call_signature_no_false_ts2345() {
    let diagnostics = compile_and_get_diagnostics(
        r"
interface Foo {
    (bar:number): string;
}

interface Bar extends Foo {
    (key: string): string;
}

var a: Bar;
var kitty = a(1);
        ",
    );

    // Filter out TS2318 (missing global types)
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    assert!(
        !has_error(&relevant, 2345),
        "Should NOT emit TS2345 - a(1) should match inherited (bar: number) => string.\nActual errors: {relevant:#?}"
    );
}

/// Issue: False-positive TS2345 with mixin pattern (class extends function return)
///
/// From: anonClassDeclarationEmitIsAnon.ts
/// Expected: No errors - `Timestamped(User)` should work as a valid base class
/// Actual: TS2345 (falsely claims User is not assignable to Constructor parameter)
///
/// The mixin pattern `function Timestamped<TBase extends Constructor>(Base: TBase)`
/// with `Constructor<T = {}> = new (...args: any[]) => T` should accept any class.
#[test]
fn test_mixin_pattern_no_false_ts2345() {
    let diagnostics = compile_and_get_diagnostics(
        r"
type Constructor<T = {}> = new (...args: any[]) => T;

function Timestamped<TBase extends Constructor>(Base: TBase) {
    return class extends Base {
        timestamp = 0;
    };
}

class User {
    name = '';
}

class TimestampedUser extends Timestamped(User) {
    constructor() {
        super();
    }
}
        ",
    );

    // Filter out TS2318 (missing global types)
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    assert!(
        !has_error(&relevant, 2345),
        "Should NOT emit TS2345 - User should be assignable to Constructor<{{}}>.\nActual errors: {relevant:#?}"
    );
}

/// Issue: Contextual typing for method shorthand fails when parameter type is a union
///
/// When a function parameter is `Opts | undefined`, the contextual type should still
/// flow through to object literal method parameters. TypeScript filters out non-object
/// types from unions when computing contextual types for object literals.
#[test]
fn test_contextual_typing_union_with_undefined() {
    let opts = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    }
    .apply_strict_defaults();
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
interface Opts {
    fn(x: number): void;
}

declare function a(opts: Opts | undefined): void;
a({ fn(x) {} });
        ",
        opts,
    );

    assert!(
        !has_error(&diagnostics, 7006),
        "Should NOT emit TS7006 - 'x' should be contextually typed as number from Opts.fn.\nActual errors: {diagnostics:#?}"
    );
}

/// Issue: Contextual typing for property assignment fails when parameter type is a union
#[test]
fn test_contextual_typing_property_in_union_with_null() {
    let opts = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    }
    .apply_strict_defaults();
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
interface Opts {
    callback: (x: number) => void;
}

declare function b(opts: Opts | null): void;
b({ callback: (x) => {} });
        ",
        opts,
    );

    assert!(
        !has_error(&diagnostics, 7006),
        "Should NOT emit TS7006 - 'x' should be contextually typed as number from Opts.callback.\nActual errors: {diagnostics:#?}"
    );
}

#[test]
fn test_optional_function_property_in_union_with_primitive_does_not_contextually_type_callback() {
    let diagnostics = without_missing_global_type_errors(compile_and_get_diagnostics_with_options(
        r#"
type Validate = (text: string, pos: number, self: Rule) => number | boolean;
interface FullRule {
    validate: string | RegExp | Validate;
    normalize?: (match: {x: string}) => void;
}

type Rule = string | FullRule;

const obj: {field: Rule} = {
    field: {
        validate: (_t, _p, _s) => false,
        normalize: match => match.x,
    }
};
        "#,
        CheckerOptions {
            no_implicit_any: true,
            strict: true,
            target: ScriptTarget::ESNext,
            ..CheckerOptions::default()
        },
    ));

    assert!(
        has_error(&diagnostics, 7006),
        "Expected TS7006 when optional callback property comes from a primitive-containing union.\nActual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 2322),
        "Should NOT emit outer TS2322 when the callback loses contextual typing from a primitive-containing union.\nActual diagnostics: {diagnostics:#?}"
    );
}

// TS7022: Variable implicitly has type 'any' because it does not have a type annotation
// and is referenced directly or indirectly in its own initializer.

/// TS7022 should fire for direct self-referencing object literals under noImplicitAny.
/// From: recursiveObjectLiteral.ts
#[test]
fn test_ts7022_recursive_object_literal() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
var a = { f: a };
        ",
        opts,
    );
    assert!(
        has_error(&diagnostics, 7022),
        "Should emit TS7022 for self-referencing object literal.\nActual errors: {diagnostics:#?}"
    );
}

#[test]
fn test_ts7022_emitted_for_self_referential_default_parameter() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        target: ScriptTarget::ES2015,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
function f(yield = yield) {
}
        ",
        opts,
    );

    assert!(
        has_error(&diagnostics, 2372),
        "Should emit TS2372 for the self-referential default parameter.\nActual errors: {diagnostics:#?}"
    );
    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 7022 && message.contains("'yield'") && message.contains("its own initializer")
        }),
        "Should emit TS7022 for the self-referential default parameter under noImplicitAny.\nActual errors: {diagnostics:#?}"
    );
}

#[test]
fn test_ts7022_emitted_for_default_export_self_import_initializer() {
    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &[(
            "QSpinner.js",
            r#"
import DefaultSpinner from './QSpinner'

export default {
  mixins: [DefaultSpinner],
  name: 'QSpinner'
}
"#,
        )],
        "QSpinner.js",
        CheckerOptions {
            module: tsz_common::common::ModuleKind::CommonJS,
            target: ScriptTarget::ES2015,
            strict: true,
            allow_js: true,
            check_js: true,
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        has_error(&diagnostics, 7022),
        "Should emit TS7022 for a default export that self-imports through its own initializer.\nActual errors: {diagnostics:#?}"
    );
    assert!(
        diagnostic_message(&diagnostics, 7022)
            .is_some_and(|message| message.contains("'default' implicitly has type 'any'")),
        "Expected TS7022 to point at the synthetic default export symbol.\nActual errors: {diagnostics:#?}"
    );
}

#[test]
fn test_ts7022_emitted_for_circular_class_field_initializers() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r##"
class A {
    #foo = this.#bar;
    #bar = this.#foo;
    ["#baz"] = this["#baz"];
}
        "##,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            strict: true,
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        diagnostics
            .iter()
            .any(|(code, message)| *code == 7022
                && message.contains("'#foo' implicitly has type 'any'")),
        "Expected TS7022 for circular private field '#foo'.\nActual errors: {diagnostics:#?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|(code, message)| *code == 7022
                && message.contains("'#bar' implicitly has type 'any'")),
        "Expected TS7022 for circular private field '#bar'.\nActual errors: {diagnostics:#?}"
    );
    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 7022 && message.contains("'[\"#baz\"]' implicitly has type 'any'")
        }),
        "Expected TS7022 for computed class field '[\"#baz\"]'.\nActual errors: {diagnostics:#?}"
    );
}

#[test]
fn test_ts7022_emitted_for_destructured_parameter_capture_without_context() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
function foo({
    value1,
    test1 = value1.test1,
    test2 = value1.test2
}) {}
        "#,
        CheckerOptions {
            strict: true,
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 7022 && message.contains("'value1' implicitly has type 'any'")
        }),
        "Expected TS7022 for destructured parameter binding captured by sibling defaults.\nActual errors: {diagnostics:#?}"
    );
}
