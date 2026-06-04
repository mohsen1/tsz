#[test]
fn satisfies_array_literal_elaborates_per_element() {
    // `[10, "20"] satisfies number[]` should elaborate per-element rather than
    // emitting a generic TS1360 on the whole expression. tsc emits TS2322 at
    // the offending `"20"` element with `Type 'string' is not assignable to
    // type 'number'.`, matching its `elaborateElementwise` behavior.
    //
    // Iteration variable / property names are deliberately varied across
    // assertions to avoid fingerprinting a specific spelling — the rule is
    // structural over array literal sources, not over specific identifiers.
    let diags = check_source_diagnostics(
        r#"
declare function take(...args: unknown[]): void;
take(10, ...([10, "20"] satisfies number[]));
take(10, ...([1, 2, "x", 4] satisfies number[]));
take(10, ...(([1, "wrapped"]) satisfies number[]));
take(10, ...(([1, "asserted"] as (number | string)[]) satisfies number[]));
"#,
    );

    // First satisfies has one bad element: "20" (string).
    // Second satisfies has one bad element: "x" (string).
    // The wrapped cases prove source unwrapping reaches the same array-literal
    // element path for parenthesized and asserted array sources.
    // Each source should emit TS2322 at the bad element, NOT TS1360 on the whole satisfies.
    let ts2322 = diagnostics_with_code(&diags, 2322);
    let ts1360 = diagnostics_with_code(&diags, 1360);

    assert_eq!(
        ts1360.len(),
        0,
        "Expected NO TS1360 generic-satisfies error; expected per-element TS2322 instead, got TS1360s: {:?}",
        diagnostic_messages(&ts1360)
    );
    assert_eq!(
        ts2322.len(),
        4,
        "Expected exactly 4 TS2322 elaborations (one per bad element), got: {:?}",
        diagnostic_messages(&ts2322)
    );
    for diag in &ts2322 {
        assert!(
            diag.message_text.contains("'string'") && diag.message_text.contains("'number'"),
            "Expected TS2322 message about string -> number, got: {}",
            diag.message_text
        );
    }
}

#[test]
fn satisfies_array_literal_all_elements_compatible_no_diagnostic() {
    // Sanity check: when every element of an array literal satisfies the
    // target's element type, no diagnostic should be reported. This guards
    // against the new array-elaboration path firing on assignable sources.
    let diags = check_source_diagnostics(
        r#"
declare function take(...args: unknown[]): void;
take(10, ...([1, 2, 3] satisfies number[]));
"#,
    );
    assert_eq!(
        diags.len(),
        0,
        "Expected no diagnostics for fully-compatible array literal, got: {:?}",
        diagnostic_summaries(&diags)
    );
}

#[test]
fn satisfies_result_type_is_assignable_to_target_literal_union() {
    // `"A" satisfies string` should have type `"A"` so it remains assignable to
    // a parameter of type `"A" | "B"`. Widening to `string` (the previous
    // behavior) would produce a false TS2345.
    let diags = check_source_diagnostics(
        r#"
declare function fn(s: "A" | "B"): void;
fn("A" satisfies string);
fn("C" satisfies string);
"#,
    );
    // First call should succeed; second should fail with TS2345 (string literal
    // "C" is not assignable to "A" | "B").
    let ts2345 = diagnostics_with_code(&diags, 2345);
    assert_eq!(
        ts2345.len(),
        1,
        "Expected exactly 1 TS2345 for the `\"C\"` call (not the `\"A\"` call), got: {:?}",
        diagnostic_messages(&ts2345)
    );
}

#[test]
fn ts2322_nested_generic_alias_two_levels() {
    // Box<Box<number>> should not be assignable to Box<Box<string>>
    let diags = check_source_diagnostics(
        r#"
type Box<T> = { value: T };
declare const x: Box<Box<number>>;
declare let y: Box<Box<string>>;
y = x;
"#,
    );
    let ts2322 = diagnostics_with_code(&diags, 2322);
    assert_eq!(
        ts2322.len(),
        1,
        "Expected 1 TS2322 for Box<Box<number>> vs Box<Box<string>>, got: {:?}",
        diagnostic_codes(&diags)
    );
}

#[test]
fn ts2322_nested_fn_alias_four_levels() {
    // Cb<Cb<Cb<Cb<number>>>> should not be assignable to Cb<Cb<Cb<Cb<string>>>>
    // where Cb<T> = {noAlias: () => T}["noAlias"]
    let diags = check_source_diagnostics(
        r#"
type Cb<T> = {noAlias: () => T}["noAlias"];
declare const x: Cb<Cb<Cb<Cb<number>>>>;
declare let y: Cb<Cb<Cb<Cb<string>>>>;
y = x;
"#,
    );
    let ts2322 = diagnostics_with_code(&diags, 2322);
    assert_eq!(
        ts2322.len(),
        1,
        "Expected 1 TS2322 for Cb<Cb<Cb<Cb<number>>>> vs Cb<Cb<Cb<Cb<string>>>>, got: {:?}",
        diagnostic_codes(&diags)
    );
    // Both source and target must be shown in structurally-expanded form.
    // tsc does not preserve alias names when the alias body is an IndexedAccess type.
    let msg = &ts2322[0].message_text;
    assert!(
        msg.contains("() => () => () => () => number"),
        "Expected source to expand to '() => () => () => () => number', got: {msg}"
    );
    assert!(
        msg.contains("() => () => () => () => string"),
        "Expected target to expand to '() => () => () => () => string', got: {msg}"
    );
}

#[test]
fn ts7023_no_false_positive_on_property_name_collision_assign() {
    // `Object.assign` inside an arrow body is a property name on the right of
    // a property access. The lexical `assign` variable is not referenced.
    let diags = check_source_diagnostics(
        r#"
const assign = <T, U>(a: T, b: U) => Object.assign(a, b);
"#,
    );
    let ts7023 = diagnostics_with_code(&diags, 7023);
    assert!(
        ts7023.is_empty(),
        "Expected no TS7023 for property-name collision with enclosing variable, got: {:?}",
        diagnostic_messages(&ts7023)
    );
}

#[test]
fn ts7023_no_false_positive_on_property_name_collision_alt_name() {
    // Same rule with a different variable name to prove the fix is structural,
    // not name-specific.
    let diags = check_source_diagnostics(
        r#"
const merge = <T, U>(a: T, b: U) => Object.merge(a, b);
declare namespace Object { function merge<A, B>(a: A, b: B): A & B; }
"#,
    );
    let ts7023 = diagnostics_with_code(&diags, 7023);
    assert!(
        ts7023.is_empty(),
        "Expected no TS7023 for `merge` colliding with property name, got: {:?}",
        diagnostic_messages(&ts7023)
    );
}

#[test]
fn ts7023_still_fires_on_genuine_self_reference() {
    // Sanity: a real recursive call inside a function-like initializer
    // without a return type annotation must still produce TS7023.
    let diags = check_source_diagnostics(
        r#"
const recur = (n: number) => recur(n);
"#,
    );
    let ts7023 = diagnostics_with_code(&diags, 7023);
    assert_eq!(
        ts7023.len(),
        1,
        "Expected TS7023 for genuine recursive arrow without return annotation, got: {:?}",
        diagnostic_codes(&diags)
    );
}
