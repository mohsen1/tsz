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
