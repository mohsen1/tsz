use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;
fn parse_test_source(source: &str) -> (tsz_parser::ParserState, tsz_parser::parser::NodeIndex) {
    let mut parser = tsz_parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    (parser, root)
}

fn strict_diagnostics(source: &str) -> Vec<(u32, String)> {
    let options = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    }
    .apply_strict_defaults();

    tsz_checker::test_utils::check_source(source, "test.ts", options)
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

fn checked_js_diagnostics(source: &str) -> Vec<(u32, String)> {
    let options = CheckerOptions {
        strict: true,
        allow_js: true,
        check_js: true,
        ..CheckerOptions::default()
    }
    .apply_strict_defaults();

    tsz_checker::test_utils::check_source(source, "test.js", options)
        .into_iter()
        .filter(|d| d.code != 2318)
        .map(|d| (d.code, d.message_text))
        .collect()
}

fn strict_diagnostics_with_libs(source: &str) -> Vec<(u32, String)> {
    let options = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    }
    .apply_strict_defaults();
    let lib_files = tsz_checker::test_utils::load_lib_files(&["es5.d.ts"]);

    tsz_checker::test_utils::check_source_with_libs(source, "test.ts", options, &lib_files)
        .into_iter()
        .filter(|d| d.code != 2318 && d.code != 6133)
        .map(|d| (d.code, d.message_text))
        .collect()
}

#[test]
fn nested_or_right_operand_preserves_false_path_narrowing() {
    let diagnostics = strict_diagnostics(
        r#"
function f(x: number | string | boolean) {
    let y: number | string | boolean;
    let z: number | string | boolean;
    return typeof x === "string"
        || ((z = x)
        || (typeof x === "number"
        ? ((x = 10) && x.toString())
        : ((y = x) && x.toString())));
}
"#,
    );

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2339 && message == "Property 'toString' does not exist on type 'never'."
        }),
        "expected the conformance TS2339 for `x.toString()` narrowed to never, got: {diagnostics:?}"
    );
}

#[test]
fn shadowed_builtin_guard_names_do_not_narrow() {
    let diagnostics = strict_diagnostics(
        r#"
interface ArrayBufferView {
    byteLength: number;
}

const Array = {
    isArray(_value: unknown): boolean {
        return true;
    },
};

declare let maybeArray: string | string[];
if (Array.isArray(maybeArray)) {
    const arrayOnly: string[] = maybeArray;
}

const ArrayBuffer = {
    isView(_value: unknown): boolean {
        return true;
    },
};

declare let maybeView: string | ArrayBufferView;
if (ArrayBuffer.isView(maybeView)) {
    const viewOnly: ArrayBufferView = maybeView;
}
"#,
    );

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2322
                && message == "Type 'string | string[]' is not assignable to type 'string[]'."
        }),
        "expected shadowed Array.isArray to avoid array narrowing: {diagnostics:#?}"
    );
    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2322
                && message
                    == "Type 'string | ArrayBufferView' is not assignable to type 'ArrayBufferView'."
        }),
        "expected shadowed ArrayBuffer.isView to avoid ArrayBufferView narrowing: {diagnostics:#?}"
    );
}

#[test]
fn overloaded_type_guard_uses_selected_predicate() {
    let diagnostics = strict_diagnostics(
        r#"
interface S {
  s: string;
}
interface N {
  n: number;
}

function guard(x: unknown): x is S;
function guard(x: unknown, flag: true): x is N;
function guard(_x: unknown, _flag?: true): _x is S | N {
  return true;
}
function takesS(value: S): void {
  value;
}
function takesN(value: N): void {
  value;
}

let value = {} as S | N;

if (guard(value, true)) {
  takesN(value);
  takesS(value);
}
"#,
    );

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2345
                && message == "Argument of type 'N' is not assignable to parameter of type 'S'."
        }),
        "expected selected two-argument overload to narrow value to N: {diagnostics:#?}"
    );
    assert!(
        diagnostics.iter().all(|(code, message)| {
            *code != 2345
                || message != "Argument of type 'S' is not assignable to parameter of type 'N'."
        }),
        "overloaded type guard used the first predicate instead of the selected overload: {diagnostics:#?}"
    );
}

#[test]
fn instanceof_symbol_hasinstance_generic_predicate_erases_to_any() {
    let source = r#"
interface SymbolConstructor {
    readonly hasInstance: unique symbol;
}
declare var Symbol: SymbolConstructor;

interface BConstructor {
    new <T>(): B<T>;
    [Symbol.hasInstance](value: unknown): value is B<any>;
}
interface B<T> {
    foo: T;
}
declare var B: BConstructor;

declare var obj3: B<number> | string;
if (obj3 instanceof B) {
    obj3.foo = 1;
    obj3.foo = "str";
    obj3.bar = "str";
}

declare var obj4: any;
if (obj4 instanceof B) {
    obj4.bar = "str";
}
"#;

    let diagnostics = strict_diagnostics(source);
    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2322 && message == "Type 'string' is not assignable to type 'number'."
        }),
        "expected assignment to preserve B<number> after instanceof, got: {diagnostics:#?}"
    );
    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2339 && message == "Property 'bar' does not exist on type 'B<number>'."
        }),
        "expected obj3.bar to see the narrowed B<number> type, got: {diagnostics:#?}"
    );
    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2339 && message == "Property 'bar' does not exist on type 'B<any>'."
        }),
        "expected obj4.bar to narrow any through B<any>, got: {diagnostics:#?}"
    );
    assert!(
        diagnostics
            .iter()
            .all(|(_, message)| !message.contains("string | B<number>")),
        "instanceof should not leave obj3 as the original union: {diagnostics:#?}"
    );
}

