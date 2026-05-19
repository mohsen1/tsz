use crate::test_utils::check_source_codes;

#[test]
fn template_expr_contextual_type_no_false_positive() {
    // Template expression `\`${scope}:${event}\`` passed to a parameter expecting
    // a template literal type should NOT produce TS2345
    let source = r#"
type Registry = { a: { a1: {} }; b: { b1: {} } };
type Keyof<T> = keyof T & string;
declare function f1<
  Scope extends Keyof<Registry>,
  Event extends Keyof<Registry[Scope]>,
>(eventPath: `${Scope}:${Event}`): void;
function f2<
  Scope extends Keyof<Registry>,
  Event extends Keyof<Registry[Scope]>,
>(scope: Scope, event: Event) {
  f1(`${scope}:${event}`);
}
"#;
    let errors = check_source_codes(source);
    assert!(
        !errors.contains(&2345),
        "Should not emit TS2345 for template literal matching contextual type, got: {errors:?}"
    );
}

#[test]
fn template_expr_contextual_type_preserves_renamed_dependent_keys() {
    let source = r#"
type Catalog = { user: { created: {} }; team: { archived: {} } };
type StringKeys<T> = keyof T & string;
type Events<T extends StringKeys<Catalog>> = StringKeys<Catalog[T]>;
declare function route<
  Scope extends StringKeys<Catalog>,
  Event extends Events<Scope>,
>(path: `${Scope}/${Event}`): void;
function relay<
  Area extends StringKeys<Catalog>,
  Action extends Events<Area>,
>(area: Area, action: Action) {
  route(`${area}/${action}`);
}
"#;
    let errors = check_source_codes(source);
    assert!(
        !errors.contains(&2345),
        "Renamed dependent template keys should not emit TS2345, got: {errors:?}"
    );
}

#[test]
fn template_expr_contextual_type_reports_separator_mismatch() {
    let source = r#"
type Catalog = { user: { created: {} }; team: { archived: {} } };
type StringKeys<T> = keyof T & string;
type Events<T extends StringKeys<Catalog>> = StringKeys<Catalog[T]>;
declare function route<
  Scope extends StringKeys<Catalog>,
  Event extends Events<Scope>,
>(path: `${Scope}/${Event}`): void;
function relay<
  Area extends StringKeys<Catalog>,
  Action extends Events<Area>,
>(area: Area, action: Action) {
  route(`${area}:${action}`);
}
"#;
    let errors = check_source_codes(source);
    assert!(
        errors.contains(&2345),
        "Mismatched dependent template separators should still emit TS2345, got: {errors:?}"
    );
}

#[test]
fn generic_array_like_context_provides_element_type() {
    // When contextual type is a generic Application like ReadonlyArray<[K, V]>,
    // ensure the solver extracts the element type from the type arguments.
    // This exercises the Application -> evaluation path in get_array_element_type.
    // The full Iterable<readonly [K, V]> path (used by Map constructor) is
    // validated by conformance tests (for-of37, for-of40, for-of50) since it
    // requires Symbol.iterator from lib definitions.
    let source = r#"
interface ReadonlyArray<T> {
    readonly length: number;
    readonly [n: number]: T;
}
declare function f<K, V>(entries: ReadonlyArray<readonly [K, V]>): [K, V];
const r = f([["", true]]);
"#;
    let errors = check_source_codes(source);
    let semantic_errors: Vec<_> = errors.into_iter().filter(|&c| c != 2318).collect();
    assert!(
        !semantic_errors.contains(&2345) && !semantic_errors.contains(&2769),
        "ReadonlyArray<readonly [K, V]> should contextually type array elements as tuples, got: {semantic_errors:?}"
    );
}

#[test]
fn array_param_context_still_works() {
    // Ensure the fix doesn't break the already-working array parameter path.
    // When the parameter is a plain array type (readonly (readonly [K, V])[]),
    // contextual typing should still work without needing the fallback.
    let source = r#"
declare function f<K, V>(entries: readonly (readonly [K, V])[]): [K, V];
const result = f([["", true]]);
"#;
    let errors = check_source_codes(source);
    let semantic_errors: Vec<_> = errors.into_iter().filter(|&c| c != 2318).collect();
    assert!(
        !semantic_errors.contains(&2345) && !semantic_errors.contains(&2769),
        "Array parameter should contextually type elements as tuples, got: {semantic_errors:?}"
    );
}

#[test]
fn generic_iterable_context_preserves_heterogeneous_entries_for_type_mismatch() {
    let source = r#"
declare function f<K, V>(entries: readonly (readonly [K, V])[]): [K, V];
const result = f([["", true], ["", 0]]);
"#;
    let errors = check_source_codes(source);
    let semantic_errors: Vec<_> = errors.into_iter().filter(|&c| c != 2318).collect();
    // tsc emits TS2322 ("Type 'number' is not assignable to type 'boolean'.")
    // on the inner element when V is inferred from the first entry
    // and the second entry's V mismatches. Earlier we incorrectly
    // surfaced TS2345 on the whole array argument because element-wise
    // elaboration was suppressed for any call argument targeting a
    // generic parameter; we now elaborate when the resolved target
    // element type is concrete.
    assert!(
        semantic_errors.contains(&2322),
        "Heterogeneous generic entries should produce TS2322 element elaboration, got: {semantic_errors:?}"
    );
}

#[test]
fn template_expr_without_context_stays_string() {
    // Template expression assigned to `string` should still work (not break)
    let source = r#"
function f(x: string, y: number): string {
    return `${x} is ${y}`;
}
"#;
    let errors = check_source_codes(source);
    // Filter out TS2318 (lib not found) since test env has no lib definitions
    let semantic_errors: Vec<_> = errors.into_iter().filter(|&c| c != 2318).collect();
    assert!(
        semantic_errors.is_empty(),
        "Template expression returning string should produce no semantic errors, got: {semantic_errors:?}"
    );
}

/// Issue #2871: a local function named `Symbol` must not be treated as
/// the lib global `Symbol`. The const initializer should keep the local
/// function's return type (`string`) instead of being inferred as
/// `unique symbol`. Without the fix, the TS2322 lands on `asString`
/// instead of `asSymbol`.
#[test]
fn shadowed_symbol_call_keeps_local_return_type() {
    let source = r#"
function test() {
    const Symbol = () => "local";
    const value = Symbol();
    const asSymbol: symbol = value;
    const asString: string = value;
    asSymbol;
    asString;
}
"#;
    let codes = check_source_codes(source);
    let ts2322_count = codes.iter().filter(|&&c| c == 2322).count();
    assert_eq!(
        ts2322_count, 1,
        "Expected exactly one TS2322 (string->symbol on asSymbol), got: {codes:?}"
    );
}

/// Issue #2871: same rule, different declaration kind. A local
/// `function Symbol(): "outer"` shadows the global, so the const
/// initializer's type must come from the local return type, not the
/// global `Symbol()` special case.
#[test]
fn shadowed_symbol_call_function_decl_not_unique_symbol() {
    let source = r#"
function outer() {
    function Symbol(): "outer" { return "outer"; }
    const value = Symbol();
    const taken: symbol = value;
    taken;
}
"#;
    let codes = check_source_codes(source);
    assert!(
        codes.contains(&2322),
        "Expected TS2322 for string->symbol via shadowed Symbol(), got: {codes:?}"
    );
}
