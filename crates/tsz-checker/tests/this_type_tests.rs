//! Tests for polymorphic `this` type in class methods.
//!
//! When a class method body does `return this;` without an explicit return type
//! annotation, the inferred return type should be the polymorphic `ThisType`
//! (not the concrete declaring class type).  This enables fluent method chaining
//! on subclass instances.

use tsz_checker::context::CheckerOptions;

/// Helper to compile TypeScript and get diagnostics
fn compile_and_get_diagnostics(source: &str) -> Vec<(u32, String)> {
    compile_and_get_diagnostics_with_options(source, CheckerOptions::default())
}

fn compile_and_get_diagnostics_with_options(
    source: &str,
    options: CheckerOptions,
) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_with_options(source, options)
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

fn has_error(diagnostics: &[(u32, String)], code: u32) -> bool {
    diagnostics.iter().any(|(c, _)| *c == code)
}

fn errors_with_code(diagnostics: &[(u32, String)], code: u32) -> Vec<&str> {
    diagnostics
        .iter()
        .filter(|(c, _)| *c == code)
        .map(|(_, msg)| msg.as_str())
        .collect()
}

fn messages(diagnostics: &[(u32, String)]) -> Vec<&str> {
    diagnostics.iter().map(|(_, msg)| msg.as_str()).collect()
}

/// Fluent method chaining: `c.foo().bar().baz()` where foo/bar/baz are defined
/// on classes A/B/C in a hierarchy and each returns `this` implicitly.
///
/// Without polymorphic `this`, `c.foo()` would return `A` (the declaring class)
/// and `.bar()` would fail because `bar` is only on `B`.
#[test]
fn test_fluent_class_chain_no_false_ts2339() {
    let source = r#"
class A {
    foo() {
        return this;
    }
}
class B extends A {
    bar() {
        return this;
    }
}
class C extends B {
    baz() {
        return this;
    }
}
declare var c: C;
var z = c.foo().bar().baz();
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    let ts2339_errors = errors_with_code(&diagnostics, 2339);
    assert!(
        ts2339_errors.is_empty(),
        "Should not have TS2339 for fluent chain, but got: {ts2339_errors:?}"
    );
}

/// When a method has an explicit return type annotation (not inferred),
/// the annotation should be used as-is. Only unannotated methods that
/// `return this;` should get polymorphic `ThisType`.
#[test]
fn test_explicit_return_type_not_replaced() {
    let source = r#"
class A {
    foo(): A {
        return this;
    }
}
class B extends A {
    bar() {
        return this;
    }
}
declare var b: B;
var x = b.foo();  // Should be A, not B (explicit annotation)
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    // foo() has explicit `: A` annotation, so b.foo() returns A.
    // Accessing .bar() on A should fail.
    // This test just verifies no crash and no false positive on `b.foo()`.
    assert!(
        !has_error(&diagnostics, 2339),
        "Should not error on b.foo() since A.foo() returns A"
    );
}

/// A method that returns `this.property` should NOT get polymorphic return type.
/// Only direct `return this;` contributes the class instance type that triggers
/// the polymorphic `ThisType` substitution.
#[test]
fn test_return_this_property_stays_concrete() {
    let source = r#"
class A {
    x: number = 5;
    getX() {
        return this.x;
    }
}
class B extends A {
    y: string = "hello";
}
declare var b: B;
var result: number = b.getX();
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    // getX() should return number (from this.x), not ThisType.
    // Assigning to `number` should not error.
    assert!(
        !has_error(&diagnostics, 2322),
        "getX() should return number, not polymorphic this: {diagnostics:?}"
    );
}

#[test]
fn test_generic_class_this_indexed_array_element_return() {
    let source = r#"
class Container<T> {
    items: T[] = [];

    getFirst(): this["items"][number] | undefined {
        return this.items[0];
    }
}

export {};
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        !has_error(&diagnostics, 2322),
        "this[\"items\"][number] should resolve to the class array element type. Got: {diagnostics:?}"
    );
}

