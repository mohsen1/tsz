//! Regression tests for recursive template literal type patterns.
//!
//! Covers the class of patterns where a conditional type recursively
//! replaces characters in a string type using template literal inference.
//! The canonical example is `ReplaceAll<S, From, To>`:
//!
//! ```ts
//! type ReplaceAll<S extends string, From extends string, To extends string> =
//!   S extends `${infer L}${From}${infer R}` ? ReplaceAll<`${L}${To}${R}`, From, To> : S;
//! ```
//!
//! The underlying invariant: a string literal type separator like `"-"` inside
//! a template pattern (`${infer L}${"-"}${infer R}`) must be treated as a
//! fixed-text separator — not as a consecutive-type span that captures one
//! character. Prior to this fix, the `"-"` remained as a `Type` span in the
//! normalized template, causing the one-character capture rule to apply and
//! breaking iteration after the first substitution.

use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;
use tsz_common::common::{ModuleKind, ScriptTarget};

fn check(source: &str) -> Vec<Diagnostic> {
    tsz_checker::test_utils::check_source(
        source,
        "test.ts",
        CheckerOptions {
            target: ScriptTarget::ESNext,
            module: ModuleKind::CommonJS,
            ..CheckerOptions::default()
        },
    )
}

fn error_codes(diagnostics: &[Diagnostic]) -> Vec<u32> {
    diagnostics
        .iter()
        .filter(|d| d.category == tsz_checker::diagnostics::DiagnosticCategory::Error)
        .map(|d| d.code)
        .collect()
}

// ─── ReplaceAll: canonical two-argument form ───────────────────────────────

/// `ReplaceAll` with a dash separator — the original reported repro.
/// `"a-b-c"` should become `"a_b_c"`, not `"a_b-c"`.
#[test]
fn replace_all_dash_separator_three_segments() {
    let diags = check(
        r#"
type ReplaceAll<S extends string, From extends string, To extends string> =
  S extends `${infer L}${From}${infer R}` ? ReplaceAll<`${L}${To}${R}`, From, To> : S;
type T = ReplaceAll<"a-b-c", "-", "_">;
const _: T = "a_b_c";
"#,
    );
    assert!(
        error_codes(&diags).is_empty(),
        "ReplaceAll<\"a-b-c\", \"-\", \"_\"> should be \"a_b_c\"; got: {diags:#?}"
    );
}

/// Same type but with a DOT separator — proves the fix is not tied to `-`.
#[test]
fn replace_all_dot_separator_three_segments() {
    let diags = check(
        r#"
type ReplaceAll<S extends string, From extends string, To extends string> =
  S extends `${infer L}${From}${infer R}` ? ReplaceAll<`${L}${To}${R}`, From, To> : S;
type T = ReplaceAll<"a.b.c", ".", "/">;
const _: T = "a/b/c";
"#,
    );
    assert!(
        error_codes(&diags).is_empty(),
        "ReplaceAll<\"a.b.c\", \".\", \"/\"> should be \"a/b/c\"; got: {diags:#?}"
    );
}

/// The base case: string with no occurrence of `From`.
/// Must evaluate to the original string unchanged.
#[test]
fn replace_all_no_match_returns_original() {
    let diags = check(
        r#"
type ReplaceAll<S extends string, From extends string, To extends string> =
  S extends `${infer L}${From}${infer R}` ? ReplaceAll<`${L}${To}${R}`, From, To> : S;
type T = ReplaceAll<"abc", "-", "_">;
const _: T = "abc";
"#,
    );
    assert!(
        error_codes(&diags).is_empty(),
        "ReplaceAll<\"abc\", \"-\", \"_\"> should be \"abc\"; got: {diags:#?}"
    );
}

/// Exactly one occurrence — a single replacement, same as `Replace`.
#[test]
fn replace_all_single_occurrence() {
    let diags = check(
        r#"
type ReplaceAll<S extends string, From extends string, To extends string> =
  S extends `${infer L}${From}${infer R}` ? ReplaceAll<`${L}${To}${R}`, From, To> : S;
type T = ReplaceAll<"a-b", "-", "_">;
const _: T = "a_b";
"#,
    );
    assert!(
        error_codes(&diags).is_empty(),
        "ReplaceAll<\"a-b\", \"-\", \"_\"> should be \"a_b\"; got: {diags:#?}"
    );
}

