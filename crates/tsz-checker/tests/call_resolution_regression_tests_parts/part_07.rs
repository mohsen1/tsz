#[test]
fn generic_call_with_optional_chaining() {
    let source = r#"
interface Processor {
    process<T>(x: T): T;
}
declare let p: Processor | undefined;
let result = p?.process(42);
"#;
    assert!(
        no_errors(source),
        "Generic call via optional chaining should not error"
    );
}

#[test]
fn optional_chain_call_returns_possibly_undefined() {
    let source = r#"
declare let obj: { f(): number } | undefined;
let result: number = obj?.f();
"#;
    // The result of obj?.f() is number | undefined, not number
    assert!(
        has_error(source, 2322),
        "Optional chain call result should be T | undefined"
    );
}

// ============================================================================
// IIFE with contextual typing
// ============================================================================

#[test]
fn iife_with_contextual_return_type() {
    let source = r#"
let result: number = (() => 42)();
"#;
    assert!(
        no_errors(source),
        "IIFE with contextual return type should not error"
    );
}

#[test]
fn iife_with_params() {
    let source = r#"
let result = (function(x: number) { return x + 1; })(5);
"#;
    assert!(no_errors(source), "IIFE with params should not error");
}

// ============================================================================
// Property-call regression patterns
// ============================================================================

#[test]
fn method_call_through_non_null_assertion() {
    let source = r#"
declare let obj: { f(): number } | undefined;
let result: number = obj!.f();
"#;
    assert!(
        no_errors(source),
        "Method call through non-null assertion should work"
    );
}

#[test]
fn method_call_on_intersection_type() {
    let source = r#"
interface A { foo(): number; }
interface B { bar(): string; }
declare let obj: A & B;
let n: number = obj.foo();
let s: string = obj.bar();
"#;
    assert!(
        no_errors(source),
        "Method call on intersection type should work"
    );
}

#[test]
fn method_call_on_generic_constraint() {
    let source = r#"
interface HasId { getId(): string; }
function getIdOf<T extends HasId>(obj: T): string {
    return obj.getId();
}
"#;
    assert!(
        no_errors(source),
        "Method call on generic constraint should work"
    );
}

#[test]
fn chained_method_calls() {
    let source = r#"
interface Builder {
    setName(n: string): Builder;
    build(): { name: string };
}
declare let b: Builder;
let result = b.setName("test").build();
"#;
    assert!(no_errors(source), "Chained method calls should work");
}

// ============================================================================
// Overload with generic and non-generic signatures
// ============================================================================

#[test]
fn overload_generic_and_non_generic_mixed() {
    let source = r#"
declare function f(x: string): string;
declare function f<T>(x: T): T;
let s: string = f("hello");
let n: number = f(42);
"#;
    assert!(
        no_errors(source),
        "Mixed generic/non-generic overloads should resolve"
    );
}

#[test]
fn overload_with_optional_params_ambiguity() {
    let source = r#"
declare function f(x: number): number;
declare function f(x: number, y?: string): string;
let result: number = f(42);
"#;
    assert!(
        no_errors(source),
        "Overload with optional params should pick first match"
    );
}

// ============================================================================
// Type predicate through call resolution
// ============================================================================