/// Regression guard: accessing a property that truly doesn't exist should
/// still produce TS2339, even with the polymorphic this type fix.
#[test]
fn test_nonexistent_property_still_errors() {
    let source = r#"
class A {
    foo() {
        return this;
    }
}
declare var a: A;
var x = a.foo().nonExistent;
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        has_error(&diagnostics, 2339),
        "Should get TS2339 for nonexistent property: {diagnostics:?}"
    );
}

#[test]
fn test_generic_this_index_assignment_in_class_method_has_no_false_ts2322() {
    let source = r#"
class C1 {
    x: number;
    get<K extends keyof this>(key: K) {
        return this[key];
    }
    set<K extends keyof this>(key: K, value: this[K]) {
        this[key] = value;
    }
}
"#;

    let diagnostics = compile_and_get_diagnostics_with_options(
        source,
        CheckerOptions {
            emit_declarations: true,
            strict_property_initialization: false,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 2322),
        "Generic this-index assignment should not emit TS2322: {diagnostics:?}"
    );
}

#[test]
fn test_direct_this_access_finds_declared_abstract_method_in_generic_class() {
    let source = r#"
abstract class Box<Output, Def extends {} = {}, Input = Output> {
    readonly _output!: Output;
    abstract _parse(value: Input): Output;
    parse(value: Input): Output {
        return this._parse(value);
    }
}
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let ts2339_errors = errors_with_code(&diagnostics, 2339);
    assert!(
        ts2339_errors.is_empty(),
        "Direct this access should find declared abstract methods: {diagnostics:?}"
    );
}

#[test]
fn test_direct_this_access_finds_later_declared_method_in_generic_class() {
    let source = r#"
abstract class Box<T> {
    parse(value: T): T {
        return this.safeParse(value);
    }
    safeParse(value: T): T {
        return value;
    }
}
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let ts2339_errors = errors_with_code(&diagnostics, 2339);
    assert!(
        ts2339_errors.is_empty(),
        "Direct this access should find later declared instance methods: {diagnostics:?}"
    );
}

#[test]
fn test_direct_this_access_does_not_expose_static_method() {
    let source = r#"
class C {
    static s(): number {
        return 1;
    }
    m() {
        return this.s();
    }
}
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        diagnostics
            .iter()
            .any(|(code, message)| { *code == 2576 && message.contains("static member 'C.s'") }),
        "Static methods should not be exposed through instance this: {diagnostics:?}"
    );
}

#[test]
fn test_generic_this_index_assignment_in_base_class_has_no_false_ts2322() {
    let source = r#"
class Base {
    get<K extends keyof this>(prop: K) {
        return this[prop];
    }
    set<K extends keyof this>(prop: K, value: this[K]) {
        this[prop] = value;
    }
}
class Person extends Base {
    parts: number;
    constructor(parts: number) {
        super();
        this.set("parts", parts);
    }
}
"#;

    let diagnostics = compile_and_get_diagnostics_with_options(
        source,
        CheckerOptions {
            emit_declarations: true,
            strict_property_initialization: false,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 2322),
        "Base-class generic this-index assignment should not emit TS2322: {diagnostics:?}"
    );
}

#[test]
fn test_keyof_this_call_arguments_do_not_reuse_other_class_key_cache() {
    let source = r#"
function getProperty<T, K extends keyof T>(obj: T, key: K) {
    return obj[key];
}

function setProperty<T, K extends keyof T>(obj: T, key: K, value: T[K]) {
    obj[key] = value;
}

class C1 {
    x: number;
    get<K extends keyof this>(key: K) {
        return this[key];
    }
    set<K extends keyof this>(key: K, value: this[K]) {
        this[key] = value;
    }
    foo() {
        this.get("x");
        getProperty(this, "x");
        this.set("x", 42);
        setProperty(this, "x", 42);
    }
}

class OtherPerson {
    parts: number;
    constructor(parts: number) {
        setProperty(this, "parts", parts);
    }
    getParts() {
        return getProperty(this, "parts");
    }
}
"#;

    let diagnostics = compile_and_get_diagnostics_with_options(
        source,
        CheckerOptions {
            emit_declarations: true,
            strict_null_checks: true,
            strict_property_initialization: false,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 2345),
        "`keyof this` evaluation must stay class-context-sensitive and not \
         reuse another class's cached keys: {diagnostics:?}"
    );
}