#[test]
fn typeof_primitive_checks_narrow_explicit_any_only_in_true_branch() {
    let diagnostics = strict_diagnostics(
        r#"
var x: any = { p: 0 };

if (x instanceof Object) {
    x.p;
} else {
    x.p;
}

if (typeof x === "string") {
    x.p;
} else {
    x.p;
}

if (typeof x === "number") {
    x.p;
} else {
    x.p;
}

if (typeof x === "boolean") {
    x.p;
} else {
    x.p;
}

if (typeof x === "object") {
    x.p;
} else {
    x.p;
}
"#,
    );

    let ts2339 = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .map(|(_, message)| message.as_str())
        .collect::<Vec<_>>();

    assert_eq!(
        ts2339.len(),
        3,
        "expected exactly the string/number/boolean true-branch TS2339 diagnostics, got: {diagnostics:#?}"
    );
    assert!(
        ts2339
            .iter()
            .any(|message| message.contains("type 'string'")),
        "expected string true-branch TS2339, got: {diagnostics:#?}"
    );
    assert!(
        ts2339
            .iter()
            .any(|message| message.contains("type 'number'")),
        "expected number true-branch TS2339, got: {diagnostics:#?}"
    );
    assert!(
        ts2339
            .iter()
            .any(|message| message.contains("type 'boolean'")),
        "expected boolean true-branch TS2339, got: {diagnostics:#?}"
    );
    assert!(
        ts2339.iter().all(|message| !message.contains("never")),
        "object/else branches must not narrow explicit any to never, got: {diagnostics:#?}"
    );
}

#[test]
fn homomorphic_mapped_type_preserves_null_in_primitive_union() {
    let source = r#"
type Narrowable = string | number | bigint | boolean;

type Narrow<A> = (A extends Narrowable ? A : never) | ({
    [K in keyof A]: Narrow<A[K]>;
});

const satisfies =
  <TWide,>() =>
  <TNarrow extends TWide>(narrow: Narrow<TNarrow>) =>
    narrow;

type Item = { value: string | null };

satisfies<Item>()({ value: null });
"#;

    let diagnostics = strict_diagnostics(source);
    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2322),
        "homomorphic mapped types should preserve null in primitive unions: {diagnostics:#?}"
    );
}

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

    let (parser, root) = parse_test_source(source);
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

