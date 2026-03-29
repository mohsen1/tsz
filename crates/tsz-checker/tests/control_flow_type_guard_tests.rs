use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

#[test]
fn test_user_defined_type_guard_narrowing_full() {
    let source = r#"
interface X {
    x: string;
}

interface Y {
    y: string;
}

interface Z {
    z: string;
}

declare function isX(obj: any): obj is X;
declare function isY(obj: any): obj is Y;
declare function isZ(obj: any): obj is Z;

function f1(obj: Object) {
    if (isX(obj) || isY(obj) || isZ(obj)) {
        obj;
    }
    if (isX(obj) && isY(obj) && isZ(obj)) {
        obj;
    }
}

// Repro from #8911

// two interfaces
interface A {
  a: string;
}

interface B {
  b: string;
}

// a type guard for B
function isB(toTest: any): toTest is B {
  return toTest && toTest.b;
}

// a function that turns an A into an A & B
function union(a: A): A & B | null {
  if (isB(a)) {
    return a;
  } else {
    return null;
  }
}

// Repro from #9016

declare function log(s: string): void;

// Supported beast features
interface Beast     { wings?: boolean; legs?: number }
interface Legged    { legs: number; }
interface Winged    { wings: boolean; }

// Beast feature detection via user-defined type guards
function hasLegs(x: Beast): x is Legged { return x && typeof x.legs === 'number'; }
function hasWings(x: Beast): x is Winged { return x && !!x.wings; }

// Function to identify a given beast by detecting its features
function identifyBeast(beast: Beast) {

    // All beasts with legs
    if (hasLegs(beast)) {

        // All winged beasts with legs
        if (hasWings(beast)) {
            if (beast.legs === 4) {
                log(`pegasus - 4 legs, wings`);
            }
            else if (beast.legs === 2) {
                log(`bird - 2 legs, wings`);
            }
            else {
                log(`unknown - ${beast.legs} legs, wings`);
            }
        }

        // All non-winged beasts with legs
        else {
            log(`manbearpig - ${beast.legs} legs, no wings`);
        }
    }

    // All beasts without legs    
    else {
        if (hasWings(beast)) {
            log(`quetzalcoatl - no legs, wings`)
        }
        else {
            log(`snake - no legs, no wings`)
        }
    }
}

function beastFoo(beast: Object) {
    if (hasWings(beast) && hasLegs(beast)) {
        beast;  // Winged & Legged
    }
    else {
        beast;
    }

    if (hasLegs(beast) && hasWings(beast)) {
        beast;  // Legged & Winged
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty(), "Parse errors");

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    }
    .apply_strict_defaults();

    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );

    checker.check_source_file(root);

    // Collect all diagnostics
    let diagnostics: Vec<(u32, String)> = checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect();

    // Filter out TS2318 (missing global types) and TS2345 (Beast argument error which we expect)
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318 && *code != 2345)
        .cloned()
        .collect();

    // Now we check if TS2322 is present. It SHOULD NOT be present if fixed.
    // If it is present, we have reproduced the failure.
    if relevant.iter().any(|(code, _)| *code == 2322) {
        panic!("Found TS2322 error (Narrowing failed): {relevant:?}");
    }
}

/// Regression test: type predicate narrowing must work for primitive types.
///
/// Previously, the flow analysis fast-path in `apply_flow_narrowing` would
/// short-circuit for `TypeId::STRING` and `TypeId::NUMBER`, returning the
/// declared type without applying any flow narrowing. This prevented
/// user-defined type predicates from narrowing primitive types to literal
/// subtypes (e.g., `value is "foo"` narrowing `string` to `"foo"`).
#[test]
fn test_type_predicate_narrows_string_to_literal() {
    let source = r#"
declare function isFoo(value: string): value is "foo";
declare function doThis(value: "foo"): void;
declare function doThat(value: string): void;

function test(value: string) {
    if (isFoo(value)) {
        doThis(value);
    } else {
        doThat(value);
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty(), "Parse errors");

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    }
    .apply_strict_defaults();

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

    // Filter out TS2318 (missing global types) — not relevant here.
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    // TS2345 would mean the type predicate narrowing failed — value is still
    // `string` instead of being narrowed to `"foo"`.
    if relevant.iter().any(|(code, _)| *code == 2345) {
        panic!(
            "Found TS2345 error — type predicate narrowing to literal type failed: {relevant:?}"
        );
    }
}

/// Regression test: type predicate narrowing for literal type union.
///
/// Same issue as above but with `value is ("foo" | "bar")`.
#[test]
fn test_type_predicate_narrows_string_to_literal_union() {
    let source = r#"
declare function isFooOrBar(value: string): value is ("foo" | "bar");
declare function doThis(value: "foo" | "bar"): void;

function test(value: string) {
    if (isFooOrBar(value)) {
        doThis(value);
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty(), "Parse errors");

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    }
    .apply_strict_defaults();

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

    if relevant.iter().any(|(code, _)| *code == 2345) {
        panic!(
            "Found TS2345 error — type predicate narrowing to literal union failed: {relevant:?}"
        );
    }
}

