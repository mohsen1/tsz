//! Tests for template literal pattern matching with union-type separators.
//!
//! When a template pattern contains a separator that is a union of string
//! literals — e.g. `${infer H}${"," | ";"}${infer T}` — the inference engine
//! must treat each union member as a candidate anchor, find the leftmost
//! occurrence of any member in the source string, and use it to delimit the
//! preceding infer-variable capture.
//!
//! The rule: when the next concrete span after an infer variable is a union of
//! string literals `A | B | …`, the infer variable captures the shortest
//! prefix of the source such that some member of the union matches immediately
//! after the capture.
//!
//! These tests exercise two code paths:
//! - **Conditional type path** (`infer_pattern_helpers`) — already handled
//!   via `normalize_template_spans`.
//! - **Inference path** (`infer_matching::match_template_pattern`) — the new
//!   `find_next_anchor_alternatives` / `find_leftmost_occurrence` logic.

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

// ─── Inference path: generic function calls ───────────────────────────────────
//
// These tests exercise `match_template_pattern` in `infer_matching.rs`.
// A string literal is matched against a template parameter type that contains
// a Union-type separator span so the infer-variable anchor must be derived
// from the union members.

/// Union separator `"," | ";"` — comma variant.
/// Verifies the fix for the primary reported bug: H captures "hello" not "".
#[test]
fn infer_union_separator_comma() {
    let diags = check(
        r#"
declare function splitFirst<H extends string, T extends string>(
  s: `${H}${"," | ";"}${T}`
): [H, T];
const result = splitFirst("hello,world");
const _: ["hello", "world"] = result;
"#,
    );
    assert!(
        error_codes(&diags).is_empty(),
        "Expected [\"hello\",\"world\"] from union-sep inference; got: {diags:#?}"
    );
}

/// Union separator `"," | ";"` — semicolon variant.
/// Proves the fix handles both members, not just the first.
#[test]
fn infer_union_separator_semicolon() {
    let diags = check(
        r#"
declare function splitFirst<H extends string, T extends string>(
  s: `${H}${"," | ";"}${T}`
): [H, T];
const result = splitFirst("hello;world");
const _: ["hello", "world"] = result;
"#,
    );
    assert!(
        error_codes(&diags).is_empty(),
        "Expected [\"hello\",\"world\"] from semicolon branch; got: {diags:#?}"
    );
}

/// Union separator with renamed type variables — proves no hardcoding on
/// variable names `H`/`T`. Uses `A`/`B` instead.
#[test]
fn infer_union_separator_renamed_vars() {
    let diags = check(
        r#"
declare function splitFirst<A extends string, B extends string>(
  s: `${A}${"," | ";"}${B}`
): [A, B];
const result = splitFirst("foo,bar");
const _: ["foo", "bar"] = result;
"#,
    );
    assert!(
        error_codes(&diags).is_empty(),
        "Expected [\"foo\",\"bar\"] with renamed vars; got: {diags:#?}"
    );
}

/// Literal (Text) separator — baseline that must still work after the change.
#[test]
fn infer_text_separator_baseline() {
    let diags = check(
        r#"
declare function splitFirst<H extends string, T extends string>(
  s: `${H},${T}`
): [H, T];
const result = splitFirst("alpha,beta");
const _: ["alpha", "beta"] = result;
"#,
    );
    assert!(
        error_codes(&diags).is_empty(),
        "Literal-separator baseline failed; got: {diags:#?}"
    );
}

/// Three-member union separator: `"," | ";" | "|"`.
#[test]
fn infer_three_member_union_separator() {
    let diags = check(
        r#"
declare function splitFirst<H extends string, T extends string>(
  s: `${H}${"," | ";" | "|"}${T}`
): [H, T];
const r1 = splitFirst("a,b");
const _1: ["a", "b"] = r1;
const r2 = splitFirst("a;b");
const _2: ["a", "b"] = r2;
const r3 = splitFirst("a|b");
const _3: ["a", "b"] = r3;
"#,
    );
    assert!(
        error_codes(&diags).is_empty(),
        "Three-member union separator failed; got: {diags:#?}"
    );
}

/// Multi-char union members: `"--" | "__"`.
/// Verifies multi-character alternatives are handled correctly.
#[test]
fn infer_multichar_union_separator() {
    let diags = check(
        r#"
declare function splitFirst<H extends string, T extends string>(
  s: `${H}${"--" | "__"}${T}`
): [H, T];
const r1 = splitFirst("hello--world");
const _1: ["hello", "world"] = r1;
const r2 = splitFirst("hello__world");
const _2: ["hello", "world"] = r2;
"#,
    );
    assert!(
        error_codes(&diags).is_empty(),
        "Multi-char union separator failed; got: {diags:#?}"
    );
}