#[test]
fn type_predicate_preserves_subclass_union_member_without_redundant_intersection() {
    let source = r#"
class C1 { p1!: string; }
class C2 { p2!: number; }
class D1 extends C1 { p3!: number; }

declare function isC1(x: any): x is C1;
declare function isC2(x: any): x is C2;
declare let c2OrD1: C2 | D1;

let n: number | false = isC2(c2OrD1) && c2OrD1.p2;
let r2: C2 | D1 = isC1(c2OrD1) && c2OrD1;
"#;

    let diagnostics = strict_diagnostics(source);
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect();

    assert_eq!(
        ts2322.len(),
        1,
        "expected one TS2322 from the falsy `&&` branch, got: {diagnostics:?}"
    );
    assert!(
        ts2322[0].1.contains("false | D1"),
        "expected predicate narrowing to display `false | D1`, got: {}",
        ts2322[0].1
    );
    assert!(
        !ts2322[0].1.contains('&'),
        "subclass member should not be displayed as a redundant intersection: {}",
        ts2322[0].1
    );
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

    let (parser, root) = parse_test_source(source);
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

    let (parser, root) = parse_test_source(source);
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

/// Top-level `let` declarations without initializers still receive the
/// predicate's narrowed type in guarded branches, even though TS2454 is also
/// reported for the unassigned read in the guard expression.
#[test]
fn test_top_level_type_predicate_narrows_string_to_literal() {
    let source = r#"
declare function isFoo(value: string): value is "foo";
declare function doThis(value: "foo"): void;
declare function doThat(value: string): void;

let value: string;
if (isFoo(value)) {
    doThis(value);
} else {
    doThat(value);
}
"#;

    let (parser, root) = parse_test_source(source);
    assert!(parser.get_diagnostics().is_empty(), "Parse errors");

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions {
        strict: true,
        no_implicit_returns: true,
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
        .filter(|(code, _)| *code != 2318)
        .collect();

    assert!(
        diagnostics.iter().any(|(code, _)| *code == 2454),
        "top-level unassigned guard read should still report TS2454, got: {diagnostics:?}"
    );
    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2345),
        "type predicate narrowing should prevent TS2345 in guarded branch, got: {diagnostics:?}"
    );
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

    let (parser, root) = parse_test_source(source);
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

    let (parser, root) = parse_test_source(source);
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

#[test]
fn test_generic_type_predicate_false_branch_does_not_collapse_to_never() {
    let source = r#"
type Result = { value: string };
type Results = Result[];

function isPlainResponse<T>(value: T | { data: T}): value is T {
    return !value.hasOwnProperty('data');
}

function getResults2(value: Results | { data: Results }): Results {
    return isPlainResponse(value) ? value : value.data;
}
"#;

    let (parser, root) = parse_test_source(source);
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

    let formatted_predicates: Vec<_> = checker
        .ctx
        .call_type_predicates
        .iter()
        .map(|(node, (predicate, params))| {
            (
                *node,
                predicate
                    .type_id
                    .map(|ty| checker.format_type(ty))
                    .unwrap_or_else(|| "<none>".to_string()),
                params
                    .iter()
                    .map(|param| {
                        (
                            param.name.map(|name| name.0),
                            checker.format_type(param.type_id),
                        )
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .collect();

    if relevant
        .iter()
        .any(|(_, message)| message.contains("type 'never'"))
    {
        panic!(
            "Found erroneous false-branch collapse to never for generic predicate: {relevant:?}; formatted_predicates={formatted_predicates:?}; call_type_predicates={:?}",
            checker.ctx.call_type_predicates,
        );
    }
}

#[test]
fn explicit_type_argument_instantiates_generic_type_predicate() {
    let source = r#"
function isArray<T>(x: unknown): x is T[] {
    return Array.isArray(x);
}

function useGenericPred(x: unknown) {
    if (isArray<number>(x)) {
        const _n: number[] = x;
    }
}
"#;

    let (parser, root) = parse_test_source(source);
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
        .map(|d| (d.code, d.message_text.clone()))
        .collect();

    assert!(
        !relevant.iter().any(|(code, _)| *code == 2322),
        "explicit type arguments should instantiate `x is T[]` as `number[]`: {relevant:?}"
    );
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

    let (parser, root) = parse_test_source(source);
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

#[test]
fn jsdoc_assertion_return_predicate_accepts_tab_whitespace() {
    let source = concat!(
        "// @ts-check\n",
        "\n",
        "/**\n",
        " * @param {unknown} value\n",
        " * @returns {asserts\tvalue\tis\tstring}\n",
        " */\n",
        "function assertString(value) {}\n",
        "\n",
        "/** @type {string | number} */\n",
        "let maybe = \"ok\";\n",
        "\n",
        "assertString(maybe);\n",
        "maybe.toUpperCase();\n",
    );

    let diagnostics = checked_js_diagnostics(source);

    assert!(
        diagnostics.is_empty(),
        "JSDoc @returns assertion predicates should accept tab whitespace: {diagnostics:#?}"
    );
}

#[test]
fn jsdoc_callback_assertion_predicate_accepts_tab_whitespace() {
    let source = concat!(
        "// @ts-check\n",
        "\n",
        "/**\n",
        " * @callback AssertString\n",
        " * @param {unknown} value\n",
        " * @returns {asserts\tvalue\tis\tstring}\n",
        " */\n",
        "\n",
        "/** @type {AssertString} */\n",
        "const assertString = (value) => {};\n",
        "\n",
        "/** @type {string | number} */\n",
        "let maybe = \"ok\";\n",
        "\n",
        "assertString(maybe);\n",
        "maybe.toUpperCase();\n",
    );

    let diagnostics = checked_js_diagnostics(source);

    assert!(
        diagnostics.is_empty(),
        "JSDoc @callback assertion predicates should accept tab whitespace: {diagnostics:#?}"
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

    let (parser, root) = parse_test_source(source);
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

    let (parser, root) = parse_test_source(source);
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

    let (parser, root) = parse_test_source(source);
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

#[test]
fn type_predicate_narrowing_does_not_leak_after_if_without_else() {
    let diagnostics = strict_diagnostics(
        r#"
function isNumber(value: unknown): value is number {
    return typeof value === "number";
}

function test(x: unknown) {
    if (isNumber(x)) {
        let n: number = x;
    }
    x.toFixed(2);
}
"#,
    );

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 18046 && message.contains("'x' is of type 'unknown'")
        }),
        "expected TS18046 after predicate branch rejoins with the original type, got: {diagnostics:?}"
    );
}

#[test]
fn renamed_type_predicate_narrowing_does_not_leak_after_if_else_join() {
    let diagnostics = strict_diagnostics(
        r#"
function keepsText(input: unknown): input is string {
    return typeof input === "string";
}

function use(candidate: unknown) {
    if (keepsText(candidate)) {
        let s: string = candidate;
    } else {
        let u: unknown = candidate;
    }
    candidate.trim();
}
"#,
    );

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 18046 && message.contains("'candidate' is of type 'unknown'")
        }),
        "expected TS18046 after both predicate branches can reach the join, got: {diagnostics:?}"
    );
}

#[test]
fn type_predicate_narrowing_survives_when_false_branch_terminates() {
    let diagnostics = strict_diagnostics(
        r#"
function isNumber(value: unknown): value is number {
    return typeof value === "number";
}

function test(x: unknown) {
    if (!isNumber(x)) {
        return;
    }
    let n: number = x;
}
"#,
    );

    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2322),
        "predicate narrowing should survive after terminating false branch, got: {diagnostics:?}"
    );
}

#[test]
fn exhaustive_typeof_chain_on_unknown_leaves_empty_object_residual() {
    let diagnostics = strict_diagnostics(
        r#"
function narrowUnknown(x: unknown) {
    if (typeof x === "string") return;
    if (typeof x === "number") return;
    if (typeof x === "boolean") return;
    if (typeof x === "undefined") return;
    if (typeof x === "object") return;
    if (typeof x === "function") return;
    if (typeof x === "symbol") return;
    if (typeof x === "bigint") return;

    const remaining: never = x;
    return remaining;
}
"#,
    );

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2322 && message.contains("Type '{}' is not assignable to type 'never'")
        }),
        "expected exhaustive typeof exclusions from unknown to leave {{}}, got: {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .all(|(_, message)| !message
                .contains("Type 'unknown' is not assignable to type 'never'")),
        "exhaustive typeof exclusions should not leave unknown, got: {diagnostics:?}"
    );
}

