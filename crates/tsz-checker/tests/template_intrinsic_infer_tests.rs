//! Regression tests for inferring through an **intrinsic string mapping** in a
//! template-literal pattern: `S extends \`${Uppercase<infer P>}\` ? P : never`.
//!
//! Structural rule (owner: `match_intrinsic_span_from` in
//! `evaluation/evaluate_rules/infer_pattern_template_match.rs`): a template span
//! that is a string-intrinsic (`Uppercase`/`Lowercase`/`Capitalize`/
//! `Uncapitalize`) wrapping an `infer` matches only when the captured segment
//! is a **fixpoint** of the mapping (e.g. an already-uppercase run for
//! `Uppercase`), and infers the variable as `string` — the intrinsic's domain —
//! not the literal segment. So `"ABC" extends \`${Uppercase<infer P>}\` ? P :
//! never` is `string` (any string assignable), while a non-uppercase source
//! takes the false branch (`never`). Previously the span was unhandled and the
//! conditional wrongly collapsed to `never`, drawing a spurious TS2322.

use tsz_checker::test_utils::check_source_codes;

fn codes(source: &str) -> Vec<u32> {
    let mut c = check_source_codes(source);
    c.sort_unstable();
    c.dedup();
    c
}

#[test]
fn uppercase_infer_matches_uppercase_source() {
    // Result is `string` (the intrinsic domain): assigning any string is clean.
    assert!(
        codes(
            r#"
type X = "ABC" extends `${Uppercase<infer P>}` ? P : never;
const a: X = "ABC";
"#,
        )
        .is_empty(),
        "Uppercase<infer P> over an uppercase source should match",
    );
}

#[test]
fn uppercase_infer_binds_string_not_literal() {
    // Because P infers as `string` (not "ABC"), assigning a different string is
    // also clean — this distinguishes the intrinsic case from plain `infer P`.
    assert!(
        codes(
            r#"
type X = "ABC" extends `${Uppercase<infer P>}` ? P : never;
const a: X = "xyz";
"#,
        )
        .is_empty(),
        "intrinsic infer binds `string`, so any string is assignable",
    );
}

#[test]
fn uppercase_infer_nonmatching_source_takes_false_branch() {
    // A lowercase source is not a fixpoint of Uppercase, so X = never and the
    // assignment is rejected (TS2322).
    assert_eq!(
        codes(
            r#"
type X = "abc" extends `${Uppercase<infer P>}` ? P : never;
const a: X = "abc";
"#,
        ),
        vec![2322],
        "non-uppercase source should take the false branch (never)",
    );
}

#[test]
fn lowercase_infer_matches_lowercase_source() {
    assert!(
        codes(
            r#"
type X = "abc" extends `${Lowercase<infer P>}` ? P : never;
const a: X = "abc";
"#,
        )
        .is_empty(),
        "Lowercase<infer P> over a lowercase source should match",
    );
}

#[test]
fn capitalize_infer_matches_capitalized_source() {
    assert!(
        codes(
            r#"
type X = "Abc" extends `${Capitalize<infer P>}` ? P : never;
const a: X = "Abc";
"#,
        )
        .is_empty(),
        "Capitalize<infer P> over a capitalized source should match",
    );
}

#[test]
fn uppercase_infer_with_prefix_text_matches() {
    // The intrinsic span follows literal head text.
    assert!(
        codes(
            r#"
type X = "prefix-ABC" extends `prefix-${Uppercase<infer P>}` ? P : never;
const a: X = "ABC";
"#,
        )
        .is_empty(),
        "intrinsic infer after a text prefix should match",
    );
}

#[test]
fn uppercase_infer_generic_alias_form_matches() {
    // Not keyed on a particular alias/param name; works through a generic alias.
    assert!(
        codes(
            r#"
type Up<S extends string> = S extends `${Uppercase<infer P>}` ? P : never;
const a: Up<"ABC"> = "ABC";
"#,
        )
        .is_empty(),
        "generic-alias intrinsic infer should match",
    );
}

#[test]
fn plain_infer_still_binds_literal() {
    // Control: a *plain* `infer P` (no intrinsic) still binds the precise
    // literal, so assigning a different string errors — proving the new arm did
    // not loosen non-intrinsic template inference.
    assert_eq!(
        codes(
            r#"
type X = "ABC" extends `${infer P}` ? P : never;
const a: X = "ABC";
const b: X = "xyz";
"#,
        ),
        vec![2322],
        "plain infer P should still bind the literal (b: X = \"xyz\" errors)",
    );
}