#[test]
fn test_direct_this_property_access_preserves_polymorphic_this_in_class_members() {
    let source = r#"
class C {
    self = this;
    c = new C();
    foo() {
        return this;
    }
    f2() {
        var a: C[];
        var a = [this, this.c];
        var b: this[];
        var b = [this, this.self, null, undefined];
    }
}
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let ts2403 = errors_with_code(&diagnostics, 2403);

    assert!(
        ts2403.len() == 1,
        "Expected only the this[] duplicate declaration to emit TS2403: {diagnostics:?}"
    );
    assert!(
        ts2403[0].contains(
            "Variable 'b' must be of type 'this[]', but here has type '(this | null | undefined)[]'."
        ),
        "Expected duplicate declaration message to preserve this[]: {ts2403:?}"
    );
}

#[test]
fn test_this_type_relationship_assignment_diagnostics_use_nominal_class_names() {
    let source = r#"
class C {
    self = this;
    c = new C();
    foo() {
        return this;
    }
}

class D extends C {
    self1 = this;
    self2 = this.self;
    self3 = this.foo();
    d = new D();
    bar() {
        this.d = this.self;
        this.d = this.c;
        this.self = this.d;
        this.c = this.d;
    }
}
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let all_messages = messages(&diagnostics);

    assert!(
        all_messages.contains(&"Type 'C' is missing the following properties from type 'D': self1, self2, self3, d, bar"),
        "Expected C-to-D TS2739 message, got: {diagnostics:?}"
    );
    assert!(
        all_messages.contains(&"Type 'D' is not assignable to type 'this'."),
        "Expected D-to-this TS2322 message, got: {diagnostics:?}"
    );
    assert!(
        !all_messages.iter().any(|msg| msg.contains("{ self:")),
        "Class instance diagnostics should not expand to anonymous object shapes: {diagnostics:?}"
    );
}

/// Regression: assigning a polymorphic-this call result to a base class variable
/// must not emit TS2322. `derived.clone()` returns `Derived` (the polymorphic
/// this), which IS assignable to `Base`. tsc accepts this without error.
/// See: <https://github.com/mohsen1/tsz/issues/3135>
#[test]
fn test_polymorphic_this_subtype_assignment_no_false_ts2322() {
    let source = r#"
class Base {
    clone(): this {
        return this;
    }
}

class Derived extends Base {
    derivedOnly = 1;
}

const derived = new Derived();
const base: Base = derived.clone();
"#;
    let diagnostics = compile_and_get_diagnostics_with_options(
        source,
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
    );
    assert!(
        !has_error(&diagnostics, 2322),
        "Polymorphic-this call result assigned to base type must not emit TS2322: {diagnostics:?}"
    );
}

/// Variant: multiple levels of inheritance must not emit TS2322 for
/// polymorphic-this assignments to any ancestor type.
#[test]
fn test_polymorphic_this_multi_level_subtype_assignment_no_false_ts2322() {
    let source = r#"
class A {
    clone(): this { return this; }
}
class B extends A { bProp = 1; }
class C extends B { cProp = 2; }

const c = new C();
const b: B = c.clone();
const a: A = c.clone();
"#;
    let diagnostics = compile_and_get_diagnostics_with_options(
        source,
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
    );
    assert!(
        !has_error(&diagnostics, 2322),
        "Multi-level polymorphic-this subtype assignment must not emit TS2322: {diagnostics:?}"
    );
}

/// Single-signature callable interface whose call signature returns `this`.
/// Calling a value of that interface type must return the interface type, not the
/// unresolved polymorphic `this`.
#[test]
fn test_callable_interface_this_return_single_sig() {
    let source = r#"
interface Builder {
    (): this;
    name: string;
}
declare const b: Builder;
const result = b();
const _: string = result.name;
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        !has_error(&diagnostics, 2339),
        "Calling callable interface with `this` return should resolve `.name` without TS2339: {diagnostics:?}"
    );
    assert!(
        !has_error(&diagnostics, 2322),
        "Assigning result.name to string must not emit TS2322: {diagnostics:?}"
    );
}