#[test]
fn exhaustive_typeof_chain_with_renamed_value_and_negated_conditions_leaves_empty_object() {
    let diagnostics = strict_diagnostics(
        r#"
function narrowCandidate(candidate: unknown) {
    if (!(typeof candidate !== "string")) return;
    if (!(typeof candidate !== "number")) return;
    if (!(typeof candidate !== "boolean")) return;
    if (!(typeof candidate !== "undefined")) return;
    if (!(typeof candidate !== "object")) return;
    if (!(typeof candidate !== "function")) return;
    if (!(typeof candidate !== "symbol")) return;
    if (!(typeof candidate !== "bigint")) return;

    const remaining: never = candidate;
    return remaining;
}
"#,
    );

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2322 && message.contains("Type '{}' is not assignable to type 'never'")
        }),
        "renamed negated typeof exclusions should leave {{}}, got: {diagnostics:?}"
    );
}

#[test]
fn partial_typeof_chain_on_unknown_stays_unknown() {
    let diagnostics = strict_diagnostics(
        r#"
function partial(x: unknown) {
    if (typeof x === "string") return;
    if (typeof x === "number") return;

    const remaining: never = x;
    return remaining;
}
"#,
    );

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2322 && message.contains("Type 'unknown' is not assignable to type 'never'")
        }),
        "partial typeof exclusions should keep unknown, got: {diagnostics:?}"
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

    let (parser, root) = parse_test_source(source);
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

/// Test that negated assertion with conditional type predicate narrows correctly.
/// When `assert(!isB(foo))` where `isB` uses a conditional type predicate like
/// `Extract<T, U>`, the false branch should exclude matching union members.
///
/// This exercises the `resolve_type_uncached` path for `Conditional` types in the
/// solver's narrowing context — ensuring inner Lazy types are resolved before
/// the conditional is evaluated/distributed.
#[test]
fn test_negated_assertion_with_conditional_type_predicate() {
    let source = r#"
type Foo = {type: 'A', a: number} | {type: 'B', b: number};
type MyExtract<T, U> = T extends U ? T : never;
declare function isB(x: Foo): x is MyExtract<Foo, {type: 'B'}>;
declare function assert(x: boolean): asserts x;

function test(foo: Foo): {type: 'A', a: number} {
    assert(!isB(foo));
    return foo;
}
"#;

    let (parser, root) = parse_test_source(source);
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

    // Should NOT have TS2322 — foo is narrowed to {type: 'A', a: number}
    // by excluding Extract<Foo, {type: 'B'}> = {type: 'B', b: number}
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect();
    assert!(
        ts2322.is_empty(),
        "Negated assertion with conditional type predicate should narrow correctly: {ts2322:?}"
    );
}

/// Regression test: generic type inference from type predicate literal types
/// should preserve the literal type, not widen it.
///
/// When calling `capture<V>(pred: (x: unknown) => x is V)` with a predicate
/// like `isB: (x: unknown) => x is 'B'`, V should be inferred as `'B'` (literal),
/// not widened to `string`. This matches tsc's behavior where types from type
/// annotations don't carry the `RequiresWidening` flag.
#[test]
fn test_generic_inference_preserves_literal_from_type_predicate() {
    let source = r#"
declare function capture<V>(predicate: (arg: unknown) => arg is V): V;
declare function isB(arg: unknown): arg is 'B';

const result = capture(isB);
const check: 'B' = result;
"#;

    let (parser, root) = parse_test_source(source);
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

    // TS2322 would mean V was widened from 'B' to string
    let ts2322: Vec<_> = relevant.iter().filter(|(code, _)| *code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "Generic inference from type predicate should preserve literal 'B', not widen to string: {ts2322:?}"
    );
}

