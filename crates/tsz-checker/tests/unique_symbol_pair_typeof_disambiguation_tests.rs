//! A pair of *distinct* `unique symbol` operands in an assignability diagnostic
//! must be disambiguated to each side's `typeof <name>` form, matching tsc's
//! `getTypeNamesForErrorDisplay`.
//!
//! Both distinct unique symbols stringify to the bare `unique symbol` keyword
//! (`typeToString`), so without disambiguation the message reads the confusing
//! `Argument of type 'unique symbol' is not assignable to parameter of type
//! 'unique symbol'` — a type that looks unassignable to itself. tsc re-qualifies
//! each side to `typeof <name>` (the same rule that keeps a two-`unique symbol`
//! mismatch a TS2322 rather than TS2719). tsz now applies this in the shared
//! diagnostic pair-finalization (`finalize_pair_display_for_diagnostic`), so the
//! argument (TS2345) and nested-leaf surfaces are corrected together.
//!
//! Only fires when *both* operands are unique symbols; when one side is a wide
//! `symbol`, a string literal, or any other type, the two default names already
//! differ and tsc keeps the bare `unique symbol` — asserted by the negative
//! controls below. Verified against pinned typescript@7.0.2 with `--strict
//! --target es2024 --lib es2024`.
//!
//! Binder names vary across cases (anti-hardcoding): the behavior is structural,
//! not tied to any particular spelling.

fn compile_diagnostics(source: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_source_code_messages(source)
}

fn message(diags: &[(u32, String)], code: u32) -> String {
    diags
        .iter()
        .find(|(c, _)| *c == code)
        .map(|(_, m)| m.clone())
        .unwrap_or_else(|| panic!("expected TS{code}; got: {diags:?}"))
}

/// TS2345 argument, two distinct unique symbols: `typeof q` vs `typeof p`, not
/// `unique symbol` vs `unique symbol`.
#[test]
fn ts2345_two_unique_symbols_disambiguate_to_typeof_names() {
    let diags = compile_diagnostics(
        "declare const p: unique symbol;\ndeclare const q: unique symbol;\ndeclare function take(x: typeof p): void;\ntake(q);\n",
    );
    assert_eq!(
        message(&diags, 2345),
        "Argument of type 'typeof q' is not assignable to parameter of type 'typeof p'.",
    );
}

/// Anti-hardcoding: the disambiguation is structural, not tied to the `p`/`q`
/// spelling — renamed binders produce their own `typeof <name>` pair.
#[test]
fn ts2345_two_unique_symbols_disambiguation_is_binder_name_independent() {
    let diags = compile_diagnostics(
        "declare const alpha: unique symbol;\ndeclare const beta: unique symbol;\ndeclare function accept(x: typeof alpha): void;\naccept(beta);\n",
    );
    assert_eq!(
        message(&diags, 2345),
        "Argument of type 'typeof beta' is not assignable to parameter of type 'typeof alpha'.",
    );
}

/// Negative control: a wide `symbol` argument against a `unique symbol`
/// parameter keeps the bare `unique symbol` target — the two default names
/// (`symbol` vs `unique symbol`) already differ, so tsc does not disambiguate.
#[test]
fn wide_symbol_argument_to_unique_symbol_parameter_stays_unique_symbol() {
    let diags = compile_diagnostics(
        "declare const t: unique symbol;\ndeclare const w: symbol;\ndeclare function g(x: typeof t): void;\ng(w);\n",
    );
    assert_eq!(
        message(&diags, 2345),
        "Argument of type 'symbol' is not assignable to parameter of type 'unique symbol'.",
    );
}

/// Negative control: a string-literal source against a `unique symbol` target
/// keeps the bare `unique symbol` — only one operand is a unique symbol.
#[test]
fn string_literal_to_unique_symbol_target_stays_unique_symbol() {
    let diags =
        compile_diagnostics("declare const k: unique symbol;\nconst bad: typeof k = \"s\";\n");
    assert_eq!(
        message(&diags, 2322),
        "Type '\"s\"' is not assignable to type 'unique symbol'.",
    );
}
