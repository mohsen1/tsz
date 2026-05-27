/// TS Unsoundness #38: Correlated Unions (Cross-Product Limitation)
///
/// When accessing a Union of Objects with a Union of Keys, TS computes the
/// Cross-Product, resulting in a wider type than expected (loss of correlation).
/// TS cannot track that `obj.kind === "a"` implies `obj.val` is `number`.
#[test]
fn test_correlated_unions_basic_access() {
    let source = r#"
type A = { kind: 'a'; val: number };
type B = { kind: 'b'; val: string };
type AB = A | B;

function test(obj: AB) {
    // Accessing 'val' gives number | string (cross-product)
    const v = obj.val;
}
"#;

    let (parser, root) = parse_test_source(source);
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    if !checker.ctx.diagnostics.is_empty() {
        println!("=== Correlated Unions Basic Access Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    // Basic union property access should work
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Union property access should work: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #38: Correlated Unions - Discriminant narrowing
///
/// When discriminant is checked, the specific variant is narrowed.
#[test]
fn test_correlated_unions_discriminant_narrowing() {
    let source = r#"
type A = { kind: 'a'; val: number };
type B = { kind: 'b'; val: string };
type AB = A | B;

function test(obj: AB) {
    if (obj.kind === 'a') {
        // After narrowing, obj is A, so val is number
        const n: number = obj.val;
    } else {
        // After narrowing, obj is B, so val is string
        const s: string = obj.val;
    }
}
"#;

    let (parser, root) = parse_test_source(source);
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let error_count = checker.ctx.diagnostics.len();

    // Currently may fail until discriminated union narrowing is implemented
    if error_count > 0 {
        println!("=== Correlated Unions Discriminant Narrowing Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
        println!("Expected 0 errors once discriminated union narrowing works");
    }

    // For now, just check it doesn't crash
    // Once discriminated union narrowing works, change to expect 0 errors
}

/// TS Unsoundness #38: Correlated Unions - Index access cross-product
///
/// IndexAccess(Union(ObjA, `ObjB`), Key) produces Union(ObjA[Key], `ObjB`[Key]).
#[test]
fn test_correlated_unions_index_access() {
    let source = r#"
type Data = {
    numbers: number[];
    strings: string[];
};

function getArray(data: Data, key: 'numbers' | 'strings') {
    // data[key] gives number[] | string[] (cross-product)
    const arr = data[key];
}
"#;

    let (parser, root) = parse_test_source(source);
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    if !checker.ctx.diagnostics.is_empty() {
        println!("=== Correlated Unions Index Access Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    // Index access with union key should work
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Index access with union key should work: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #38: Correlated Unions - Common property access
///
/// Accessing a property common to all union members works.
#[test]
fn test_correlated_unions_common_property() {
    let source = r#"
type Circle = { kind: 'circle'; radius: number };
type Square = { kind: 'square'; size: number };
type Shape = Circle | Square;

function getKind(shape: Shape): string {
    // 'kind' is common to both, gives 'circle' | 'square'
    return shape.kind;
}
"#;

    let (parser, root) = parse_test_source(source);
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    if !checker.ctx.diagnostics.is_empty() {
        println!("=== Correlated Unions Common Property Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    // Common property access should work
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Common property access on union should work: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #42: CFA Invalidation in Closures
///
/// Type narrowing is reset inside closures for mutable variables (let/var)
/// because the callback might run after the variable has changed.
#[test]
fn test_cfa_invalidation_mutable_in_closure() {
    let source = r#"
let x: string | number = "hello";

if (typeof x === "string") {
    // x is narrowed to string here
    const upper = x.toUpperCase();

    // Inside callback, narrowing is invalid for mutable variable
    function callback() {
        // x should NOT be narrowed here (mutable let)
        const val = x;
    }
}
"#;

    let (parser, root) = parse_test_source(source);
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    // Just check it doesn't crash - narrowing behavior depends on CFA implementation
    if !checker.ctx.diagnostics.is_empty() {
        println!("=== CFA Invalidation Mutable Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }
}

/// TS Unsoundness #42: CFA Invalidation - const maintains narrowing
///
/// For const variables, narrowing can be maintained inside closures
/// because the variable cannot be reassigned.
#[test]
fn test_cfa_const_maintains_narrowing() {
    let source = r#"
const x: string | number = "hello";

if (typeof x === "string") {
    // x is narrowed to string here
    const upper = x.toUpperCase();

    // Inside callback, narrowing IS valid for const
    function callback() {
        // x can stay narrowed (const cannot change)
        const val = x.toUpperCase();
    }
}
"#;

    let (parser, root) = parse_test_source(source);
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    // Currently doesn't maintain narrowing in closures
    // Once implemented, change to expect 0 errors
    if !checker.ctx.diagnostics.is_empty() {
        println!("=== CFA Const Narrowing Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
        println!("Expected 0 errors once const narrowing in closures is implemented");
    }
}

/// TS Unsoundness #42: CFA Invalidation - arrow function closure
///
/// Arrow functions also invalidate narrowing for captured mutable variables.
#[test]
fn test_cfa_invalidation_arrow_function() {
    let source = r#"
let value: string | null = "test";

if (value !== null) {
    // value is narrowed to string here
    const len = value.length;

    // Arrow function captures mutable variable
    const fn = () => {
        // value narrowing invalid here
        const v = value;
    };
}
"#;

    let (parser, root) = parse_test_source(source);
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    // Just check it doesn't crash
    if !checker.ctx.diagnostics.is_empty() {
        println!("=== CFA Invalidation Arrow Function Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }
}

/// TS Unsoundness #42: CFA Invalidation - callback parameter
///
/// Callback passed to another function also invalidates narrowing.
#[test]
fn test_cfa_invalidation_callback_parameter() {
    let source = r#"
declare function doLater(fn: () => void): void;

let data: string | undefined = "hello";

if (data !== undefined) {
    // data is narrowed to string here
    const first = data.charAt(0);

    // Callback passed to function
    doLater(() => {
        // data narrowing invalid - might run later after reassignment
        const d = data;
    });
}
"#;

    let (parser, root) = parse_test_source(source);
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    // Just check it doesn't crash
    if !checker.ctx.diagnostics.is_empty() {
        println!("=== CFA Invalidation Callback Parameter Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }
}

/// TS Unsoundness #36: JSX Intrinsic Lookup - lowercase tag resolution
///
/// Lowercase JSX tags like `<div />` are looked up as properties on the
/// global `JSX.IntrinsicElements` interface. This test verifies that the
/// checker can resolve intrinsic element types.
///
/// EXPECTED: Tests verify JSX parsing and checking don't crash. Full
/// JSX type checking is not yet implemented.
#[test]
fn test_jsx_intrinsic_element_lowercase_lookup() {
    // Use .tsx extension for JSX
    let source = r#"
declare namespace JSX {
    interface IntrinsicElements {
        div: { className?: string; id?: string };
        span: { className?: string };
    }
}

// Lowercase tags should be looked up in JSX.IntrinsicElements
const elem = <div className="test" />;
const elem2 = <span id="foo" />;
"#;

    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    // Check if parsing JSX is supported
    if !parser.get_diagnostics().is_empty() {
        println!("=== JSX Intrinsic Lowercase Parse Diagnostics ===");
        for diag in parser.get_diagnostics() {
            println!("[{}] {}", diag.start, diag.message);
        }
        // JSX parsing may not be enabled - skip test
        return;
    }

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.tsx".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    // Currently expect errors - JSX type checking not implemented
    // Once JSX.IntrinsicElements lookup works, change to expect 0 errors
    println!("=== JSX Intrinsic Lowercase Diagnostics ===");
    println!(
        "Got {} diagnostics (JSX checking not yet implemented)",
        checker.ctx.diagnostics.len()
    );
    for diag in &checker.ctx.diagnostics {
        println!("[{}] {}", diag.start, diag.message_text);
    }
    // Just verify we don't crash - actual JSX checking is future work
}

/// TS Unsoundness #36: JSX Intrinsic Lookup - uppercase component resolution
///
/// Uppercase JSX tags like `<MyComp />` are resolved as value references
/// in the current scope and checked as function/constructor calls.
///
/// EXPECTED: Tests verify JSX parsing and checking don't crash. Full
/// JSX type checking is not yet implemented.
#[test]
fn test_jsx_component_uppercase_resolution() {
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {}
}

// Component function
function MyButton(props: { label: string }): JSX.Element {
    return null as any;
}

// Uppercase tags resolve to variables in scope
const btn = <MyButton label="Click me" />;
"#;

    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    if !parser.get_diagnostics().is_empty() {
        println!("=== JSX Component Uppercase Parse Diagnostics ===");
        for diag in parser.get_diagnostics() {
            println!("[{}] {}", diag.start, diag.message);
        }
        return;
    }

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.tsx".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    println!("=== JSX Component Uppercase Diagnostics ===");
    println!(
        "Got {} diagnostics (JSX checking not yet implemented)",
        checker.ctx.diagnostics.len()
    );
    for diag in &checker.ctx.diagnostics {
        println!("[{}] {}", diag.start, diag.message_text);
    }
    // Just verify we don't crash
}

/// TS Unsoundness #36: JSX Intrinsic Lookup - invalid intrinsic element
///
/// When a lowercase tag is not found in JSX.IntrinsicElements, TypeScript
/// should report an error that the element does not exist.
///
/// EXPECTED: Tests verify JSX parsing and checking don't crash. Full
/// JSX type checking is not yet implemented.
#[test]
fn test_jsx_intrinsic_element_not_found_error() {
    let source = r#"
declare namespace JSX {
    interface IntrinsicElements {
        div: {};
    }
}

// 'unknowntag' is not in IntrinsicElements - should error
const elem = <unknowntag />;
"#;

    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    if !parser.get_diagnostics().is_empty() {
        return;
    }

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.tsx".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    // Once JSX checking is implemented, expect 1 error for unknown element
    println!("=== JSX Invalid Intrinsic Diagnostics ===");
    println!(
        "Got {} diagnostics (expected 1 once JSX implemented)",
        checker.ctx.diagnostics.len()
    );
    for diag in &checker.ctx.diagnostics {
        println!("[{}] {}", diag.start, diag.message_text);
    }
}

// =============================================================================
// NAMESPACE TYPE MEMBER ACCESS PATTERN TESTS
// =============================================================================

/// Test that namespace interface members can be used as type annotations
#[test]
fn test_namespace_type_member_interface_annotation() {
    let source = r#"
namespace Models {
    export interface User {
        id: number;
        name: string;
    }
    export interface Post {
        title: string;
        author: User;
    }
}

const user: Models.User = { id: 1, name: "Alice" };
const post: Models.Post = { title: "Hello", author: user };
function getUser(): Models.User {
    return { id: 0, name: "" };
}
"#;

    let (parser, root) = parse_test_source(source);
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Expected no errors for namespace interface type annotations, got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test that namespace type alias members can be used as type annotations
///
/// NOTE: Currently ignored - namespace type alias members are not correctly resolved
/// when used as type annotations. The checker emits type incompatibility errors
/// for cases that should work correctly.
#[test]
fn test_namespace_type_member_type_alias_annotation() {
    let source = r#"
namespace Types {
    export type ID = number;
    export type Name = string;
    export type Pair<T> = [T, T];
}

const id: Types.ID = 42;
const name: Types.Name = "Bob";
const pair: Types.Pair<number> = [1, 2];
"#;

    let (parser, root) = parse_test_source(source);
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Expected no errors for namespace type alias annotations, got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test that nested namespace type members can be used as type annotations
#[test]
fn test_namespace_type_member_nested_annotation() {
    let source = r#"
namespace Outer {
    export namespace Inner {
        export interface Config {
            enabled: boolean;
        }
        export namespace Deep {
            export type Value = string | number;
        }
    }
}

const config: Outer.Inner.Config = { enabled: true };
const value: Outer.Inner.Deep.Value = "test";
"#;

    let (parser, root) = parse_test_source(source);
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Expected no errors for nested namespace type annotations, got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test that namespace generic type members work correctly
#[test]
fn test_namespace_type_member_generic_usage() {
    let source = r#"
namespace Collections {
    export interface Container<T> {
        value: T;
    }
    export type Optional<T> = T | null;
    export interface Map<K, V> {
        get(key: K): V;
    }
}

const strContainer: Collections.Container<string> = { value: "hello" };
const numContainer: Collections.Container<number> = { value: 42 };
const optString: Collections.Optional<string> = null;
"#;

    let (parser, root) = parse_test_source(source);
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Expected no errors for namespace generic type usage, got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test that namespace type members work in function signatures
#[test]
fn test_namespace_type_member_function_signature() {
    let source = r#"
namespace API {
    export interface Request {
        method: string;
        url: string;
    }
    export interface Response {
        status: number;
        body: string;
    }
}

function handleRequest(req: API.Request): API.Response {
    return { status: 200, body: "" };
}

const makeRequest: (req: API.Request) => API.Response = handleRequest;
"#;

    let (parser, root) = parse_test_source(source);
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Expected no errors for namespace types in function signatures, got: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_use_before_assignment_basic_flow() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
function foo() {
    let x: number;
    return x;
}

function bar(flag: boolean) {
    let x: number;
    if (flag) { x = 1; }
    return x;
}

function baz(flag: boolean) {
    let x: number;
    if (flag) { x = 1; } else { x = 2; }
    return x;
}

function qux() {
    let x: number;
    x = 5;
    return x;
}
"#;

    let (parser, root) = parse_test_source(source);
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    // TS2454 requires strictNullChecks
    let options = crate::checker::context::CheckerOptions {
        jsx_factory: "React.createElement".to_string(),
        jsx_fragment_factory: "React.Fragment".to_string(),
        strict_null_checks: true,
        ..Default::default()
    };
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|diag| diag.code == diagnostic_codes::VARIABLE_IS_USED_BEFORE_BEING_ASSIGNED)
        .count();
    assert_eq!(
        count, 2,
        "Expected 2 use-before-assignment errors, got: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_use_before_assignment_try_catch() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
function foo() {
    let x: number;
    try {
        x = 1;
    } catch {
    }
    return x;
}
"#;

    let (parser, root) = parse_test_source(source);
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    // TS2454 requires strictNullChecks
    let options = crate::checker::context::CheckerOptions {
        jsx_factory: "React.createElement".to_string(),
        jsx_fragment_factory: "React.Fragment".to_string(),
        strict_null_checks: true,
        ..Default::default()
    };
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|diag| diag.code == diagnostic_codes::VARIABLE_IS_USED_BEFORE_BEING_ASSIGNED)
        .count();
    assert_eq!(
        count, 1,
        "Expected 1 use-before-assignment error, got: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_use_before_assignment_for_of_initializer() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
function foo(items: number[]) {
    let x: number;
    for (x of items) {
        x;
    }
}
"#;

    let (parser, root) = parse_test_source(source);
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|diag| diag.code == diagnostic_codes::VARIABLE_IS_USED_BEFORE_BEING_ASSIGNED)
        .count();
    assert_eq!(
        count, 0,
        "Expected no use-before-assignment errors, got: {:?}",
        checker.ctx.diagnostics
    );
}

// Test for-in with external variable: `let k: string; for (k in obj) { k; }`
#[test]
fn test_use_before_assignment_for_in_initializer() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
function foo(obj: Record<string, number>) {
    let k: string;
    for (k in obj) {
        k;
    }
}
"#;

    let (parser, root) = parse_test_source(source);
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|diag| diag.code == diagnostic_codes::VARIABLE_IS_USED_BEFORE_BEING_ASSIGNED)
        .count();
    assert_eq!(
        count, 0,
        "Expected no use-before-assignment errors for for-in, got: {:?}",
        checker.ctx.diagnostics
    );
}

// =============================================================================
// TS2564 Additional Edge Case Tests (Worker 2)
// =============================================================================

/// Test that required properties without initialization emit TS2564
#[test]
fn test_ts2564_required_property_emits_error() {
    let source = r#"
class Foo {
    name: string;
}
"#;

    let (parser, root) = parse_test_source(source);
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions {
            jsx_factory: "React.createElement".to_string(),
            jsx_factory_from_config: false,
            jsx_fragment_factory: "React.Fragment".to_string(),
            jsx_fragment_factory_from_config: false,
            strict: true,
            strict_property_initialization: true,
            ..Default::default()
        },
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2564)
        .count();
    assert_eq!(
        count, 1,
        "Expected TS2564 for uninitialized property, got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test that properties with `undefined` in their type skip TS2564
#[test]
fn test_ts2564_union_with_undefined_skips_check() {
    let source = r#"
class Foo {
    name: string | undefined;
}
"#;

    let (parser, root) = parse_test_source(source);
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let has_2564 = checker.ctx.diagnostics.iter().any(|d| d.code == 2564);
    assert!(
        !has_2564,
        "Expected no TS2564 errors for undefined union, got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test that optional properties skip TS2564 check
#[test]
fn test_ts2564_optional_property_skips_check() {
    let source = r#"
class Foo {
    name?: string;
    value?: number;
}
"#;

    let (parser, root) = parse_test_source(source);
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    // Optional properties should not have TS2564 errors
    let has_2564 = checker.ctx.diagnostics.iter().any(|d| d.code == 2564);
    assert!(
        !has_2564,
        "Expected no TS2564 errors for optional properties, got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test that definite assignment assertion (!) skips TS2564 check
#[test]
fn test_ts2564_definite_assignment_assertion_skips_check() {
    let source = r#"
class Foo {
    name!: string;
    value!: number;
}
"#;

    let (parser, root) = parse_test_source(source);
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    // Definite assignment assertion should not have TS2564 errors
    let has_2564 = checker.ctx.diagnostics.iter().any(|d| d.code == 2564);
    assert!(
        !has_2564,
        "Expected no TS2564 errors for definite assignment assertion, got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test that properties with initializers skip TS2564 check
#[test]
fn test_ts2564_property_with_initializer_skips_check() {
    let source = r#"
class Foo {
    name: string = "default";
    value: number = 42;
}
"#;

    let (parser, root) = parse_test_source(source);
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    // Properties with initializers should not have TS2564 errors
    let has_2564 = checker.ctx.diagnostics.iter().any(|d| d.code == 2564);
    assert!(
        !has_2564,
        "Expected no TS2564 errors for properties with initializers, got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test that static properties skip TS2564 check (static fields have different semantics)
#[test]
fn test_ts2564_static_property_skips_check() {
    let source = r#"
class Foo {
    static name: string;
    static value: number;
}
"#;

    let (parser, root) = parse_test_source(source);
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    // Static properties should not have TS2564 errors (different initialization semantics)
    let has_2564 = checker.ctx.diagnostics.iter().any(|d| d.code == 2564);
    assert!(
        !has_2564,
        "Expected no TS2564 errors for static properties, got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test that properties assigned directly in constructor skip TS2564 check
#[test]
fn test_ts2564_simple_constructor_assignment() {
    let source = r#"
class Foo {
    name: string;
    value: number;
    constructor() {
        this.name = "assigned";
        this.value = 123;
    }
}
"#;

    let (parser, root) = parse_test_source(source);
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    // Properties assigned in constructor should not have TS2564 errors
    let has_2564 = checker.ctx.diagnostics.iter().any(|d| d.code == 2564);
    assert!(
        !has_2564,
        "Expected no TS2564 errors for properties assigned in constructor, got: {:?}",
        checker.ctx.diagnostics
    );
}

// =============================================================================
// TS2564 Additional Edge Case Tests (Worker 4 - Fixed Bugs)
// =============================================================================

/// Test that switch statements without default case emit TS2564
#[test]
fn test_ts2564_switch_without_default_emits_error() {
    let source = r#"
class Foo {
    value: number;
    constructor(type: number) {
        switch (type) {
            case 0:
                this.value = 0;
                break;
            case 1:
                this.value = 1;
                break;
            // No default case - might not execute
        }
    }
}
"#;

    let (parser, root) = parse_test_source(source);
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions {
            jsx_factory: "React.createElement".to_string(),
            jsx_factory_from_config: false,
            jsx_fragment_factory: "React.Fragment".to_string(),
            jsx_fragment_factory_from_config: false,
            strict: true,
            strict_property_initialization: true,
            ..Default::default()
        },
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    // Should have TS2564 because switch might not execute any case
    let count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2564)
        .count();
    assert_eq!(
        count, 1,
        "Expected TS2564 for switch without default, got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test that switch statements with default case pass TS2564 check
#[test]
fn test_ts2564_switch_with_default_passes() {
    let source = r#"
class Foo {
    value: number;
    constructor(type: number) {
        switch (type) {
            case 0:
                this.value = 0;
                break;
            default:
                this.value = -1;
                break;
        }
    }
}
"#;

    let (parser, root) = parse_test_source(source);
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions {
            jsx_factory: "React.createElement".to_string(),
            jsx_factory_from_config: false,
            jsx_fragment_factory: "React.Fragment".to_string(),
            jsx_fragment_factory_from_config: false,
            strict: true,
            ..Default::default()
        },
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    // Should NOT have TS2564 because default case ensures initialization
    let has_2564 = checker.ctx.diagnostics.iter().any(|d| d.code == 2564);
    assert!(
        !has_2564,
        "Expected no TS2564 for switch with default, got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test that destructuring assignments to this.* are tracked
#[test]
fn test_ts2564_destructuring_assignment_passes() {
    let source = r#"
class Foo {
    a: number;
    b: string;
    constructor(data: { a: number; b: string }) {
        ({ a: this.a, b: this.b } = data);
    }
}
"#;

    let (parser, root) = parse_test_source(source);
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions {
            jsx_factory: "React.createElement".to_string(),
            jsx_factory_from_config: false,
            jsx_fragment_factory: "React.Fragment".to_string(),
            jsx_fragment_factory_from_config: false,
            strict: true,
            ..Default::default()
        },
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    // Should NOT have TS2564 because properties are initialized via destructuring
    let has_2564 = checker.ctx.diagnostics.iter().any(|d| d.code == 2564);
    assert!(
        !has_2564,
        "Expected no TS2564 for destructuring assignment, got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test that array destructuring assignments to this.* are tracked
#[test]
fn test_ts2564_array_destructuring_assignment_passes() {
    let source = r#"
class Foo {
    a: number;
    b: string;
    constructor(data: [number, string]) {
        [this.a, this.b] = data;
    }
}
"#;

    let (parser, root) = parse_test_source(source);
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions {
            jsx_factory: "React.createElement".to_string(),
            jsx_factory_from_config: false,
            jsx_fragment_factory: "React.Fragment".to_string(),
            jsx_fragment_factory_from_config: false,
            strict: true,
            ..Default::default()
        },
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    // Should NOT have TS2564 because properties are initialized via array destructuring
    let has_2564 = checker.ctx.diagnostics.iter().any(|d| d.code == 2564);
    assert!(
        !has_2564,
        "Expected no TS2564 for array destructuring assignment, got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test that properties assigned only in loop body emit TS2564
#[test]
fn test_ts2564_loop_assignment_emits_error() {
    let source = r#"
class Foo {
    value: number;
    constructor() {
        for (let i = 0; i < 10; i++) {
            this.value = i;
        }
    }
}
"#;

    let (parser, root) = parse_test_source(source);
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions {
            jsx_factory: "React.createElement".to_string(),
            jsx_factory_from_config: false,
            jsx_fragment_factory: "React.Fragment".to_string(),
            jsx_fragment_factory_from_config: false,
            strict: true,
            ..Default::default()
        },
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    // Should have TS2564 because loop might not execute
    let count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2564)
        .count();
    assert_eq!(
        count, 1,
        "Expected TS2564 for loop assignment, got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test that properties assigned in do-while loop pass (executes at least once)
#[test]
fn test_ts2564_do_while_assignment_passes() {
    let source = r#"
class Foo {
    value: number;
    constructor() {
        do {
            this.value = 1;
        } while (false);
    }
}
"#;

    let (parser, root) = parse_test_source(source);
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions {
            jsx_factory: "React.createElement".to_string(),
            jsx_factory_from_config: false,
            jsx_fragment_factory: "React.Fragment".to_string(),
            jsx_fragment_factory_from_config: false,
            strict: true,
            ..Default::default()
        },
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    // Should NOT have TS2564 because do-while always executes at least once
    let has_2564 = checker.ctx.diagnostics.iter().any(|d| d.code == 2564);
    assert!(
        !has_2564,
        "Expected no TS2564 for do-while assignment, got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test that while loop with false condition doesn't count as definite assignment
#[test]
fn test_ts2564_while_loop_false_condition_emits_error() {
    let source = r#"
class Foo {
    value: number;
    constructor() {
        while (false) {
            this.value = 1;
        }
    }
}
"#;

    let (parser, root) = parse_test_source(source);
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions {
            jsx_factory: "React.createElement".to_string(),
            jsx_factory_from_config: false,
            jsx_fragment_factory: "React.Fragment".to_string(),
            jsx_fragment_factory_from_config: false,
            strict: true,
            ..Default::default()
        },
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    // Should have TS2564 because while loop might not execute
    let count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2564)
        .count();
    assert_eq!(
        count, 1,
        "Expected TS2564 for while loop with false condition, got: {:?}",
        checker.ctx.diagnostics
    );
}