/// Regression test: `this is DatafulFoo<T>` type predicate narrows `this` so
/// that property accesses use the narrowed interface members.
///
/// From conformance test `spreadObjectOrFalsy.ts`:
/// ```ts
/// interface DatafulFoo<T> { data: T; }
/// class Foo<T extends string> {
///     data: T | undefined;
///     bar() {
///         if (this.hasData()) {
///             this.data.toLocaleLowerCase(); // NO TS2532
///         }
///     }
///     hasData(): this is DatafulFoo<T> { return true; }
/// }
/// ```
///
/// After narrowing, `this.data` should be `T` (from `DatafulFoo<T>`),
/// not `T | undefined` (from `Foo<T>`). TS2532 must not be emitted.
#[test]
fn test_this_type_predicate_narrows_property_type() {
    let source = r#"
interface DatafulFoo<T> {
    data: T;
}

class Foo<T extends string> {
    data: T | undefined;
    bar() {
        if (this.hasData()) {
            this.data.toLocaleLowerCase();
        }
    }
    hasData(): this is DatafulFoo<T> {
        return true;
    }
}
"#;

    let (parser, root) = parse_test_source(source);
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

    let ts2532: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2532)
        .collect();

    assert!(
        ts2532.is_empty(),
        "Expected no TS2532 after `this is DatafulFoo<T>` narrows `this`. \
         `this.data` should be `T`, not `T | undefined`. Got: {ts2532:#?}"
    );
}

/// `controlFlowAliasing.ts` C11: alias narrowing must survive a *later*
/// reassignment of a `readonly` auto-property (`this.x = 10` in the else
/// branch), but it must be invalidated for a parameter that is reassigned
/// elsewhere in the same scope.
#[test]
fn alias_narrowing_readonly_property_survives_later_assignment() {
    let source = r#"
class C11 {
    constructor(readonly x: string | number) {
        const thisX_isString = typeof this.x === 'string';
        const xIsString = typeof x === 'string';
        if (thisX_isString && xIsString) {
            let s: string;
            s = this.x; // OK: this.x is a constant reference (readonly)
            s = x;      // TS2322: x is reassigned later in the constructor
        }
        else {
            this.x = 10;
            x = 10;
        }
    }
}
"#;

    let (parser, root) = parse_test_source(source);
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

    let ts2322: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .collect();

    assert_eq!(
        ts2322.len(),
        1,
        "Expected exactly one TS2322 (on `s = x;`); the readonly property \
         narrowing on `s = this.x;` must NOT error. Got: {ts2322:#?}"
    );
}

/// `controlFlowAliasing.ts` f27: alias narrowing must NOT apply when the
/// captured chain steps through a *mutable* property, even if no assignment
/// is observed. tsc's `isConstantReference` rejects the chain because
/// `obj` on `outer` is not declared `readonly`.
#[test]
fn alias_narrowing_rejected_for_mutable_property_chain() {
    let source = r#"
function f27(outer: { obj: { kind: 'foo', foo: string } | { kind: 'bar', bar: number } }) {
    const isFoo = outer.obj.kind === 'foo';
    if (isFoo) {
        outer.obj.foo;  // TS2339: not narrowed, `obj` is mutable
    }
    else {
        outer.obj.bar;  // TS2339: not narrowed, `obj` is mutable
    }
}
"#;

    let (parser, root) = parse_test_source(source);
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

    let ts2339: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2339)
        .collect();

    assert_eq!(
        ts2339.len(),
        2,
        "Expected exactly two TS2339 from f27 (one per branch); the alias \
         narrowing must be invalidated because `outer.obj` is mutable. \
         Got: {ts2339:#?}"
    );
}

/// `controlFlowAliasing.ts` f26: alias narrowing applies through a chain
/// of `readonly` property accesses even when the same enclosing scope has
/// other mutating operations on unrelated state. The whole chain must be
/// constant for narrowing to apply.
#[test]
fn alias_narrowing_applies_for_readonly_property_chain() {
    let source = r#"
function f26(outer: { readonly obj: { kind: 'foo', foo: string } | { kind: 'bar', bar: number } }) {
    const isFoo = outer.obj.kind === 'foo';
    if (isFoo) {
        outer.obj.foo;  // OK: `obj` is readonly so the chain is constant
    }
    else {
        outer.obj.bar;  // OK: `obj` is readonly so the chain is constant
    }
}
"#;

    let (parser, root) = parse_test_source(source);
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

    let ts2339: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2339)
        .collect();

    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 in f26 (readonly chain narrows correctly), \
         got: {ts2339:#?}"
    );
}

/// Regression test: switch statement narrowing via destructured discriminant alias.
///
/// `const { kind } = obj; switch (kind) { case 'foo': obj.foo; }` should narrow
/// `obj` to the `{ kind: 'foo', foo: string }` branch — no TS2339 on `obj.foo`.
///
/// Fix: `switch_can_affect_reference` now checks `is_aliased_discriminant_switch_expr`
/// so that switch(alias) where `alias` is `const { kind } = obj` allows entry into
/// per-clause narrowing.
#[test]
fn test_switch_narrowing_via_destructured_discriminant_alias() {
    let source = r#"
function f(obj: { kind: 'foo', foo: string } | { kind: 'bar', bar: number }) {
    const { kind } = obj;
    switch (kind) {
        case 'foo': obj.foo; break;
        case 'bar': obj.bar; break;
    }
}
"#;

    let (parser, root) = parse_test_source(source);
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

    let ts2339: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2339)
        .collect();

    assert!(
        ts2339.is_empty(),
        "Expected no TS2339: switch(kind) should narrow obj via destructured discriminant alias, \
         got: {ts2339:#?}"
    );
}

