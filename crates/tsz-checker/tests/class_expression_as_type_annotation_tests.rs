//! Tests for class expression variables used as type annotations.
//!
//! Structural rule: when a `const`/`let`/`var` variable's initializer is a
//! class expression, TypeScript treats the variable name as both a value (the
//! constructor) and a type (the instance type), exactly like a class
//! declaration.  The variable must NOT emit TS2749 ("refers to a value, but is
//! being used as a type here") when used as a type annotation.
//!
//! This covers the adjacent-case matrix required by CLAUDE.md §26:
//!   1. Non-generic class expression, named and anonymous.
//!   2. Generic class expression with one and two type parameters.
//!   3. Renamed type-parameter spellings (T, U, X, Item, V) — fixing the rule,
//!      not the spelling.
//!   4. `const`, `let`, and `var` declarations.
//!   5. Nested / wrapper class expressions (class inside a method).
//!   6. Negative cases: plain function variables and object literals still
//!      emit TS2749 when used as types.
//!   7. Original issue repro (#6199): `InstanceType<typeof GenericExprClass>`.

fn codes(source: &str) -> Vec<u32> {
    tsz_checker::test_utils::check_source_codes(source)
}

fn messages(source: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_source_code_messages(source)
}

// ---------------------------------------------------------------------------
// 1. Non-generic class expression used as a type annotation
// ---------------------------------------------------------------------------

/// `const Foo = class { ... }` — using `Foo` as a type must NOT emit TS2749.
#[test]
fn test_non_generic_class_expr_const_as_type_no_ts2749() {
    let diags = codes(
        r#"
const Foo = class {
    x: number = 0;
};
declare function accept(v: Foo): void;
"#,
    );
    assert!(
        !diags.contains(&2749),
        "Non-generic `const Foo = class {{}}` used as type should not emit TS2749. Got: {diags:?}"
    );
}