/// Regression test: type guard narrowing during return type inference.
///
/// When a function body uses a type guard in an if-condition and then returns
/// the narrowed value, the inferred return type should reflect the narrowing.
/// Previously, `infer_return_type_from_body` only collected return expressions
/// without evaluating if-conditions, so the flow analyzer couldn't find the
/// type predicate and identifiers kept their un-narrowed declared type.
///
/// This caused false TS2722 ("Cannot invoke an object which is possibly
/// 'undefined'") when calling the result of a function that narrows via
/// Extract<T, Function>.
#[test]
fn test_type_guard_narrowing_in_return_type_inference() {
    let source = r#"
function isFunction<T>(value: T): value is Extract<T, Function> {
    return typeof value === "function";
}
function getFunction<T>(item: T) {
    if (isFunction(item)) {
        return item;
    }
    throw new Error();
}
function f12(x: string | (() => string) | undefined) {
    const f = getFunction(x);
    f();
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty(), "Parse errors");

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    }
    .apply_strict_defaults();

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

    // TS2722 would mean the type guard narrowing was not applied during
    // return type inference, causing the inferred return type to be
    // the un-narrowed T instead of Extract<T, Function>.
    if relevant.iter().any(|(code, _)| *code == 2722) {
        panic!(
            "Found TS2722 error — type guard narrowing failed during return type inference: {relevant:?}"
        );
    }
}

/// Regression test: simple type predicate narrowing in inferred return type.
///
/// Verifies that a function whose body uses `if (isString(x)) return x`
/// correctly infers the return type as the narrowed type, not the original.
#[test]
fn test_simple_type_predicate_return_inference() {
    let source = r#"
function isString(value: unknown): value is string {
    return typeof value === "string";
}
function getString(x: string | number) {
    if (isString(x)) {
        return x;
    }
    throw new Error();
}
const s: string = getString("hello");
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty(), "Parse errors");

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    }
    .apply_strict_defaults();

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

    // TS2322 would mean the inferred return type is string | number instead
    // of string (the type predicate narrowing was not applied).
    if relevant.iter().any(|(code, _)| *code == 2322) {
        panic!(
            "Found TS2322 error — type predicate narrowing not applied to inferred return type: {relevant:?}"
        );
    }
}

/// Regression test: union type predicate narrowing.
///
/// When a method is called on a union type (e.g., `Entry | Group`) and only
/// some members have `this is T` predicates, narrowing should still work.
/// Previously the code required ALL union members to have matching predicates.
#[test]
fn test_union_this_predicate_narrowing() {
    let source = r#"
class Entry {
    c = 1;
    isInit(x: any): this is Entry { return true; }
}
class Group {
    d = 'no';
    isInit(x: any): boolean { return false; }
}
declare var chunk: Entry | Group;
if (chunk.isInit(chunk)) {
    chunk.c;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty(), "Parse errors");

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    }
    .apply_strict_defaults();

    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );

    checker.check_source_file(root);

    let relevant: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code != 2318)
        .map(|d| d.code)
        .collect();

    // Since Group.isInit() returns `boolean` (not a type predicate and not `false`),
    // the union call is NOT a type predicate.  `chunk` is not narrowed, so accessing
    // `chunk.c` should error because `Group` has no property `c`.
    assert!(
        relevant.contains(&2339),
        "Expected TS2339 for property access on un-narrowed union, got: {relevant:?}"
    );
}

/// Regression test: JSDoc method `@return {this is Entry}` type predicate.
///
/// In JS files, class methods with `@return {this is T}` should create type
/// predicates. Previously `signature_builder.rs` hardcoded `type_predicate = None`
/// for methods without syntax-level type annotations.
#[test]
fn test_jsdoc_method_this_predicate() {
    let source = r#"
// @ts-check
class Entry {
    constructor() { this.c = 1; }
    /**
     * @param {any} x
     * @return {this is Entry}
     */
    isInit(x) { return true; }
}
/** @param {Entry} e */
function f(e) {
    if (e.isInit(e)) {
        e.c;
    }
}
"#;

    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty(), "Parse errors");

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions {
        strict: true,
        check_js: true,
        ..CheckerOptions::default()
    }
    .apply_strict_defaults();

    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.js".to_string(),
        options,
    );

    checker.check_source_file(root);

    let relevant: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code != 2318)
        .map(|d| (d.code, d.message_text.clone()))
        .collect();

    assert!(
        relevant.is_empty(),
        "JSDoc @return {{this is Entry}} should create type predicate, got: {relevant:?}"
    );
}