#[test]
fn destructured_boolean_discriminant_truthiness_narrows_source_object() {
    let diagnostics = strict_diagnostics(
        r#"
function processResult(
    result: { ok: true; value: string } | { ok: false; error: string }
): string {
    const { ok } = result;
    if (ok) {
        return result.value;
    }
    return result.error;
}
"#,
    );

    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2339),
        "Expected destructured boolean discriminant to narrow source object, got: {diagnostics:#?}"
    );
}

#[test]
fn renamed_destructured_boolean_discriminant_truthiness_narrows_source_object() {
    let diagnostics = strict_diagnostics(
        r#"
function readState(
    state: { ready: true; payload: number } | { ready: false; reason: string }
) {
    const { ready: isReady } = state;
    if (isReady) {
        const payload: number = state.payload;
    } else {
        const reason: string = state.reason;
    }
}
"#,
    );

    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2339),
        "Expected renamed destructured discriminant to narrow source object, got: {diagnostics:#?}"
    );
}

#[test]
fn non_const_destructured_discriminant_truthiness_does_not_narrow_source_object() {
    let diagnostics = strict_diagnostics(
        r#"
function processResult(
    result: { ok: true; value: string } | { ok: false; error: string }
) {
    let { ok } = result;
    if (ok) {
        return result.value;
    }
    return result.error;
}
"#,
    );

    let ts2339_count = diagnostics.iter().filter(|(code, _)| *code == 2339).count();
    assert_eq!(
        ts2339_count, 2,
        "Expected non-const destructured discriminant not to narrow source object, got: {diagnostics:#?}"
    );
}

#[test]
fn destructured_discriminant_with_default_does_not_narrow_source_object() {
    let diagnostics = strict_diagnostics(
        r#"
function processResult(
    result: { ok: true; value: string } | { ok: false; error: string }
) {
    const { ok = true } = result;
    if (ok) {
        return result.value;
    }
    return result.error;
}
"#,
    );

    let ts2339_count = diagnostics.iter().filter(|(code, _)| *code == 2339).count();
    assert_eq!(
        ts2339_count, 2,
        "Expected defaulted destructured discriminant not to narrow source object, got: {diagnostics:#?}"
    );
}

/// Regression test: aliased condition with loose equality narrows discriminated union.
///
/// `const isFoo = kind == 'foo'; if (isFoo && obj.foo) { ... }` should narrow `obj`
/// to the `{ kind: 'foo', foo?: string }` branch — no TS2339 on `obj.foo`.
///
/// Fix: `discriminant_comparison` (and `literal_comparison`) are now also called for
/// loose equality `==` comparisons, not just strict `===`.
#[test]
fn test_aliased_loose_equality_condition_narrows_discriminant() {
    let source = r#"
function f(obj: { kind: 'foo', foo?: string } | { kind: 'bar', bar?: number }) {
    const { kind } = obj;
    const isFoo = kind == 'foo';
    if (isFoo && obj.foo) {
        let t: string = obj.foo;
    }
}
"#;

    let (parser, root) = parse_test_source(source);
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

    let ts2339: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2339)
        .collect();
    let ts2322: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .collect();

    assert!(
        ts2339.is_empty() && ts2322.is_empty(),
        "Expected no errors: aliased loose == condition should narrow discriminated union, \
         ts2339={ts2339:#?}, ts2322={ts2322:#?}"
    );
}

#[test]
fn filter_truthiness_callback_does_not_inherit_type_predicate_overload() {
    let diagnostics = strict_diagnostics_with_libs(
        r#"
const values: (number | null)[] = [1, null, 2];
const filtered: number[] = values.filter(x => !!x);
"#,
    );

    assert!(
        diagnostics.iter().any(|(code, _)| *code == 2322),
        "Expected TS2322 because `!!x` should not infer `x is number`, got: {diagnostics:#?}"
    );
}

#[test]
fn mapped_then_filter_truthiness_callback_does_not_inherit_type_predicate_overload() {
    let diagnostics = strict_diagnostics_with_libs(
        r#"
const values: (number | null)[] = [1, null, 2];
const mapped = values.map(x => x);
const filtered: number[] = mapped.filter(x => !!x);
"#,
    );

    assert!(
        diagnostics.iter().any(|(code, _)| *code == 2322),
        "Expected TS2322 after map/filter because `!!x` should not infer `x is number`, got: {diagnostics:#?}"
    );
}

#[test]
fn filter_null_comparison_callback_still_infers_type_predicate() {
    let diagnostics = strict_diagnostics_with_libs(
        r#"
const values: (number | null)[] = [1, null, 2];
const filtered: number[] = values.filter(x => x !== null);
"#,
    );

    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2322),
        "`x !== null` should infer `x is number` for filter, got: {diagnostics:#?}"
    );
}

#[test]
fn contextual_type_guard_assignment_requires_explicit_or_inferred_predicate() {
    let diagnostics = strict_diagnostics(
        r#"
const truthyGuard: (x: number | null) => x is number = x => !!x;
const nullGuard: (x: number | null) => x is number = x => x !== null;
"#,
    );

    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect();

    assert_eq!(
        ts2322.len(),
        1,
        "Expected exactly one TS2322 for `x => !!x`; `x => x !== null` should infer a predicate. Got: {diagnostics:#?}"
    );
}

