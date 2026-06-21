//! Regression for TS2677 on a function-**type**-node type predicate whose
//! parameter or asserted type is written through a type alias.
//!
//! A type predicate `p is X` is valid when `X` is assignable to `p`'s declared
//! type. When either side is written through a type alias (`TypeData::Lazy(DefId)`
//! head, or a generic-alias `Application`), `tsc` resolves the alias to its body
//! and runs the relation structurally. tsz's function-type-node predicate check
//! (`check_type_predicate_assignability`) previously ran the relation with a
//! `NoopResolver`, so an aliased side stayed opaque and the relation spuriously
//! failed — a false-positive TS2677 (issue #14231).
//!
//! The fix threads the checker's `DefId`-resolving resolver into the relation so
//! both sides resolve during the relation walk, mirroring the function-declaration
//! and arrow-function paths, which already evaluate their inputs through the
//! `TypeEnvironment` before relating.
//!
//! These cases lock in:
//!   1. No spurious TS2677 when the parameter, the asserted type, or both are
//!      written through an alias.
//!   2. The rule is structural — renaming the alias/parameter must not change it.
//!   3. Generic-alias `Application` forms (`keyof`-bodied aliases) resolve too.
//!   4. Negative controls: a genuinely non-assignable predicate still errors,
//!      whether or not an alias is involved (no over-broad suppression).

use tsz_checker::context::CheckerOptions;

fn check(source: &str) -> Vec<(u32, String)> {
    let options = CheckerOptions {
        strict: true,
        ..Default::default()
    };
    tsz_checker::test_utils::check_source(source, "test.ts", options)
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

fn ts2677(diags: &[(u32, String)]) -> Vec<&(u32, String)> {
    diags.iter().filter(|(c, _)| *c == 2677).collect()
}

#[test]
fn function_type_predicate_aliased_parameter_type_is_clean() {
    let source = r#"
type A = string;
let g: (p: A) => p is string;
export {};
"#;
    let diags = check(source);
    assert!(
        ts2677(&diags).is_empty(),
        "An alias-typed parameter must resolve before the predicate relation: {diags:?}"
    );
}

#[test]
fn function_type_predicate_aliased_asserted_type_is_clean() {
    let source = r#"
type A = string;
let g: (p: string) => p is A;
export {};
"#;
    let diags = check(source);
    assert!(
        ts2677(&diags).is_empty(),
        "An alias-typed asserted type must resolve before the predicate relation: {diags:?}"
    );
}

#[test]
fn function_type_predicate_both_sides_aliased_is_clean() {
    let source = r#"
type A = string;
let g: (p: A) => p is A;
export {};
"#;
    let diags = check(source);
    assert!(
        ts2677(&diags).is_empty(),
        "Both sides aliased must resolve before the predicate relation: {diags:?}"
    );
}

#[test]
fn function_type_predicate_alias_resolution_is_structural_not_name_based() {
    // Renaming the alias and parameter must not change the verdict — the fix is
    // structural resolution, not a name match.
    let source = r#"
type Renamed = string;
let predicateHolder: (payload: Renamed) => payload is Renamed;
export {};
"#;
    let diags = check(source);
    assert!(
        ts2677(&diags).is_empty(),
        "Alias resolution must be independent of the alias/parameter names: {diags:?}"
    );
}

#[test]
fn function_type_predicate_generic_alias_application_is_clean() {
    // A generic-alias `Application` (`keyof`-bodied) on both sides resolves to
    // the same body and relates cleanly.
    let source = r#"
interface Shape { a: number; b: string; }
type Keys = keyof Shape;
let g: (p: Keys) => p is keyof Shape;
export {};
"#;
    let diags = check(source);
    assert!(
        ts2677(&diags).is_empty(),
        "A generic-alias keyof body must resolve before the predicate relation: {diags:?}"
    );
}

#[test]
fn function_type_predicate_alias_narrowing_to_literal_is_clean() {
    // `"a"` is assignable to `keyof Shape` (= "a" | "b") through the alias.
    let source = r#"
interface Shape { a: number; b: string; }
type Keys = keyof Shape;
let g: (p: Keys) => p is "a";
export {};
"#;
    let diags = check(source);
    assert!(
        ts2677(&diags).is_empty(),
        "A literal asserted type assignable to an aliased keyof must be clean: {diags:?}"
    );
}

#[test]
fn function_type_predicate_aliased_non_assignable_still_errors() {
    // Negative control: the alias resolves to a body that is genuinely NOT
    // assignable to the parameter type, so TS2677 must still fire.
    let source = r#"
type N = number;
let h: (p: string) => p is N;
export {};
"#;
    let diags = check(source);
    assert!(
        !ts2677(&diags).is_empty(),
        "A resolved-but-non-assignable predicate type must still report TS2677: {diags:?}"
    );
}

#[test]
fn function_type_predicate_unaliased_non_assignable_still_errors() {
    // Negative control with no alias at all — the existing behavior must hold.
    let source = r#"
let h: (p: string) => p is number;
export {};
"#;
    let diags = check(source);
    assert!(
        !ts2677(&diags).is_empty(),
        "A non-assignable predicate type must still report TS2677: {diags:?}"
    );
}