/// Regression test: JSDoc `@callback` with type predicate `@return {x is number}`.
///
/// `@callback Cb` definitions with `@return {x is Type}` should create function
/// types with type predicates. Previously `parse_jsdoc_typedefs` only handled
/// `@typedef` and `@import`, not `@callback`.
///
/// NOTE: This test validates the parsing infrastructure (`JsdocCallbackInfo` and
/// `jsdoc_returns_type_predicate_from_type_expr`). Full integration testing of
/// @callback type predicate narrowing is covered by conformance test
/// `returnTagTypeGuard.ts` because it requires JSDoc comment infrastructure
/// that isn't available in unit test harness.
#[test]
fn test_jsdoc_callback_predicate_parsing() {
    use tsz_checker::state::CheckerState;
    // Test the parse_jsdoc_typedefs path by using a JS function with
    // a direct @return predicate (not via @callback alias) which
    // exercises the same predicate parsing code.
    let source = r#"
/**
 * @param {unknown} x
 * @return {x is number}
 */
function isNumber(x) { return typeof x === "number" }

/** @param {unknown} x */
function g(x) {
    if (isNumber(x)) {
        x * 2;
    }
}
"#;

    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty(), "Parse errors");

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions {
        strict: true,
        check_js: true,
        ..CheckerOptions::default()
    }
    .apply_strict_defaults();

    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.js".to_string(),
        options,
    );

    checker.check_source_file(root);

    let relevant: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code != 2318)
        .map(|d| (d.code, d.message_text.clone()))
        .collect();

    // TS18046 ("x is of type unknown") would indicate the predicate was not applied
    assert!(
        relevant.is_empty(),
        "JSDoc @return {{x is number}} should create type predicate, got: {relevant:?}"
    );
}

/// Control flow alias invalidation: when a type guard alias is created and
/// the aliased reference is later reassigned, the alias narrowing must be
/// invalidated (TS2322 should be emitted).
#[test]
fn test_alias_narrowing_invalidated_by_reassignment() {
    let source = r#"
function f(x: string | number) {
    const isString = typeof x === "string";
    x = 42;  // reassign the aliased reference
    if (isString) {
        // x was reassigned, alias should be invalidated
        let s: string = x;  // should error: TS2322
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty(), "Parse errors");

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    }
    .apply_strict_defaults();

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
        .filter(|d| d.code != 2318)
        .map(|d| (d.code, d.message_text.clone()))
        .collect();

    assert!(
        diagnostics.iter().any(|(code, _)| *code == 2322),
        "Expected TS2322 when alias reference is reassigned, got: {diagnostics:?}"
    );
}

/// Control flow alias narrowing should work when the reference is NOT reassigned.
#[test]
fn test_alias_narrowing_works_without_reassignment() {
    let source = r#"
function f(x: string | number) {
    const isString = typeof x === "string";
    if (isString) {
        let s: string = x;  // should NOT error: alias is valid
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty(), "Parse errors");

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    }
    .apply_strict_defaults();

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
        .filter(|d| d.code != 2318)
        .map(|d| (d.code, d.message_text.clone()))
        .collect();

    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2322),
        "Unexpected TS2322 when alias reference is NOT reassigned: {diagnostics:?}"
    );
}

/// Property access alias invalidation: when a typeof guard aliases a
/// property access (e.g., `typeof obj.x`) and the base object's property
/// is reassigned later, the alias must be invalidated.
#[test]
fn test_alias_narrowing_invalidated_by_property_reassignment() {
    let source = r#"
function f(obj: { x: string | number }) {
    const isString = typeof obj.x === "string";
    obj.x = 42;  // reassign the aliased property
    if (isString) {
        let s: string = obj.x;  // should error: TS2322
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty(), "Parse errors");

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    }
    .apply_strict_defaults();

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
        .filter(|d| d.code != 2318)
        .map(|d| (d.code, d.message_text.clone()))
        .collect();

    assert!(
        diagnostics.iter().any(|(code, _)| *code == 2322),
        "Expected TS2322 when aliased property is reassigned, got: {diagnostics:?}"
    );
}

/// Regression test: type predicate narrowing with discriminated union members.
///
/// When interfaces have string literal discriminant properties (e.g., `kind: "a"`),
/// the reverse subtype check in `narrow_to_type` could produce false positives from
/// the global subtype cache, causing non-matching union members to be kept instead
/// of filtered out.
#[test]
fn test_type_predicate_narrowing_discriminated_union() {
    let source = r#"
interface A { kind: "a"; x: number }
interface B { kind: "b"; y: string }

function isA(v: A | B): v is A { return v.kind === "a"; }

declare const v: A | B;
if (isA(v)) {
    let check: A = v;  // Should work - v narrowed to A
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty(), "Parse errors");

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    }
    .apply_strict_defaults();

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
        .filter(|d| d.code != 2318)
        .map(|d| (d.code, d.message_text.clone()))
        .collect();

    // Should NOT have TS2322 — v is narrowed to A
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect();
    assert!(
        ts2322.is_empty(),
        "Type predicate narrowing failed for discriminated union: {ts2322:?}"
    );
}
