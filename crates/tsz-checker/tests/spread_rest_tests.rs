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
fn test_object_rest_not_last_does_not_emit_array_rest_error() {
    let source = r#"
var { ...rest, x } = { x: 1 };

({ ...rest, x } = { x: 1 });
"#;

    let diagnostics = check_source(source);

    let ts2462_count = diagnostics.iter().filter(|d| d.code == 2462).count();
    assert_eq!(
        ts2462_count, 0,
        "Expected no TS2462 for object rest when it is not an array pattern, got {ts2462_count}"
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
