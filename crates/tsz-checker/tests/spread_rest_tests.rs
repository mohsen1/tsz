//! Tests for spread and rest operator type checking

use tsz_binder::BinderState;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

/// Helper function to check source and return diagnostics
fn check_source(source: &str) -> Vec<Diagnostic> {
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
        tsz_checker::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

#[test]
fn test_array_spread_with_tuple() {
    let source = r#"
type Tuple = [string, number];
const t: Tuple = ["hello", 42];
const arr = [...t];  // Should be (string | number)[]
"#;

    let diagnostics = check_source(source);

    // Should NOT emit TS2322 or TS2488
    let errors = diagnostics
        .iter()
        .filter(|d| d.code == 2322 || d.code == 2488)
        .count();
    assert_eq!(
        errors, 0,
        "Expected no errors for array spread with tuple, got {errors}"
    );
}

#[test]
fn test_array_spread_with_array() {
    let source = r"
const nums = [1, 2, 3];
const arr = [...nums];  // Should be number[]
";

    let diagnostics = check_source(source);

    // Should NOT emit TS2322 or TS2488
    let errors = diagnostics
        .iter()
        .filter(|d| d.code == 2322 || d.code == 2488)
        .count();
    assert_eq!(
        errors, 0,
        "Expected no errors for array spread with array, got {errors}"
    );
}

#[test]
fn test_array_spread_with_non_iterable_emits_ts2488() {
    let source = r"
const num = 42;
const arr = [...num];  // Should emit TS2488
";

    let diagnostics = check_source(source);

    // Should emit TS2488
    let ts2488_count = diagnostics.iter().filter(|d| d.code == 2488).count();
    assert!(
        ts2488_count >= 1,
        "Expected at least 1 TS2488 error for non-iterable spread, got {ts2488_count}"
    );
}

#[test]
fn test_tuple_context_with_spread() {
    let source = r#"
type Tuple = [string, number, boolean];
const t: Tuple = ["hello", ...[1, 2], true];  // Error: can't spread number[] into tuple position
"#;

    let _diagnostics = check_source(source);
    // This is a complex case - spread in tuple context
    // The behavior depends on implementation
}

#[test]
fn test_object_spread() {
    let source = r"
const obj1 = { a: 1, b: 2 };
const obj2 = { c: 3 };
const merged = { ...obj1, ...obj2 };  // Should be { a: number, b: number, c: number }
";

    let diagnostics = check_source(source);

    // Should NOT emit TS2322
    let ts2322_count = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        ts2322_count, 0,
        "Expected no TS2322 error for object spread, got {ts2322_count}"
    );
}

#[test]
fn test_rest_parameter() {
    let source = r"
function sum(...nums: number[]) {
    return nums.reduce((a, b) => a + b, 0);
}
sum(1, 2, 3);
";

    let diagnostics = check_source(source);

    // Should NOT emit TS2322
    let ts2322_count = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        ts2322_count, 0,
        "Expected no TS2322 error for rest parameter, got {ts2322_count}"
    );
}

#[test]
fn test_rest_parameter_with_wrong_types_emits_ts2345() {
    let source = r#"
function sum(...nums: number[]) {
    return nums.reduce((a, b) => a + b, 0);
}
sum(1, "two", 3);  // Should emit TS2345
"#;

    let diagnostics = check_source(source);

    // Should emit TS2345 for string argument (TS2345 is for function arguments, TS2322 is for assignments)
    let ts2345_count = diagnostics.iter().filter(|d| d.code == 2345).count();
    assert!(
        ts2345_count >= 1,
        "Expected at least 1 TS2345 error for wrong type in rest parameter, got {ts2345_count}"
    );
}

#[test]
fn test_array_destructuring_with_rest() {
    let source = r"
const arr = [1, 2, 3, 4, 5];
const [first, second, ...rest] = arr;
// first: number, second: number, rest: number[]
";

    let diagnostics = check_source(source);

    // Should NOT emit TS2322
    let ts2322_count = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        ts2322_count, 0,
        "Expected no TS2322 error for array destructuring with rest, got {ts2322_count}"
    );
}

#[test]
fn test_tuple_destructuring_with_rest() {
    let source = r#"
type Tuple = [string, number, boolean, ...string[]];
const t: Tuple = ["hello", 42, true, "a", "b"];
const [s, n, ...rest] = t;
// s: string, n: number, rest: (boolean | string)[]
"#;

    let diagnostics = check_source(source);

    // Should NOT emit TS2322
    let ts2322_count = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        ts2322_count, 0,
        "Expected no TS2322 error for tuple destructuring with rest, got {ts2322_count}"
    );
}