/// Callable type alias (not an interface) with a `this` return signature.
/// Ensures the fix covers type aliases, not only declared interfaces.
#[test]
fn test_callable_type_alias_this_return() {
    let source = r#"
type Factory = {
    (): this;
    tag: number;
};
declare const f: Factory;
const r = f();
const _: number = r.tag;
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        !has_error(&diagnostics, 2339),
        "Callable type alias with `this` return must resolve `.tag` after call: {diagnostics:?}"
    );
    assert!(
        !has_error(&diagnostics, 2322),
        "Assigning r.tag to number must not emit TS2322: {diagnostics:?}"
    );
}

/// Multi-signature callable interface where the first overload returns `this`
/// and the second returns a concrete type.  Only the `this`-returning overload
/// is called here; the result must have the callee type.
#[test]
fn test_callable_interface_this_return_multi_sig() {
    let source = r#"
interface MultiBuilder {
    (x: number): this;
    (x: string): string;
    prop: boolean;
}
declare const mb: MultiBuilder;
// Call the number overload → returns this (= MultiBuilder).
const r = mb(42);
const _: boolean = r.prop;
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        !has_error(&diagnostics, 2339),
        "Multi-sig callable: `this`-returning overload result must expose `.prop`: {diagnostics:?}"
    );
    assert!(
        !has_error(&diagnostics, 2322),
        "Multi-sig callable: r.prop assigned to boolean must not emit TS2322: {diagnostics:?}"
    );
}

/// Regression guard: a callable interface whose call signature returns a
/// concrete type (not `this`) must not be affected by the substitution.
#[test]
fn test_callable_interface_concrete_return_unaffected() {
    let source = r#"
interface Maker {
    (): string;
    tag: number;
}
declare const m: Maker;
const r = m();
const _s: string = r;
const _t: number = m.tag;
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        !has_error(&diagnostics, 2322),
        "Concrete-return callable must not be affected by this-substitution: {diagnostics:?}"
    );
    assert!(
        !has_error(&diagnostics, 2339),
        "Concrete-return callable: `.tag` property must still be accessible on callee: {diagnostics:?}"
    );
}

/// Regression guard: method calls on classes already work via property-access
/// substitution. They must not be broken by the new call-result substitution.
#[test]
fn test_class_method_this_return_still_works_after_fix() {
    let source = r#"
class Node {
    value: number = 0;
    setVal(v: number): this {
        this.value = v;
        return this;
    }
}
class SpecialNode extends Node {
    extra: string = "";
}
declare const sn: SpecialNode;
const r = sn.setVal(1);
const _: string = r.extra;
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        !has_error(&diagnostics, 2339),
        "Class method returning `this` must still expose subclass properties: {diagnostics:?}"
    );
    assert!(
        !has_error(&diagnostics, 2322),
        "r.extra assigned to string must not emit TS2322: {diagnostics:?}"
    );
}

#[test]
fn function_call_apply_bind_use_declared_this_parameter_not_host_object() {
    let source = r#"
function greet(this: { name: string }, greeting: string): string {
    return greeting + this.name;
}
const obj = { name: "Alice", greet };
const called: string = obj.greet.call({ name: "Bob" }, "Hello");
const applied: string = obj.greet.apply({ name: "Bob" }, ["Hello"]);
const bound: (greeting: string) => string = obj.greet.bind({ name: "Bob" });
const boundResult: string = bound("Hello");
export {};
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "Function call/apply/bind should accept the function's declared this parameter, got: {diagnostics:?}"
    );
}

#[test]
fn this_array_push_accepts_this_element_with_lib_array_signature() {
    let source = r#"
class State {
  history: this[] = [];

  save(): void {
    this.history.push(this);
  }
}

const s = new State();
s.save();
"#;
    let libs = tsz_checker::test_utils::load_compiled_lib_files(&["lib.es5.d.ts"]);
    let diagnostics = tsz_checker::test_utils::check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions::default(),
        &libs,
    )
    .into_iter()
    .map(|d| (d.code, d.message_text))
    .collect::<Vec<_>>();
    assert!(
        diagnostics.is_empty(),
        "pushing this into this[] should not produce diagnostics, got: {diagnostics:?}"
    );
}
