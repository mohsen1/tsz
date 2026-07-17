//! #55: a `unique symbol` source must not display as `symbol` in a diagnostic.
//!
//! tsc's `typeToString` never widens a `unique symbol` to `symbol` — the source
//! of a failing assignment/argument renders as `unique symbol` (or `[unique
//! symbol]`), matching the type inference actually preserved. tsz previously
//! ran the *mutable-location* widening (`symbol`) in the diagnostic formatter,
//! so a correctly-inferred unique symbol printed as `symbol`. Fixed by teaching
//! the display wideners (`widen_type_for_display`,
//! `widen_type_preserving_unique_symbols`) to leave unique symbols intact.
//!
//! These witnesses all use `null` targets, where tsc renders the bare `unique
//! symbol` form. Verified against tsc 7.0.2. (The named `typeof cs` form for
//! primitive targets is a separate printer-naming gap, tracked independently.)

fn compile_diagnostics(source: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_source_code_messages(source)
}

fn code(diags: &[(u32, String)], want: u32) -> Vec<&str> {
    diags
        .iter()
        .filter_map(|(c, m)| (*c == want).then_some(m.as_str()))
        .collect()
}

#[test]
fn bare_unique_symbol_generic_call_source_is_unique_symbol_not_symbol() {
    // `idOne(cs)` infers `typeof cs`; the failing assignment to `null` must show
    // `unique symbol`, never `symbol`.
    let diags = compile_diagnostics(
        r#"
declare const cs: unique symbol;
declare function idOne<T>(x: T): T;
const v: null = idOne(cs);
"#,
    );
    let msgs = code(&diags, 2322);
    assert!(
        msgs.iter().any(|m| m.contains("Type 'unique symbol'")),
        "expected 'unique symbol' source, got: {msgs:?}"
    );
    assert!(
        !msgs.iter().any(|m| m.contains("Type 'symbol'")),
        "must not widen unique symbol to 'symbol', got: {msgs:?}"
    );
}

#[test]
fn direct_unique_symbol_source_is_unique_symbol_not_symbol() {
    let diags = compile_diagnostics(
        r#"
declare const cs: unique symbol;
const v: null = cs;
"#,
    );
    let msgs = code(&diags, 2322);
    assert!(
        msgs.iter().any(|m| m.contains("Type 'unique symbol'")),
        "expected 'unique symbol' source, got: {msgs:?}"
    );
}

#[test]
fn spread_rest_unique_symbol_tuple_source_is_unique_symbol_not_symbol() {
    // `spreadRest(...t)` infers `[typeof cs]`; the source must show
    // `[unique symbol]`, not `[symbol]`.
    let diags = compile_diagnostics(
        r#"
declare const cs: unique symbol;
declare const t: [typeof cs];
declare function spreadRest<Rest extends unknown[]>(...x: Rest): Rest;
const v: null = spreadRest(...t);
"#,
    );
    let msgs = code(&diags, 2322);
    assert!(
        msgs.iter().any(|m| m.contains("Type '[unique symbol]'")),
        "expected '[unique symbol]' source, got: {msgs:?}"
    );
    assert!(
        !msgs.iter().any(|m| m.contains("Type '[symbol]'")),
        "must not widen tuple unique symbol to '[symbol]', got: {msgs:?}"
    );
}

#[test]
fn unique_symbol_argument_source_is_unique_symbol_not_symbol() {
    // TS2345 argument display must also preserve the unique symbol.
    let diags = compile_diagnostics(
        r#"
declare const cs: unique symbol;
declare function takesString(x: string): void;
takesString(cs);
"#,
    );
    let msgs = code(&diags, 2345);
    assert!(
        msgs.iter()
            .any(|m| m.contains("Argument of type 'unique symbol'")),
        "expected 'unique symbol' argument source, got: {msgs:?}"
    );
}

#[test]
fn renamed_binders_still_render_unique_symbol() {
    // Different identifier spellings — proves the preservation is structural.
    let diags = compile_diagnostics(
        r#"
declare const alpha: unique symbol;
declare function pick<T>(x: T): T;
const v: null = pick(alpha);
"#,
    );
    let msgs = code(&diags, 2322);
    assert!(
        msgs.iter().any(|m| m.contains("Type 'unique symbol'")),
        "renamed binder must still render 'unique symbol', got: {msgs:?}"
    );
}

#[test]
fn fresh_array_literal_of_unique_symbol_still_widens_to_symbol_array() {
    // Protected negative: a *fresh* array literal `[cs]` genuinely widens its
    // element to `symbol` (semantic, matching tsc `symbol[]`), so the source is
    // `symbol[]` in both compilers — the display fix must not resurrect it.
    let diags = compile_diagnostics(
        r#"
declare const cs: unique symbol;
declare function idRest<Rest extends unknown[]>(x: Rest): Rest;
const v: null = idRest([cs]);
"#,
    );
    let msgs = code(&diags, 2322);
    assert!(
        msgs.iter().any(|m| m.contains("Type 'symbol[]'")),
        "fresh [cs] must still widen to 'symbol[]', got: {msgs:?}"
    );
}