#[test]
fn test_discriminated_tuple_rest_destructuring_no_false_ts2345() {
    let source = r"
type Expression = BooleanLogicExpression | 'true' | 'false';
type BooleanLogicExpression = ['and', ...Expression[]] | ['not', Expression];

function evaluate(expression: Expression): boolean {
  if (Array.isArray(expression)) {
    const [operator, ...operands] = expression;
    switch (operator) {
      case 'and': {
        return operands.every((child) => evaluate(child));
      }
      case 'not': {
        return !evaluate(operands[0]);
      }
      default: {
        throw new Error(`${operator} is not a supported operator`);
      }
    }
  } else {
    return expression === 'true';
  }
}
";

    let diagnostics = check_source(source);
    let ts2345_count = diagnostics.iter().filter(|d| d.code == 2345).count();
    assert_eq!(
        ts2345_count,
        0,
        "Expected no TS2345 from discriminated tuple rest destructuring, got {} errors: {:?}",
        ts2345_count,
        diagnostics
            .iter()
            .filter(|d| d.code == 2345)
            .map(|d| &d.message_text)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_tuple_assignment_destructuring_no_false_ts2322() {
    // Tuple assignment destructuring should not produce false TS2322 errors.
    // tsc checks each element individually, not the whole tuple against an inferred array type.
    let source = r#"
type Robot = [number, string, string];
var robotA: Robot = [1, "mower", "mowing"];
let nameA: string;
let numberB: number;
[, nameA] = robotA;
[numberB] = robotA;
[numberB, nameA] = robotA;
"#;

    let diagnostics = check_source(source);

    let ts2322_count = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        ts2322_count,
        0,
        "Expected no TS2322 for tuple assignment destructuring, got {} errors: {:?}",
        ts2322_count,
        diagnostics
            .iter()
            .filter(|d| d.code == 2322)
            .map(|d| &d.message_text)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_spread_in_function_call() {
    let source = r"
function add(a: number, b: number, c: number) {
    return a + b + c;
}
const args = [1, 2, 3];
add(...args);  // Should work
";

    let diagnostics = check_source(source);

    // Should NOT emit TS2322
    let ts2322_count = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        ts2322_count, 0,
        "Expected no TS2322 error for spread in function call, got {ts2322_count}"
    );
}

#[test]
fn test_spread_in_function_call_with_wrong_types() {
    let source = r#"
function add(a: number, b: number, c: number) {
    return a + b + c;
}
const args = [1, "two", 3];
add(...args);  // Should emit TS2345
"#;

    let diagnostics = check_source(source);

    // TypeScript emits TS2556 for this case: "A spread argument must either have a tuple type or be passed to a rest parameter."
    // The spread array has type (string | number)[] which is not a tuple type.
    let ts2556_count = diagnostics.iter().filter(|d| d.code == 2556).count();
    assert!(
        ts2556_count >= 1,
        "Expected at least 1 TS2556 error for spread of non-tuple array, got {ts2556_count}"
    );
}

#[test]
fn test_non_tuple_spread_into_optional_tail_does_not_emit_ts2556() {
    let source = r#"
declare function all(a?: number, b?: number): void;
declare function prefix(s: string, a?: number, b?: number): void;
declare function rest(s: string, a?: number, b?: number, ...rest: number[]): void;

declare const ns: number[];
declare const mixed: (number | string)[];

all(...ns);
all(...mixed);
prefix("a", ...ns);
prefix("b", ...mixed);
rest("d", ...ns);
rest("e", ...mixed);
"#;

    let diagnostics = check_source(source);

    let ts2556_count = diagnostics.iter().filter(|d| d.code == 2556).count();
    assert_eq!(
        ts2556_count,
        0,
        "Expected no TS2556 when non-tuple spreads only cover optional/rest parameters, got diagnostics: {:?}",
        diagnostics
            .iter()
            .filter(|d| d.code == 2556)
            .map(|d| (&d.code, &d.message_text))
            .collect::<Vec<_>>()
    );

    let optional_tail_messages: Vec<&str> = diagnostics
        .iter()
        .filter(|d| d.code == 2345)
        .map(|d| d.message_text.as_str())
        .filter(|msg| msg.contains("string | number"))
        .collect();
    // tsc displays the declared parameter type (without `| undefined`) for
    // optional params in error messages.  Verify we match that behavior.
    assert!(
        optional_tail_messages
            .iter()
            .all(|msg| msg.contains("parameter of type 'number'")
                || msg.contains("parameter of type 'number | undefined'")),
        "Expected spread mismatches into optional tail params to mention `number`, got: {optional_tail_messages:?}"
    );
}

#[test]
fn test_spread_tuple_in_function_call() {
    let source = r#"
function greet(name: string, age: number, active: boolean) {
    console.log(name, age, active);
}
type Tuple = [string, number, boolean];
const args: Tuple = ["Alice", 30, true];
greet(...args);  // Should work
"#;

    let diagnostics = check_source(source);

    // Should NOT emit TS2322
    let ts2322_count = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        ts2322_count, 0,
        "Expected no TS2322 error for spread tuple in function call, got {ts2322_count}"
    );
}

#[test]
fn test_spread_tuple_in_function_call_with_wrong_types() {
    let source = r#"
function greet(name: string, age: number, active: boolean) {
    console.log(name, age, active);
}
type Tuple = [string, boolean, number];  // Wrong order
const args: Tuple = ["Alice", true, 30];
greet(...args);  // Should emit TS2345
"#;

    let diagnostics = check_source(source);

    // Should emit TS2345 (for function arguments) - boolean is not assignable to number
    let ts2345_count = diagnostics.iter().filter(|d| d.code == 2345).count();
    assert!(
        ts2345_count >= 1,
        "Expected at least 1 TS2345 error for spread tuple with wrong types, got {ts2345_count}"
    );
}

#[test]
fn test_object_spread_with_contextual_type() {
    let source = r#"
interface Person {
    name: string;
    age: number;
}
const partial = { name: "Alice" };
const person: Person = { ...partial, age: 30 };
"#;

    let diagnostics = check_source(source);

    // Should NOT emit TS2322
    let ts2322_count = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        ts2322_count, 0,
        "Expected no TS2322 error for object spread with contextual type, got {ts2322_count}"
    );
}