#[test]
fn inferred_type_predicate_narrows_discriminated_union_via_arrow_body() {
    // TS 5.5+ inferred type predicates: an arrow function whose body is a
    // single discriminant comparison should be inferred as a type predicate
    // and narrow the discriminated union at the call site.
    let source = r#"
declare const foobar:
  | { type: "foo"; foo: number }
  | { type: "bar"; bar: string };

const foobarPred = (fb: typeof foobar) => fb.type === "foo";
if (foobarPred(foobar)) {
  foobar.foo;
}
"#;

    let diags = strict_diagnostics(source);
    let ts2339: Vec<&(u32, String)> = diags.iter().filter(|d| d.0 == 2339).collect();
    assert!(
        ts2339.is_empty(),
        "Inferred predicate should narrow `foobar` so `.foo` is allowed; ts2339={ts2339:#?}"
    );
}

#[test]
fn inferred_type_predicate_works_when_iteration_var_is_renamed() {
    // The inference rule must not depend on the spelling of the parameter.
    // Renaming `fb` -> `payload` must produce the same predicate.
    let source = r#"
declare const foobar:
  | { type: "foo"; foo: number }
  | { type: "bar"; bar: string };

const isFoo = (payload: typeof foobar) => payload.type === "foo";
if (isFoo(foobar)) {
  foobar.foo;
}
"#;

    let diags = strict_diagnostics(source);
    let ts2339: Vec<&(u32, String)> = diags.iter().filter(|d| d.0 == 2339).collect();
    assert!(
        ts2339.is_empty(),
        "Inferred predicate should be name-independent; ts2339={ts2339:#?}"
    );
}

#[test]
fn inferred_type_predicate_skipped_when_body_does_not_narrow() {
    // When the body cannot narrow the parameter (e.g. a constant boolean), no
    // predicate is inferred. The discriminated union must remain wide and
    // accessing a variant-only property should still error.
    let source = r#"
declare const foobar:
  | { type: "foo"; foo: number }
  | { type: "bar"; bar: string };

const alwaysTrue = (_fb: typeof foobar) => true;
if (alwaysTrue(foobar)) {
  foobar.foo; // should error - no narrowing
}
"#;

    let diags = strict_diagnostics(source);
    let ts2339: Vec<&(u32, String)> = diags.iter().filter(|d| d.0 == 2339).collect();
    assert!(
        !ts2339.is_empty(),
        "Without a narrowing body, .foo should still error on the union; diags={diags:#?}"
    );
}

#[test]
fn inferred_type_predicate_handles_block_body_with_single_return() {
    // A function with a block body that consists of just `return <guard>;`
    // is also eligible for predicate inference.
    let source = r#"
declare const foobar:
  | { type: "foo"; foo: number }
  | { type: "bar"; bar: string };

function isFooBlock(fb: typeof foobar) {
  return fb.type === "foo";
}
if (isFooBlock(foobar)) {
  foobar.foo;
}
"#;

    let diags = strict_diagnostics(source);
    let ts2339: Vec<&(u32, String)> = diags.iter().filter(|d| d.0 == 2339).collect();
    assert!(
        ts2339.is_empty(),
        "Block body with single return should also infer a predicate; ts2339={ts2339:#?}"
    );
}

#[test]
fn inferred_type_predicate_handles_simple_statements_before_return() {
    let source = r#"
function isString(value: unknown) {
  const ignored = 0;
  ignored;
  return typeof value === "string";
}

declare const flag: boolean;
let input: unknown = flag ? "text" : 1;

if (isString(input)) {
  const asString: string = input;
  const asNumber: number = input;

  asString;
  asNumber;
}
"#;

    let diags = strict_diagnostics(source);
    let ts2322: Vec<_> = diags.iter().filter(|(code, _)| *code == 2322).collect();
    assert!(
        !ts2322.iter().any(|(_, message)| {
            message.contains("Type 'unknown' is not assignable to type 'string'")
        }),
        "Inferred predicate should allow assigning input to string; diags={diags:#?}"
    );
    assert!(
        ts2322.len() == 1,
        "Expected only the remaining number assignment to fail; diags={diags:#?}"
    );
}

#[test]
fn inferred_type_predicate_rejects_non_final_return_path() {
    let source = r#"
function isString(value: unknown, flag: boolean) {
  if (flag) {
    return false;
  }
  return typeof value === "string";
}

declare const flag: boolean;
let input: unknown = flag ? "text" : 1;

if (isString(input, flag)) {
  const asString: string = input;
}
"#;

    let diags = strict_diagnostics(source);
    assert!(
        diags.iter().any(|(code, message)| {
            *code == 2322 && message.contains("Type 'unknown' is not assignable to type 'string'")
        }),
        "A block with an alternate return path must not infer a predicate; diags={diags:#?}"
    );
}

#[test]
fn inferred_type_predicate_handles_typeof_guard() {
    // `(x) => typeof x === "string"` should infer `x is string`.
    let source = r#"
const isString = (x: string | number) => typeof x === "string";
declare let v: string | number;
if (isString(v)) {
  const s: string = v;
}
"#;

    let diags = strict_diagnostics(source);
    let ts2322: Vec<&(u32, String)> = diags.iter().filter(|d| d.0 == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "typeof inference should narrow v to string; ts2322={ts2322:#?}"
    );
}