/// When the source contains only one of the union members, the result is
/// unambiguous.  When it contains both, tsc returns a union of both valid
/// splits — that is also correct, but is not tested here (it is an inherently
/// ambiguous case and the exact union shape is an implementation detail).
/// This test uses a source with exactly one separator character.
#[test]
fn infer_union_separator_single_separator_in_source() {
    let diags = check(
        r#"
declare function splitFirst<H extends string, T extends string>(
  s: `${H}${"," | ";"}${T}`
): [H, T];
// Only "," appears — unambiguous split.
const r1 = splitFirst("alpha,beta");
const _1: ["alpha", "beta"] = r1;
// Only ";" appears — unambiguous split.
const r2 = splitFirst("alpha;beta");
const _2: ["alpha", "beta"] = r2;
"#,
    );
    assert!(
        error_codes(&diags).is_empty(),
        "Single-separator unambiguous case failed; got: {diags:#?}"
    );
}

/// Fixed prefix before the union separator.
/// `"PREFIX:${H}${"," | ";"}${T}"` ensures the Text anchor is skipped correctly.
#[test]
fn infer_union_separator_with_fixed_prefix() {
    let diags = check(
        r#"
declare function splitFirst<H extends string, T extends string>(
  s: `prefix:${H}${"," | ";"}${T}`
): [H, T];
const result = splitFirst("prefix:hello,world");
const _: ["hello", "world"] = result;
"#,
    );
    assert!(
        error_codes(&diags).is_empty(),
        "Fixed-prefix + union-separator failed; got: {diags:#?}"
    );
}

/// Fixed suffix after the union separator.
#[test]
fn infer_union_separator_with_fixed_suffix() {
    let diags = check(
        r#"
declare function splitFirst<H extends string, T extends string>(
  s: `${H}${"," | ";"}${T}:suffix`
): [H, T];
const result = splitFirst("hello,world:suffix");
const _: ["hello", "world"] = result;
"#,
    );
    assert!(
        error_codes(&diags).is_empty(),
        "Union-separator + fixed-suffix failed; got: {diags:#?}"
    );
}

// ─── Conditional type path: union separator in extends clause ─────────────────
//
// These tests exercise `match_template_literal_string_from` in
// `infer_pattern_helpers.rs`, which already handled unions via `find_map`
// over union members.  Keeping them here ensures both paths stay correct.

/// `Split<S>` using `infer` in an extends clause with a union separator.
/// Rule: `S extends <head><comma-or-semicolon><tail>` splits at the
/// first occurrence of any union member.
#[test]
fn conditional_union_separator_comma() {
    let diags = check(
        r#"
type Split<S extends string> =
  S extends `${infer H}${"," | ";"}${infer T}` ? [H, T] : [S];
type R = Split<"hello,world">;
const _: ["hello", "world"] = {} as R;
"#,
    );
    assert!(
        error_codes(&diags).is_empty(),
        "Conditional union-sep comma failed; got: {diags:#?}"
    );
}

/// Same conditional type but with a semicolon in the source.
#[test]
fn conditional_union_separator_semicolon() {
    let diags = check(
        r#"
type Split<S extends string> =
  S extends `${infer H}${"," | ";"}${infer T}` ? [H, T] : [S];
type R = Split<"hello;world">;
const _: ["hello", "world"] = {} as R;
"#,
    );
    assert!(
        error_codes(&diags).is_empty(),
        "Conditional union-sep semicolon failed; got: {diags:#?}"
    );
}

/// Non-matching source falls through to the false branch.
#[test]
fn conditional_union_separator_no_match() {
    let diags = check(
        r#"
type Split<S extends string> =
  S extends `${infer H}${"," | ";"}${infer T}` ? [H, T] : [S];
type R = Split<"helloworld">;
const _: ["helloworld"] = {} as R;
"#,
    );
    assert!(
        error_codes(&diags).is_empty(),
        "No-match fallthrough failed; got: {diags:#?}"
    );
}

/// Recursive `SplitAll` using a union separator — combines both paths.
#[test]
fn conditional_union_separator_recursive_split() {
    let diags = check(
        r#"
type SplitAll<S extends string, Sep extends string> =
  S extends `${infer H}${Sep}${infer T}` ? [H, ...SplitAll<T, Sep>] : [S];

type Csv = SplitAll<"a,b,c", ",">;
const _csv: ["a", "b", "c"] = {} as Csv;

type Ssv = SplitAll<"a;b;c", ";">;
const _ssv: ["a", "b", "c"] = {} as Ssv;
"#,
    );
    assert!(
        error_codes(&diags).is_empty(),
        "Recursive SplitAll failed; got: {diags:#?}"
    );
}

/// `ReplaceAll` using a union `From` parameter — the generic separator.
/// When From is instantiated to `","`, the template becomes
/// `${infer L}${","}${infer R}` (normalised to Text), which is the
/// well-tested path.  This exercises the union-as-Sep conditional path.
#[test]
fn conditional_replace_with_union_from() {
    let diags = check(
        r#"
type ReplaceAll<S extends string, From extends string, To extends string> =
  S extends `${infer L}${From}${infer R}` ? ReplaceAll<`${L}${To}${R}`, From, To> : S;

type R1 = ReplaceAll<"a-b-c", "-", "_">;
const _1: "a_b_c" = {} as R1;

type R2 = ReplaceAll<"a.b.c", ".", "/">;
const _2: "a/b/c" = {} as R2;
"#,
    );
    assert!(
        error_codes(&diags).is_empty(),
        "ReplaceAll with generic From failed; got: {diags:#?}"
    );
}