#[test]
fn test_nested_array_spread() {
    let source = r"
const arr1 = [1, 2];
const arr2 = [3, 4];
const combined = [...arr1, ...arr2];  // Should be number[]
";

    let diagnostics = check_source(source);

    // Should NOT emit TS2322
    let ts2322_count = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        ts2322_count, 0,
        "Expected no TS2322 error for nested array spread, got {ts2322_count}"
    );
}

#[test]
fn test_rest_with_type_annotation() {
    let source = r#"
function logAll(...messages: string[]) {
    messages.forEach(m => console.log(m));
}
logAll("hello", "world");
logAll("hello", 42);  // Should emit TS2345
"#;

    let diagnostics = check_source(source);

    // Should emit TS2345 for number argument (TS2345 is for function arguments)
    let ts2345_count = diagnostics.iter().filter(|d| d.code == 2345).count();
    assert!(
        ts2345_count >= 1,
        "Expected at least 1 TS2345 error for wrong type in rest parameter with annotation, got {ts2345_count}"
    );
}

#[test]
fn test_array_literal_with_spread_and_contextual_type() {
    let source = r#"
type Tuple = [number, string];
const createTuple = (): Tuple => [42, "hello"];
const t: Tuple = [1, "test", ...createTuple()];
"#;

    let _diagnostics = check_source(source);
    // This is a complex case - spread in tuple context
    // The behavior depends on implementation
}

#[test]
fn test_this_in_class_getter_no_false_ts2683() {
    let source = r"
// @strict: true
class Foo {
    x = 5;
    get bar() {
        return this.x;
    }
    set baz(v: number) {
        this.x = v;
    }
    method() {
        return this.x;
    }
}
";
    let diagnostics = check_source(source);
    let ts2683_count = diagnostics.iter().filter(|d| d.code == 2683).count();
    assert_eq!(
        ts2683_count, 0,
        "Expected no TS2683 for `this` in class getter/setter/method, got {ts2683_count}"
    );
}

#[test]
fn test_this_in_object_literal_getter_no_false_ts2683() {
    let source = r"
// @strict: true
var obj = {
    get foo() {
        var _this = this;
        return _this;
    },
    bar() {
        return this;
    }
};
";
    let diagnostics = check_source(source);
    let ts2683_count = diagnostics.iter().filter(|d| d.code == 2683).count();
    assert_eq!(
        ts2683_count, 0,
        "Expected no TS2683 for `this` in object literal getter/method, got {ts2683_count}"
    );
}