#[test]
fn inferred_type_predicate_handles_instanceof_object_guard() {
    let source = r#"
function isDate(x: object) {
  return x instanceof Date;
}

declare let value: object;
if (isDate(value)) {
  const date: Date = value;
}
"#;

    let diags = strict_diagnostics(source);
    assert!(
        diags.iter().all(|(code, _)| *code != 2322 && *code != 2740),
        "instanceof predicate inference should narrow object to Date; diags={diags:#?}"
    );
}

#[test]
fn inferred_type_predicate_handles_in_guard_through_non_null_assertion() {
    let source = r#"
type Foo = { foo: string };
type Bar = Foo & { bar: string };

function isBar(x: Foo | Bar | null) {
  return "bar" in x!;
}

declare let value: Foo | Bar;
if (isBar(value)) {
  const bar: Bar = value;
}
"#;

    let diags = strict_diagnostics(source);
    assert!(
        diags.iter().all(|(code, _)| *code != 2322 && *code != 2741),
        "in-operator predicate inference should narrow to the member-bearing type; diags={diags:#?}"
    );
}

#[test]
fn inferred_type_predicate_handles_class_methods() {
    let source = r#"
class Inferrer {
  isNumber(x: number | string) {
    return typeof x === "number";
  }
}

declare let value: number | string;
const inferrer = new Inferrer();
if (inferrer.isNumber(value)) {
  const numberValue: number = value;
} else {
  const stringValue: string = value;
}
"#;

    let diags = strict_diagnostics(source);
    let ts2322: Vec<_> = diags.iter().filter(|(code, _)| *code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "method predicate inference should narrow both branches; ts2322={ts2322:#?}"
    );
}

#[test]
fn inferred_type_predicate_handles_same_parameter_or_guards() {
    let source = r#"
function isNumberOrString(x: unknown) {
  return typeof x === "number" || typeof x === "string";
}

declare let value: unknown;
if (isNumberOrString(value)) {
  const primitive: number | string = value;
}
"#;

    let diags = strict_diagnostics(source);
    let ts2322: Vec<_> = diags.iter().filter(|(code, _)| *code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "OR guards for the same parameter should infer a union predicate; ts2322={ts2322:#?}"
    );
}

#[test]
fn inferred_type_predicate_allows_throwing_prefix_path() {
    let source = r#"
function assertAndPredicate(x: string | number | Date) {
  if (x instanceof Date) {
    throw new Error();
  }
  return typeof x === "string";
}

declare let value: string | number | Date;
if (assertAndPredicate(value)) {
  const stringValue: string = value;
}
"#;

    let diags = strict_diagnostics(source);
    let ts2322: Vec<_> = diags.iter().filter(|(code, _)| *code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "throw-only prefix paths should not block predicate inference; ts2322={ts2322:#?}"
    );
}

#[test]
fn inferred_type_predicate_handles_satisfies_boolean_wrapper() {
    let source = r#"
const numbers = [1, 2, null, 3].filter((x) => (x != null) satisfies boolean);
const accepted: number[] = numbers;
"#;

    let diags = strict_diagnostics(source);
    let ts2322: Vec<_> = diags.iter().filter(|(code, _)| *code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "satisfies boolean should not hide an inferable predicate from filter; ts2322={ts2322:#?}"
    );
}

#[test]
fn inferred_type_predicate_handles_safe_double_negation_truthiness() {
    let source = r#"
type Item = { value: string };
const items = [{ value: "a" }, undefined].filter((item) => !!item);
const accepted: Item[] = items;
"#;

    let diags = strict_diagnostics(source);
    let ts2322: Vec<_> = diags.iter().filter(|(code, _)| *code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "double-negation truthiness should infer when the falsy branch is only nullish; ts2322={ts2322:#?}"
    );
}

#[test]
fn inferred_type_predicate_rejects_number_double_negation_truthiness() {
    let source = r#"
const isTruthy = (x: number | null) => !!x;
declare let value: number | null;
if (isTruthy(value)) {
  const accepted: number = value;
}
"#;

    let diags = strict_diagnostics(source);
    assert!(
        diags.iter().any(|(code, message)| {
            *code == 2322
                && message.contains("Type 'number | null' is not assignable to type 'number'")
        }),
        "number|null truthiness must not infer because 0 makes the false branch non-nullish; diags={diags:#?}"
    );
}

#[test]
fn inferred_type_predicate_explicit_annotation_still_wins() {
    // When the user wrote a return type, we must NOT override their
    // intent with an inferred predicate. `: boolean` is an explicit choice.
    let source = r#"
declare const foobar:
  | { type: "foo"; foo: number }
  | { type: "bar"; bar: string };

const annotated = (fb: typeof foobar): boolean => fb.type === "foo";
if (annotated(foobar)) {
  foobar.foo; // should error - annotated boolean prevents predicate inference
}
"#;

    let diags = strict_diagnostics(source);
    let ts2339: Vec<&(u32, String)> = diags.iter().filter(|d| d.0 == 2339).collect();
    assert!(
        !ts2339.is_empty(),
        "Explicit `: boolean` annotation must suppress predicate inference; diags={diags:#?}"
    );
}