/// Named class expression: `const Bar = class MyBar { ... }` — the variable
/// name `Bar` must be usable as a type.
#[test]
fn test_named_class_expr_variable_as_type_no_ts2749() {
    let diags = codes(
        r#"
const Bar = class MyBar {
    name: string = "";
};
declare function accept(v: Bar): void;
"#,
    );
    assert!(
        !diags.contains(&2749),
        "Named class expr variable `Bar` used as type should not emit TS2749. Got: {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// 2. Generic class expression — single type parameter
// ---------------------------------------------------------------------------

/// `const Box = class<T> { value: T; }` — `Box<number>` as a type must work.
#[test]
fn test_generic_class_expr_const_as_type_no_ts2749() {
    let diags = codes(
        r#"
const Box = class<T> {
    value: T;
    constructor(v: T) { this.value = v; }
};
declare function use(b: Box<number>): void;
"#,
    );
    assert!(
        !diags.contains(&2749),
        "Generic `const Box = class<T>{{}}` used as `Box<number>` type should not emit TS2749. Got: {diags:?}"
    );
}

/// Renamed type parameter `U` — the rule must not be tied to a specific
/// type-parameter letter.
#[test]
fn test_generic_class_expr_renamed_tparam_u_as_type_no_ts2749() {
    let diags = codes(
        r#"
const Wrapper = class<U> {
    inner: U;
    constructor(v: U) { this.inner = v; }
};
declare function take(w: Wrapper<string>): void;
"#,
    );
    assert!(
        !diags.contains(&2749),
        "Generic class expr with tparam `U` used as type should not emit TS2749. Got: {diags:?}"
    );
}

/// Renamed type parameter `Item` — longer non-single-letter name.
#[test]
fn test_generic_class_expr_renamed_tparam_item_as_type_no_ts2749() {
    let diags = codes(
        r#"
const Container = class<Item> {
    items: Item[] = [];
};
declare function process(c: Container<boolean>): void;
"#,
    );
    assert!(
        !diags.contains(&2749),
        "Generic class expr with tparam `Item` used as type should not emit TS2749. Got: {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// 3. Generic class expression — two type parameters
// ---------------------------------------------------------------------------

/// `const Pair = class<K, V> { ... }` — `Pair<string, number>` as a type.
#[test]
fn test_generic_class_expr_two_tparams_as_type_no_ts2749() {
    let diags = codes(
        r#"
const Pair = class<K, V> {
    key: K;
    val: V;
    constructor(k: K, v: V) { this.key = k; this.val = v; }
};
declare function store(p: Pair<string, number>): void;
"#,
    );
    assert!(
        !diags.contains(&2749),
        "Generic class expr with two tparams used as type should not emit TS2749. Got: {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// 4. `let` and `var` declarations (not just `const`)
// ---------------------------------------------------------------------------

/// `let Foo = class { ... }` — same treatment as `const`.
#[test]
fn test_let_class_expr_as_type_no_ts2749() {
    let diags = codes(
        r#"
let Baz = class {
    y: boolean = false;
};
declare function g(v: Baz): void;
"#,
    );
    assert!(
        !diags.contains(&2749),
        "`let Baz = class {{}}` used as type should not emit TS2749. Got: {diags:?}"
    );
}

/// `var Qux = class { ... }` — `var` declarations also introduce a class type.
#[test]
fn test_var_class_expr_as_type_no_ts2749() {
    let diags = codes(
        r#"
var Qux = class {
    z: string = "";
};
declare function h(v: Qux): void;
"#,
    );
    assert!(
        !diags.contains(&2749),
        "`var Qux = class {{}}` used as type should not emit TS2749. Got: {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// 5. Class expression used as return type annotation (not just parameter)
// ---------------------------------------------------------------------------

/// The fix must cover return-type positions as well as parameter-type positions.
#[test]
fn test_class_expr_as_return_type_no_ts2749() {
    let diags = codes(
        r#"
const Producer = class<T> {
    make(): T { return undefined as any; }
};
declare function factory(): Producer<number>;
"#,
    );
    assert!(
        !diags.contains(&2749),
        "Generic class expr used as return type annotation should not emit TS2749. Got: {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// 6. Negative cases — non-class-expression variables MUST still emit TS2749
// ---------------------------------------------------------------------------

/// A plain function expression is NOT a type; using it as one must emit TS2749.
#[test]
fn test_function_expr_variable_as_type_emits_ts2749() {
    let diags = codes(
        r#"
const notAClass = function() { return 1; };
declare function bad(v: notAClass): void;
"#,
    );
    assert!(
        diags.contains(&2749),
        "Plain function expression variable used as type MUST emit TS2749. Got: {diags:?}"
    );
}

/// An object literal variable is NOT a type.
#[test]
fn test_object_literal_variable_as_type_emits_ts2749() {
    let diags = codes(
        r#"
const obj = { x: 1 };
declare function bad(v: obj): void;
"#,
    );
    assert!(
        diags.contains(&2749),
        "Object literal variable used as type MUST emit TS2749. Got: {diags:?}"
    );
}

/// An arrow-function variable is NOT a type.
#[test]
fn test_arrow_function_variable_as_type_emits_ts2749() {
    let diags = codes(
        r#"
const arrowFn = () => 42;
declare function bad(v: arrowFn): void;
"#,
    );
    assert!(
        diags.contains(&2749),
        "Arrow function variable used as type MUST emit TS2749. Got: {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// 7. Original issue #6199: InstanceType<typeof GenericExprClass> — no TS2344
// ---------------------------------------------------------------------------

/// Structural rule: `InstanceType<typeof E>` where `E` is a generic class
/// expression variable must NOT emit TS2344 ("does not satisfy the constraint
/// 'abstract new (...args: any) => any'").
///
/// TypeScript resolves `typeof E` to the constructor shape of the class, which
/// has a construct signature, satisfying `InstanceType`'s constraint.
#[test]
fn test_instance_type_of_generic_class_expr_var_no_ts2344() {
    let diags = messages(
        r#"
interface Object {}
interface Function {}
interface CallableFunction {}
interface NewableFunction {}
interface IArguments {}
interface Number {}
interface String {}
interface Boolean {}
interface RegExp {}
interface Array<T> {}

type InstanceType<T extends abstract new (...args: any) => any> =
    T extends abstract new (...args: any) => infer R ? R : any;

const GenericExpr = class<T> {
    value: T;
    constructor(v: T) { this.value = v; }
};

type Instance = InstanceType<typeof GenericExpr>;
"#,
    );
    let ts2344: Vec<_> = diags.iter().filter(|(c, _)| *c == 2344).collect();
    assert!(
        ts2344.is_empty(),
        "InstanceType<typeof GenericExpr> must not emit TS2344. Got: {ts2344:?}"
    );
}

/// Same check with a differently-named type parameter (`X` instead of `T`).
#[test]
fn test_instance_type_of_generic_class_expr_renamed_tparam_no_ts2344() {
    let diags = messages(
        r#"
interface Object {}
interface Function {}
interface CallableFunction {}
interface NewableFunction {}
interface IArguments {}
interface Number {}
interface String {}
interface Boolean {}
interface RegExp {}
interface Array<T> {}

type InstanceType<T extends abstract new (...args: any) => any> =
    T extends abstract new (...args: any) => infer R ? R : any;

const GenericCls = class<X> {
    item: X;
    constructor(v: X) { this.item = v; }
};

type Inst = InstanceType<typeof GenericCls>;
"#,
    );
    let ts2344: Vec<_> = diags.iter().filter(|(c, _)| *c == 2344).collect();
    assert!(
        ts2344.is_empty(),
        "InstanceType<typeof GenericCls> with tparam `X` must not emit TS2344. Got: {ts2344:?}"
    );
}