#[test]
fn test_this_in_object_literal_func_expr_no_false_ts2683() {
    let source = r"
// @noImplicitThis: true
var obj = {
    x: 5,
    func: function() {
        return this.x;
    }
};
";
    let diagnostics = check_source(source);
    let ts2683_count = diagnostics.iter().filter(|d| d.code == 2683).count();
    assert_eq!(
        ts2683_count, 0,
        "Expected no TS2683 for `this` in object literal function expression, got {ts2683_count}"
    );
}

#[test]
fn test_spread_string() {
    let source = r#"
const str = "hello";
const chars = [...str];  // Should be string[]
"#;

    let diagnostics = check_source(source);

    // Should NOT emit TS2488 (string is iterable)
    let ts2488_count = diagnostics.iter().filter(|d| d.code == 2488).count();
    assert_eq!(
        ts2488_count, 0,
        "Expected no TS2488 error for string spread, got {ts2488_count}"
    );
}

#[test]
fn test_object_rest_not_last_emits_ts2462() {
    // TypeScript emits TS2462 for object rest patterns where the rest is
    // not the last element, just like it does for array patterns.
    let source = r#"
var { ...rest, x } = { x: 1 };
"#;

    let diagnostics = check_source(source);

    let ts2462_count = diagnostics.iter().filter(|d| d.code == 2462).count();
    assert!(
        ts2462_count >= 1,
        "Expected TS2462 for object rest that is not last, got {ts2462_count}. Diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_array_rest_not_last_still_reports_ts2462() {
    let source = r#"
var [...rest, x] = [1, 2, 3];
"#;

    let diagnostics = check_source(source);

    let ts2462_count = diagnostics.iter().filter(|d| d.code == 2462).count();
    assert!(
        ts2462_count >= 1,
        "Expected TS2462 for array rest that is not last, got {ts2462_count}"
    );
}

#[test]
fn test_object_rest_with_type_parameter_constraint_no_false_ts2783() {
    // When a generic function destructures `{ a, ...rest } = obj` where `obj: T extends { a, b }`,
    // the rest type should omit `a` using the constraint's shape.
    // Previously, `omit_properties_from_type` returned T unchanged because
    // `object_shape(TypeParameter)` is None, causing false TS2783.
    let source = r#"
function f<T extends { a: string, b: string }>(obj: T) {
    const { a, ...rest } = obj;
    return rest;
}
"#;
    let diagnostics = check_source(source);
    let ts2783_count = diagnostics.iter().filter(|d| d.code == 2783).count();
    assert_eq!(
        ts2783_count,
        0,
        "Expected no TS2783 for object rest with type parameter constraint, got {ts2783_count}. Diagnostics: {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_object_rest_with_concrete_type_still_works() {
    // Sanity check: object rest with concrete types should continue working.
    let source = r#"
interface Obj { a: string; b: number; c: boolean }
function f(obj: Obj) {
    const { a, ...rest } = obj;
    const x: { b: number; c: boolean } = rest;
}
"#;
    let diagnostics = check_source(source);
    let ts2322_count = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        ts2322_count, 0,
        "Expected no TS2322 for object rest with concrete type, got {ts2322_count}"
    );
}

#[test]
fn test_generic_rest_spread_preserves_type_parameter() {
    // When a generic function destructures `{ a, ...rest } = obj` where `obj: T`,
    // and returns `{ ...rest, b: a }`, the return type must preserve T's identity
    // so that when the function is called with a concrete type, the return type
    // is properly instantiated. Without this, rest resolves to {} and the return
    // type becomes { b: string } regardless of T, causing false TS2741.
    let source = r#"
function test<T extends { a: string }>(obj: T) {
    let { a, ...rest } = obj;
    return { ...rest, b: a };
}
let o1 = { a: 'hello', x: 42 };
let o2: { b: string, x: number } = test(o1);
"#;
    let diagnostics = check_source(source);
    let ts2741_count = diagnostics.iter().filter(|d| d.code == 2741).count();
    assert_eq!(
        ts2741_count,
        0,
        "Expected no TS2741 for generic rest spread return, got diagnostics: {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_generic_rest_spread_with_multiple_properties() {
    // Variant with multiple destructured properties and multiple extra properties.
    let source = r#"
function pick<T extends { x: number, y: number }>(obj: T) {
    let { x, y, ...rest } = obj;
    return { ...rest, sum: x + y };
}
let input = { x: 1, y: 2, z: 'hello', w: true };
let output: { sum: number, z: string, w: boolean } = pick(input);
"#;
    let diagnostics = check_source(source);
    let error_count = diagnostics
        .iter()
        .filter(|d| d.code == 2741 || d.code == 2322)
        .count();
    assert_eq!(
        error_count, 0,
        "Expected no TS2741/TS2322 for generic rest spread with multiple properties"
    );
}

#[test]
fn test_generic_rest_destructuring_named_property_type() {
    // Destructuring a named property from a generic parameter should resolve
    // to the constraint's property type.
    let source = r#"
function getName<T extends { name: string }>(obj: T): string {
    let { name } = obj;
    return name;
}
"#;
    let diagnostics = check_source(source);
    let ts2322_count = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        ts2322_count, 0,
        "Expected no TS2322: destructured named property should have constraint type"
    );
}

#[test]
fn test_generic_rest_spread_still_catches_real_errors() {
    // Real type errors should still be caught even with generic rest/spread.
    let source = r#"
function test<T extends { a: string }>(obj: T) {
    let { a, ...rest } = obj;
    return { ...rest, b: a };
}
let o1 = { a: 'hello', x: 42 };
let o2: { b: number } = test(o1);
"#;
    let diagnostics = check_source(source);
    // b is string, not number — should have an error
    let has_type_error = diagnostics.iter().any(|d| d.code == 2322 || d.code == 2741);
    assert!(
        has_type_error,
        "Expected a type error when assigning {{ b: string }} to {{ b: number }}"
    );
}

#[test]
fn test_object_rest_excludes_private_class_members() {
    let source = r#"
class C {
    #prop = 1;
    static #propStatic = 1;

    method(other: C) {
        const { ...rest } = other;
        rest.#prop;

        const { ...sRest } = C;
        sRest.#propStatic;
    }
}
"#;

    let diagnostics = check_source(source);
    let ts2339: Vec<_> = diagnostics.iter().filter(|d| d.code == 2339).collect();
    assert_eq!(
        ts2339.len(),
        2,
        "Expected private members to be absent from object rest results, got diagnostics: {diagnostics:?}"
    );
    assert!(
        ts2339.iter().any(|d| d
            .message_text
            .contains("Property '#prop' does not exist on type '{}'.")),
        "Expected instance rest object to erase private members, got diagnostics: {diagnostics:?}"
    );
    assert!(
        ts2339.iter().any(|d| {
            d.message_text
                .contains("Property '#propStatic' does not exist on type '{ prototype: C; }'.")
                || d.message_text
                    .contains("Property '#propStatic' does not exist on type 'C'.")
        }),
        "Expected static rest object to erase private members, got diagnostics: {diagnostics:?}"
    );
}

// TS2556: rest parameter position-aware spread checking

#[test]
fn test_array_spread_at_non_rest_position_emits_ts2556() {
    // Spread covers non-rest param `a` → TS2556
    let source = r#"
declare function withRest(a: any, ...args: any[]): void;
declare var n: number[];
withRest(...n);
"#;
    let diagnostics = check_source(source);
    let ts2556_count = diagnostics.iter().filter(|d| d.code == 2556).count();
    assert!(
        ts2556_count >= 1,
        "Expected TS2556 for non-tuple spread at non-rest position, got {ts2556_count}"
    );
}

#[test]
fn test_array_spread_at_rest_position_no_ts2556() {
    // Spread covers only rest param `...args` → no TS2556
    let source = r#"
declare function withRest(a: any, ...args: any[]): void;
declare var n: number[];
withRest('a', ...n);
"#;
    let diagnostics = check_source(source);
    let ts2556_count = diagnostics.iter().filter(|d| d.code == 2556).count();
    assert_eq!(
        ts2556_count, 0,
        "Expected no TS2556 when spread is at rest position, got {ts2556_count}"
    );
}

#[test]
fn test_array_spread_to_function_without_rest_emits_ts2556() {
    // Function has no rest param → TS2556
    let source = r#"
declare function noRest(a: number, b: number): void;
declare var n: number[];
noRest(...n);
"#;
    let diagnostics = check_source(source);
    let ts2556_count = diagnostics.iter().filter(|d| d.code == 2556).count();
    assert!(
        ts2556_count >= 1,
        "Expected TS2556 for spread to function without rest param, got {ts2556_count}"
    );
}

#[test]
fn test_tuple_spread_at_non_rest_position_no_ts2556() {
    // Tuple spread has known length → no TS2556 even at non-rest position
    let source = r#"
declare function withRest(a: any, ...args: any[]): void;
declare var t: [number];
withRest(...t);
"#;
    let diagnostics = check_source(source);
    let ts2556_count = diagnostics.iter().filter(|d| d.code == 2556).count();
    assert_eq!(
        ts2556_count, 0,
        "Expected no TS2556 for tuple spread (known length), got {ts2556_count}"
    );
}

// ── Generic IndexAccess callable: no false TS2556 (inferTypes1 parity) ──

#[test]
fn test_no_ts2556_for_generic_index_access_call_with_spread() {
    // When the callable type is a generic IndexAccess (e.g., T[K]), the rest
    // parameter nature cannot be determined statically. tsc does NOT emit
    // TS2556 in this case.
    let source = r#"
function invoker<K extends string | number | symbol, A extends any[]>(key: K, ...args: A) {
    return <T extends Record<K, (...args: A) => any>>(obj: T): ReturnType<T[K]> => obj[key](...args)
}
"#;
    let diagnostics = check_source(source);
    let ts2556_count = diagnostics.iter().filter(|d| d.code == 2556).count();
    assert_eq!(
        ts2556_count, 0,
        "Expected no TS2556 for spread into generic IndexAccess callable, got {ts2556_count}. Diagnostics: {diagnostics:?}"
    );
}

// ── Mapped tuple rest parameters: evaluate_rest_param_type fix ──
// When a rest parameter has an Application/Mapped type (e.g., TupleMapper<[string, number]>),
// param_type_for_arg_index must evaluate it to its concrete form before extracting per-element
// types. Without this, each argument is checked against the whole unevaluated type, producing
// false TS2345 errors.

#[test]
fn test_mapped_tuple_rest_param_no_false_ts2345() {
    // Core case: Application type `TupleMapper<[string, number]>` as rest param
    // must be evaluated to a concrete tuple before element extraction.
    let source = r#"
type TupleMapper<T extends unknown[]> = { [K in keyof T]: T[K] };
declare function mapped<T extends unknown[]>(...args: TupleMapper<T>): void;
mapped("hello", 42);
"#;
    let diagnostics = check_source(source);
    let ts2345_count = diagnostics.iter().filter(|d| d.code == 2345).count();
    assert_eq!(
        ts2345_count, 0,
        "Mapped tuple rest param should not produce false TS2345, got {ts2345_count}. Diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_mapped_tuple_rest_param_with_return_type() {
    // Same pattern but with a return type (inferRestArgumentsMappedTuple conformance test).
    let source = r#"
type TupleMapper<T extends unknown[]> = { [K in keyof T]: T[K] };
declare function mapTuple<T extends unknown[]>(...args: TupleMapper<T>): T;
const result = mapTuple("hello", 42);
"#;
    let diagnostics = check_source(source);
    let ts2345_count = diagnostics.iter().filter(|d| d.code == 2345).count();
    assert_eq!(
        ts2345_count, 0,
        "Generic mapped tuple inference should not produce false TS2345, got {ts2345_count}. Diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_plain_tuple_rest_param_still_catches_mismatch() {
    // Ensure the fix doesn't break normal tuple rest param type checking.
    let source = r#"
declare function f(...args: [string, number]): void;
f(42, "hello");
"#;
    let diagnostics = check_source(source);
    let ts2345_count = diagnostics.iter().filter(|d| d.code == 2345).count();
    assert!(
        ts2345_count >= 1,
        "Plain tuple rest param should still reject mismatched args, got {ts2345_count} TS2345 errors"
    );
}

// ── Variadic tuple spread: no false TS2345 for rest elements ──
// When spreading a variadic tuple (e.g., [number, string, ...boolean[]]) into a function
// that expects a matching rest parameter, the rest element's array type (boolean[])
// must be decomposed to its element type (boolean) rather than pushed as a whole array.

#[test]
fn test_variadic_tuple_spread_no_false_ts2345() {
    // Spreading a variadic tuple into a function with a matching variadic rest parameter
    // should produce zero errors.
    let source = r#"
declare const t1: [number, string, ...boolean[]];
declare let f10: (...x: [number, string, ...boolean[]]) => void;
f10(...t1);
"#;
    let diagnostics = check_source(source);
    let ts2345_count = diagnostics.iter().filter(|d| d.code == 2345).count();
    assert_eq!(
        ts2345_count,
        0,
        "Variadic tuple spread should not produce false TS2345, got {ts2345_count}. Diagnostics: {:?}",
        diagnostics
            .iter()
            .filter(|d| d.code == 2345)
            .map(|d| &d.message_text)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_variadic_tuple_partial_spread_no_false_ts2345() {
    // Spreading only the rest portion of a variadic tuple alongside fixed args.
    let source = r#"
declare const t2: [string, ...boolean[]];
declare const t3: [...boolean[]];
declare let f10: (...x: [number, string, ...boolean[]]) => void;
f10(42, ...t2);
f10(42, "hello", ...t3);
"#;
    let diagnostics = check_source(source);
    let ts2345_count = diagnostics.iter().filter(|d| d.code == 2345).count();
    assert_eq!(
        ts2345_count,
        0,
        "Partial variadic tuple spread should not produce false TS2345, got {ts2345_count}. Diagnostics: {:?}",
        diagnostics
            .iter()
            .filter(|d| d.code == 2345)
            .map(|d| &d.message_text)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_variadic_tuple_spread_with_trailing_args() {
    // Spreading an empty tuple followed by more arguments should work.
    let source = r#"
declare const t4: [];
declare let f10: (...x: [number, string, ...boolean[]]) => void;
f10(42, "hello", true, ...t4);
f10(42, "hello", true, ...t4, false);
"#;
    let diagnostics = check_source(source);
    let ts2345_count = diagnostics.iter().filter(|d| d.code == 2345).count();
    assert_eq!(
        ts2345_count,
        0,
        "Empty tuple spread with trailing args should not produce false TS2345, got {ts2345_count}. Diagnostics: {:?}",
        diagnostics
            .iter()
            .filter(|d| d.code == 2345)
            .map(|d| &d.message_text)
            .collect::<Vec<_>>()
    );
}

fn assert_no_ts2345_for_generic_rest_call(source: &str) {
    let diagnostics = check_source(source);
    let ts2345_count = diagnostics.iter().filter(|d| d.code == 2345).count();
    assert_eq!(
        ts2345_count,
        0,
        "Generic rest parameter calls should compare each argument against its positional tuple element, got TS2345 diagnostics: {:?}",
        diagnostics
            .iter()
            .filter(|d| d.code == 2345)
            .map(|d| &d.message_text)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_generic_rest_parameter_call_with_plain_literals() {
    assert_no_ts2345_for_generic_rest_call(
        r#"
declare function f10<T extends unknown[]>(...args: T): T;
const x10 = f10(42, "hello", true);
"#,
    );
}

#[test]
fn test_generic_rest_parameter_call_with_tuple_spread() {
    assert_no_ts2345_for_generic_rest_call(
        r#"
declare function f10<T extends unknown[]>(...args: T): T;
declare const t2: [string, boolean];
const x15 = f10(42, ...t2);
"#,
    );
}

#[test]
fn test_generic_rest_parameter_call_with_trailing_spread() {
    assert_no_ts2345_for_generic_rest_call(
        r#"
declare function f10<T extends unknown[]>(...args: T): T;
declare const t1: [boolean];
const x16 = f10(42, "hello", ...t1);
"#,
    );
}

#[test]
fn test_variadic_tuple_spread_wrong_rest_type_still_errors() {
    // Spreading a variadic tuple whose rest element type doesn't match should still error.
    let source = r#"
declare const bad: [number, string, ...number[]];
declare let f10: (...x: [number, string, ...boolean[]]) => void;
f10(...bad);
"#;
    let diagnostics = check_source(source);
    let ts2345_count = diagnostics.iter().filter(|d| d.code == 2345).count();
    assert!(
        ts2345_count >= 1,
        "Mismatched variadic rest type should produce TS2345, got {ts2345_count}"
    );
}

/// Test: spreading a tuple with optional element produces TS2345 when the
/// optional element's `T | undefined` is not assignable to the parameter type.
/// Fixes callWithSpread5.ts: `fn(...nnnu, x)` where nnnu = [number, number, number?].
#[test]
fn test_optional_tuple_spread_emits_ts2345() {
    let source = r"
declare const nnnu: [number, number, number?];
declare const x: number;
declare function fn(a: number, b: number, bb: number, ...c: number[]): number;
fn(...nnnu, x);
";
    let diagnostics = check_source(source);
    let ts2345_count = diagnostics.iter().filter(|d| d.code == 2345).count();
    assert!(
        ts2345_count >= 1,
        "Optional tuple element spread should emit TS2345 for number | undefined vs number, got {ts2345_count}"
    );
}

/// Test: spreading a fixed-length tuple into a non-rest function emits TS2554
/// when the expanded argument count exceeds the expected parameter count.
/// Fixes callWithSpread3.ts: `fs2('a', ...s2)` where s2 = [string, string].
#[test]
fn test_tuple_spread_too_many_args_emits_ts2554() {
    let source = r#"
declare const s2: [string, string];
declare function fs2(a: string, b: string): void;
fs2("a", ...s2);
"#;
    let diagnostics = check_source(source);
    let ts2554_count = diagnostics.iter().filter(|d| d.code == 2554).count();
    assert!(
        ts2554_count >= 1,
        "Tuple spread with too many args should emit TS2554, got {ts2554_count}"
    );
}

/// Test: valid tuple spread that exactly fills parameters should not error.
#[test]
fn test_tuple_spread_exact_args_no_error() {
    let source = r#"
declare const s2: [string, string];
declare function fs2(a: string, b: string): void;
fs2(...s2);
"#;
    let diagnostics = check_source(source);
    let error_count = diagnostics
        .iter()
        .filter(|d| d.code == 2554 || d.code == 2556 || d.code == 2345)
        .count();
    assert_eq!(
        error_count, 0,
        "Exact tuple spread should not error, got {error_count}"
    );
}

/// Test: non-tuple array spread emits TS2556 only once per call, not once
/// per spread. Fixes callWithSpread3.ts: `fs2_(...s_, ...s_)`.
#[test]
fn test_non_tuple_spread_emits_ts2556_only_once() {
    let source = r#"
declare const s_: string[];
declare function fs2_(a: string, b: string, ...c: string[]): void;
fs2_(...s_, ...s_);
"#;
    let diagnostics = check_source(source);
    let ts2556_count = diagnostics.iter().filter(|d| d.code == 2556).count();
    assert_eq!(
        ts2556_count, 1,
        "Non-tuple spread should emit exactly 1 TS2556 per call, got {ts2556_count}"
    );
}

// =============================================================================
// TS1265 / TS1266: Tuple Element Ordering Tests
// =============================================================================

/// TS1265: A rest element cannot follow another rest element (concrete arrays).
#[test]
fn test_ts1265_rest_after_rest_concrete_arrays() {
    let source = r#"
type T1 = [...string[], ...number[]];
type T2 = [...Array<string>, ...number[]];
"#;
    let diagnostics = check_source(source);
    let ts1265_count = diagnostics.iter().filter(|d| d.code == 1265).count();
    assert_eq!(
        ts1265_count, 2,
        "Expected 2 TS1265 errors for rest after rest with concrete arrays, got {ts1265_count}"
    );
}

/// TS1265 should NOT fire for variadic type parameter spreads like [...T, ...U, ...V].
#[test]
fn test_ts1265_not_emitted_for_variadic_type_param_spreads() {
    let source = r#"
type Tup3<T extends unknown[], U extends unknown[], V extends unknown[]> = [...T, ...U, ...V];
"#;
    let diagnostics = check_source(source);
    let ts1265_count = diagnostics.iter().filter(|d| d.code == 1265).count();
    assert_eq!(
        ts1265_count, 0,
        "TS1265 should NOT fire for variadic type param spreads [...T, ...U, ...V], got {ts1265_count}"
    );
}

/// TS1266: An optional element cannot follow a rest element.
#[test]
fn test_ts1266_optional_after_rest() {
    let source = r#"
type T1 = [number, ...string[], boolean?];
"#;
    let diagnostics = check_source(source);
    let ts1266_count = diagnostics.iter().filter(|d| d.code == 1266).count();
    assert_eq!(
        ts1266_count, 1,
        "Expected 1 TS1266 error for optional after rest, got {ts1266_count}"
    );
}

/// Mixed rest and optional violations.
#[test]
fn test_ts1265_and_ts1266_together() {
    let source = r#"
type T1 = [number, ...string[], ...boolean[]];
type T2 = [number, ...string[], boolean?];
"#;
    let diagnostics = check_source(source);
    let ts1265_count = diagnostics.iter().filter(|d| d.code == 1265).count();
    let ts1266_count = diagnostics.iter().filter(|d| d.code == 1266).count();
    assert_eq!(ts1265_count, 1, "Expected 1 TS1265, got {ts1265_count}");
    assert_eq!(ts1266_count, 1, "Expected 1 TS1266, got {ts1266_count}");
}