/// Four segments — three replacements.
#[test]
fn replace_all_dash_separator_four_segments() {
    let diags = check(
        r#"
type ReplaceAll<S extends string, From extends string, To extends string> =
  S extends `${infer L}${From}${infer R}` ? ReplaceAll<`${L}${To}${R}`, From, To> : S;
type T = ReplaceAll<"a-b-c-d", "-", "_">;
const _: T = "a_b_c_d";
"#,
    );
    assert!(
        error_codes(&diags).is_empty(),
        "ReplaceAll<\"a-b-c-d\", \"-\", \"_\"> should be \"a_b_c_d\"; got: {diags:#?}"
    );
}

// ─── Replace: replace only the first occurrence ────────────────────────────

/// `Replace<S, From, To>` replaces the FIRST occurrence.
/// Verifies non-recursive variant still works after the normalisation change.
#[test]
fn replace_first_occurrence_only() {
    let diags = check(
        r#"
type Replace<S extends string, From extends string, To extends string> =
  S extends `${infer L}${From}${infer R}` ? `${L}${To}${R}` : S;
type T = Replace<"a-b-c", "-", "_">;
const _: T = "a_b-c";
"#,
    );
    assert!(
        error_codes(&diags).is_empty(),
        "Replace<\"a-b-c\", \"-\", \"_\"> should be \"a_b-c\"; got: {diags:#?}"
    );
}

// ─── Trim variants ─────────────────────────────────────────────────────────

/// `TrimLeft` removes leading spaces (regression-guard for the tail-call
/// optimisation that was already working and must remain working).
#[test]
fn trim_left_multiple_spaces() {
    let diags = check(
        r#"
type TrimLeft<S extends string> = S extends ` ${infer T}` ? TrimLeft<T> : S;
type T = TrimLeft<"   hello">;
const _: T = "hello";
"#,
    );
    assert!(
        error_codes(&diags).is_empty(),
        "TrimLeft<\"   hello\"> should be \"hello\"; got: {diags:#?}"
    );
}

/// `TrimRight` removes trailing spaces.
#[test]
fn trim_right_multiple_spaces() {
    let diags = check(
        r#"
type TrimRight<S extends string> = S extends `${infer T} ` ? TrimRight<T> : S;
type T = TrimRight<"hello   ">;
const _: T = "hello";
"#,
    );
    assert!(
        error_codes(&diags).is_empty(),
        "TrimRight<\"hello   \"> should be \"hello\"; got: {diags:#?}"
    );
}

// ─── Adjacent infer / string wildcard spans ────────────────────────────────

/// `${infer F}${string}${infer L}` should bind the first infer before the
/// string wildcard consumes its minimal character.
#[test]
fn infer_before_string_wildcard_captures_first_character() {
    let diags = check(
        r#"
type Test<T extends string> = T extends `${infer F}${string}${infer L}` ? [F, L] : never;
type T1 = Test<"hello">;
const result: T1 = ["h", "llo"];
    "#,
    );
    assert!(
        error_codes(&diags).is_empty(),
        "Test<\"hello\"> should be [\"h\", \"llo\"]; got: {diags:#?}"
    );
}

/// The same pattern needs at least two characters: one for the leading infer
/// and one for the `${string}` wildcard.
#[test]
fn infer_string_infer_pattern_rejects_single_character_source() {
    let diags = check(
        r#"
type Test<T extends string> = T extends `${infer F}${string}${infer L}` ? [F, L] : never;
type T1 = Test<"a">;
const result: T1 = ["", ""];
"#,
    );
    assert!(
        error_codes(&diags).contains(&2322),
        "Test<\"a\"> should be never and reject tuple assignment; got: {diags:#?}"
    );
}

// ─── Recursive path unions ─────────────────────────────────────────────────

/// `K extends ...` distributes over `keyof T`, so branch-local substitutions
/// must reach both `T[K]` and `${K}.${...}` to keep each key correlated with
/// its own value type.
#[test]
fn recursive_path_keeps_key_value_correlation_with_array_sibling() {
    let diags = check(
        r#"
type Path<T, K extends keyof T = keyof T> = K extends string | number
  ? T[K] extends infer V
    ? K | (V extends object ? `${K}.${Path<V>}` : never)
    : never
  : never;

interface WithArray {
  items: { name: string }[];
  meta: { count: number };
}

type WAPaths = Path<WithArray>;
const wp1: WAPaths = "items";
const wp2: WAPaths = "meta";
const wp3: WAPaths = "meta.count";
"#,
    );
    assert!(
        error_codes(&diags).is_empty(),
        "Path<WithArray> should include the non-array sibling path \"meta.count\"; got: {diags:#?}"
    );
}
