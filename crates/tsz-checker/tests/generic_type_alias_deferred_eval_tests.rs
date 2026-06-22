use tsz_checker::diagnostics::diagnostic_codes;
use tsz_checker::test_utils::check_source_diagnostics;

fn diagnostic_codes_for(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|diag| diag.code)
        .collect()
}

#[test]
fn generic_alias_declaration_does_not_expand_type_parameter_constraints() {
    let codes = diagnostic_codes_for(
        r#"
type Digit = "0" | "1" | "2" | "3" | "4" | "5" | "6";
type Deferred<T extends Digit> = `${T}${T}${T}${T}${T}${T}`;
"#,
    );

    assert!(
        !codes.contains(
            &diagnostic_codes::EXPRESSION_PRODUCES_A_UNION_TYPE_THAT_IS_TOO_COMPLEX_TO_REPRESENT
        ),
        "generic alias declaration should not eagerly expand the constrained type parameter; got {codes:?}"
    );
}

#[test]
fn concrete_generic_alias_instantiation_still_reports_too_complex_union() {
    let codes = diagnostic_codes_for(
        r#"
type Digit = "0" | "1" | "2" | "3" | "4" | "5" | "6";
type Deferred<T extends Digit> = `${T}${T}${T}${T}${T}${T}`;
type Use = Deferred<Digit>;
"#,
    );

    assert!(
        codes.contains(
            &diagnostic_codes::EXPRESSION_PRODUCES_A_UNION_TYPE_THAT_IS_TOO_COMPLEX_TO_REPRESENT
        ),
        "concrete generic alias instantiation should still report TS2590; got {codes:?}"
    );
}

#[test]
fn renamed_generic_alias_declaration_keeps_the_same_deferred_behavior() {
    let codes = diagnostic_codes_for(
        r#"
type Letter = "a" | "b" | "c" | "d" | "e" | "f" | "g";
type Boxed<X extends Letter> = { [Key in X]: `${Key}${Key}${Key}${Key}${Key}` };
"#,
    );

    assert!(
        !codes.contains(
            &diagnostic_codes::EXPRESSION_PRODUCES_A_UNION_TYPE_THAT_IS_TOO_COMPLEX_TO_REPRESENT
        ),
        "renamed generic alias declaration should not eagerly expand; got {codes:?}"
    );
}

// A mapped type whose per-key property value is itself a moderate union under
// the limit, distributed across enough keys that the cumulative member count
// exceeds tsc's union-complexity budget (`getUnionType`, ~100k), must report
// TS2590 — instead of materializing the oversized union (CPU-bound
// non-termination, #13508). `Cell` alone (22^3 = 10648) is under the limit, so
// the diagnostic is owed to the mapped *distribution* (22 * 10648 = 234_256),
// not the template expansion.
#[test]
fn mapped_distribution_exceeding_union_budget_reports_too_complex() {
    let codes = diagnostic_codes_for(
        r#"
type K = "a"|"b"|"c"|"d"|"e"|"f"|"g"|"h"|"i"|"j"|"k"|"l"|"m"|"n"|"o"|"p"|"q"|"r"|"s"|"t"|"u"|"v";
type Cell = `${K}-${K}-${K}`;
type Grid = { [P in K]: Cell };
"#,
    );

    assert!(
        codes.contains(
            &diagnostic_codes::EXPRESSION_PRODUCES_A_UNION_TYPE_THAT_IS_TOO_COMPLEX_TO_REPRESENT
        ),
        "mapped distribution past the union budget should report TS2590; got {codes:?}"
    );
}

// Same structural rule via index-access distribution: indexing a wide interface
// by `keyof` unions all property values. Each value (`Cell`, 8000 members) is
// under the limit, but the assembled `O[keyof O]` union (20 * 8000 = 160_000)
// exceeds it, so tsc — and now tsz — reports TS2590 rather than materializing
// it. Binders differ from the mapped case so the guard tracks the structural
// shape, not a spelling.
#[test]
fn index_access_distribution_exceeding_union_budget_reports_too_complex() {
    let codes = diagnostic_codes_for(
        r#"
type Tag = "a"|"b"|"c"|"d"|"e"|"f"|"g"|"h"|"i"|"j"|"k"|"l"|"m"|"n"|"o"|"p"|"q"|"r"|"s"|"t";
type Cell = `${Tag}_${Tag}_${Tag}`;
interface Bag {
  p0: Cell; p1: Cell; p2: Cell; p3: Cell; p4: Cell;
  p5: Cell; p6: Cell; p7: Cell; p8: Cell; p9: Cell;
  q0: Cell; q1: Cell; q2: Cell; q3: Cell; q4: Cell;
  q5: Cell; q6: Cell; q7: Cell; q8: Cell; q9: Cell;
}
type All = Bag[keyof Bag];
"#,
    );

    assert!(
        codes.contains(
            &diagnostic_codes::EXPRESSION_PRODUCES_A_UNION_TYPE_THAT_IS_TOO_COMPLEX_TO_REPRESENT
        ),
        "index-access distribution past the union budget should report TS2590; got {codes:?}"
    );
}

// Negative control: a mapped distribution that stays well under the budget must
// NOT trip TS2590 — the cap must only fire on genuinely oversized unions, not
// shrink the threshold for ordinary mapped types.
#[test]
fn mapped_distribution_within_union_budget_is_not_too_complex() {
    let codes = diagnostic_codes_for(
        r#"
type K = "a" | "b" | "c" | "d" | "e" | "f" | "g" | "h" | "i" | "j";
type Cell = `${K}-${K}`;
type Grid = { [P in K]: Cell };
"#,
    );

    assert!(
        !codes.contains(
            &diagnostic_codes::EXPRESSION_PRODUCES_A_UNION_TYPE_THAT_IS_TOO_COMPLEX_TO_REPRESENT
        ),
        "mapped distribution under the union budget must not report TS2590; got {codes:?}"
    );
}
