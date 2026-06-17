use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source, check_source_code_messages};

#[test]
fn typeof_type_literal_call_signature_parameter_uses_declared_type() {
    let source = r#"
function test1(a: number | string) {
  if (typeof a === "number") {
    const fn = (arg: typeof a) => true;
    return fn;
  }
  return;
}

test1(0)?.(100);
test1(0)?.("");

function test2(a: number | string) {
  if (typeof a === "number") {
    const fn: { (arg: typeof a): boolean; } = () => true;
    return fn;
  }
  return;
}

test2(0)?.(100);
test2(0)?.("");

function test3(a: number | string) {
  if (typeof a === "number") {
    return (arg: typeof a) => {};
  }
  throw "";
}

test3(1)(100);
test3(1)("");
"#;

    let diags = check_source(source, "test.ts", CheckerOptions::default());
    let ts2345: Vec<_> = diags.iter().filter(|d| d.code == 2345).collect();
    assert_eq!(
        ts2345.len(),
        2,
        "Expected TS2345 only for test1/test3 string calls, got: {diags:#?}"
    );

    let test1_string_arg =
        source.find("test1(0)?.(\"\")").unwrap() as u32 + "test1(0)?.(".len() as u32;
    let test2_string_arg =
        source.find("test2(0)?.(\"\")").unwrap() as u32 + "test2(0)?.(".len() as u32;
    let test3_string_arg = source.find("test3(1)(\"\")").unwrap() as u32 + "test3(1)(".len() as u32;

    assert!(
        ts2345.iter().any(|d| d.start == test1_string_arg),
        "Expected TS2345 on test1 string argument, got: {ts2345:#?}"
    );
    assert!(
        !ts2345.iter().any(|d| d.start == test2_string_arg),
        "Did not expect TS2345 on test2 explicit call-signature argument, got: {ts2345:#?}"
    );
    assert!(
        ts2345.iter().any(|d| d.start == test3_string_arg),
        "Expected TS2345 on test3 string argument, got: {ts2345:#?}"
    );
}

#[test]
fn typeof_expression_dispatches_to_literal_union() {
    let diagnostics = check_source_code_messages(
        r#"
declare const value: unknown;

let ok:
    | "string"
    | "number"
    | "bigint"
    | "boolean"
    | "symbol"
    | "undefined"
    | "object"
    | "function" = typeof value;

let bad: "string" = typeof value;
"#,
    );

    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect();
    assert_eq!(
        ts2322.len(),
        1,
        "`typeof value` must stay the literal-union result when routed through ExpressionDispatcher; got {diagnostics:?}"
    );
    assert!(
        ts2322[0].1.contains("\"number\""),
        "diagnostic should mention the literal-union source, got {ts2322:?}"
    );
}

/// Regression: `typeof x === "function"` must select the callable constituent
/// of a union that comes from an instantiated generic type alias. The alias
/// application (`F<string>`) is opaque until resolved; the function guard now
/// resolves it to its union body so the callable member is kept and the call
/// type-checks (matches tsc, which reports no error). Adjacent non-generic
/// alias and inline-union forms were already clean.
#[test]
fn typeof_function_selects_callable_member_of_instantiated_generic_alias() {
    let source = r#"
type StaleTime<T> = number | 'static' | ((q: T) => number);
declare const generic: StaleTime<string>;
declare const inline: number | 'static' | ((q: string) => number);
type StaleTimeConcrete = number | 'static' | ((q: string) => number);
declare const concrete: StaleTimeConcrete;

function useGeneric(q: string): number {
  if (typeof generic === 'function') {
    return generic(q);
  }
  return 0;
}

function useInline(q: string): number {
  if (typeof inline === 'function') {
    return inline(q);
  }
  return 0;
}

function useConcrete(q: string): number {
  if (typeof concrete === 'function') {
    return concrete(q);
  }
  return 0;
}
"#;
    let diags = check_source(source, "test.ts", CheckerOptions::default());
    let not_callable: Vec<_> = diags.iter().filter(|d| d.code == 2349).collect();
    assert!(
        not_callable.is_empty(),
        "typeof === 'function' must narrow the generic-alias union to its callable member; got: {diags:#?}"
    );
}

/// Regression (dual): the `typeof x !== "function"` else-branch must EXCLUDE
/// the callable constituent of an instantiated-generic-alias union, even when
/// the alias appears as a union member (`StaleTime<T> | undefined`). Without
/// resolving the alias the function constituent leaked into the narrowed type,
/// producing a spurious TS2322 on the non-callable return path.
#[test]
fn typeof_function_negative_branch_excludes_generic_alias_callable_member() {
    let source = r#"
type StaleTime<T> = number | 'static' | ((q: T) => number);

function resolve<T>(
  staleTime: StaleTime<T> | undefined,
  q: T,
): number | 'static' | undefined {
  return typeof staleTime === 'function' ? staleTime(q) : staleTime;
}
"#;
    let diags = check_source(source, "test.ts", CheckerOptions::default());
    let assignability: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2322 || d.code == 2349)
        .collect();
    assert!(
        assignability.is_empty(),
        "typeof !== 'function' else-branch must drop the alias callable member; got: {diags:#?}"
    );
}

/// Regression guard for the boxed global `Function` interface: narrowing
/// `Function | object` by `typeof === "function"` must keep the callable
/// `Function` constituent (not collapse to `never`). The non-union resolution
/// added for generic aliases deliberately does not resolve a Lazy reference to
/// the boxed `Function` interface, whose object shape would defeat the
/// callable-identity check.
#[test]
fn typeof_function_keeps_boxed_function_constituent() {
    let source = r#"
function f(instance: Function | object) {
  if (typeof instance === 'function') {
    return instance.length;
  }
  return 0;
}
"#;
    let diags = check_source(source, "test.ts", CheckerOptions::default());
    let property_missing: Vec<_> = diags.iter().filter(|d| d.code == 2339).collect();
    assert!(
        property_missing.is_empty(),
        "typeof === 'function' on `Function | object` must keep Function callable; got: {diags:#?}"
    );
}
